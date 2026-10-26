//! The model catalog, hardware profiles, and inference rows.

use sqlx::Row;
use uuid::Uuid;

use super::super::TenantScope;
use crate::domain::model::{GpuDevice, HardwareProfile, Model, ModelState};
use crate::error::StoreError;

const MODEL_COLUMNS: &str = "id, key, name, family, parameters, context_length, quantization, \
     weights_bytes, min_vram_bytes, min_gpu_count, source_uri, version, \
     installed_version, state, installed_at, is_remote, provider_key";

fn row_to_model(row: &sqlx::postgres::PgRow) -> Result<Model, StoreError> {
    let state_raw: String = row.try_get("state")?;
    Ok(Model {
        id: row.try_get("id")?,
        key: row.try_get("key")?,
        name: row.try_get("name")?,
        family: row.try_get("family")?,
        parameters: row.try_get("parameters")?,
        context_length: row.try_get("context_length")?,
        quantization: row.try_get("quantization")?,
        weights_bytes: row.try_get("weights_bytes")?,
        min_vram_bytes: row.try_get("min_vram_bytes")?,
        min_gpu_count: row.try_get("min_gpu_count")?,
        source_uri: row.try_get("source_uri")?,
        version: row.try_get("version")?,
        installed_version: row.try_get("installed_version")?,
        state: ModelState::parse(&state_raw).unwrap_or(ModelState::NotInstalled),
        installed_at: row.try_get("installed_at")?,
        is_remote: row.try_get("is_remote")?,
        provider_key: row.try_get("provider_key")?,
    })
}

/// The catalog: entries visible to this environment plus global ones.
///
/// `environment_id IS NULL` marks a catalog entry shared by every environment,
/// which is the normal case for the seeded Nemotron list.
pub async fn list_catalog(
    scope: &mut TenantScope<'_>,
) -> Result<Vec<Model>, StoreError> {
    let sql = format!(
        "SELECT {MODEL_COLUMNS} FROM models
          WHERE environment_id IS NULL OR environment_id = $1
          ORDER BY min_vram_bytes NULLS LAST, name"
    );
    let rows = sqlx::query(&sql)
        .bind(scope.environment_id())
        .fetch_all(scope.connection())
        .await?;
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        out.push(row_to_model(&row)?);
    }
    Ok(out)
}

/// One catalog entry by key.
pub async fn get_by_key(
    scope: &mut TenantScope<'_>,
    key: &str,
) -> Result<Model, StoreError> {
    let sql = format!(
        "SELECT {MODEL_COLUMNS} FROM models
          WHERE key = $1 AND (environment_id IS NULL OR environment_id = $2)"
    );
    let row = sqlx::query(&sql)
        .bind(key)
        .bind(scope.environment_id())
        .fetch_optional(scope.connection())
        .await?
        .ok_or(StoreError::NotFound)?;
    row_to_model(&row)
}

/// Only the installed models.
pub async fn list_installed(scope: &mut TenantScope<'_>) -> Result<Vec<Model>, StoreError> {
    let sql = format!(
        "SELECT {MODEL_COLUMNS} FROM models
          WHERE (environment_id IS NULL OR environment_id = $1)
            AND state IN ('installed', 'updating')
          ORDER BY name"
    );
    let rows = sqlx::query(&sql)
        .bind(scope.environment_id())
        .fetch_all(scope.connection())
        .await?;
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        out.push(row_to_model(&row)?);
    }
    Ok(out)
}

/// Record a state transition on a model.
pub async fn set_state(
    scope: &mut TenantScope<'_>,
    key: &str,
    state: ModelState,
    installed_version: Option<&str>,
) -> Result<Model, StoreError> {
    let sql = format!(
        "WITH updated AS (
            UPDATE models
               SET state = $3,
                   installed_version = COALESCE($4, installed_version),
                   installed_at = CASE WHEN $3 = 'installed' THEN now() ELSE installed_at END,
                   updated_at = now()
             WHERE key = $1 AND (environment_id IS NULL OR environment_id = $2)
         RETURNING {MODEL_COLUMNS}
         )
         SELECT {MODEL_COLUMNS} FROM updated"
    );
    let row = sqlx::query(&sql)
        .bind(key)
        .bind(scope.environment_id())
        .bind(state.as_str())
        .bind(installed_version)
        .fetch_optional(scope.connection())
        .await?
        .ok_or(StoreError::NotFound)?;
    row_to_model(&row)
}

/// Store a hardware detection result.
pub async fn insert_hardware(
    scope: &mut TenantScope<'_>,
    profile: &HardwareProfile,
) -> Result<Uuid, StoreError> {
    let gpus = serde_json::to_value(&profile.gpus).unwrap_or(serde_json::Value::Array(vec![]));
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO hardware_profiles
            (environment_id, hostname, gpus, total_vram_bytes, unified_memory_bytes,
             cuda_version, driver_version, detection_note)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
         RETURNING id",
    )
    .bind(scope.environment_id())
    .bind(profile.hostname.as_deref())
    .bind(gpus)
    .bind(profile.total_vram_bytes)
    .bind(profile.unified_memory_bytes)
    .bind(profile.cuda_version.as_deref())
    .bind(profile.driver_version.as_deref())
    .bind(profile.detection_note.as_deref())
    .fetch_one(scope.connection())
    .await?;
    Ok(id)
}

/// The most recent detection result, if any.
pub async fn latest_hardware(
    scope: &mut TenantScope<'_>,
) -> Result<Option<HardwareProfile>, StoreError> {
    let row = sqlx::query(
        "SELECT id, environment_id, detected_at, hostname, gpus, total_vram_bytes,
                unified_memory_bytes, cuda_version, driver_version, detection_note
           FROM hardware_profiles
          WHERE environment_id = $1
          ORDER BY detected_at DESC
          LIMIT 1",
    )
    .bind(scope.environment_id())
    .fetch_optional(scope.connection())
    .await?;

    let Some(row) = row else { return Ok(None) };

    let gpus_raw: serde_json::Value = row.try_get("gpus")?;
    let gpus: Vec<GpuDevice> = serde_json::from_value(gpus_raw).unwrap_or_default();

    Ok(Some(HardwareProfile {
        id: row.try_get("id")?,
        environment_id: row.try_get("environment_id")?,
        detected_at: row.try_get("detected_at")?,
        hostname: row.try_get("hostname")?,
        gpus,
        total_vram_bytes: row.try_get("total_vram_bytes")?,
        unified_memory_bytes: row.try_get("unified_memory_bytes")?,
        cuda_version: row.try_get("cuda_version")?,
        driver_version: row.try_get("driver_version")?,
        detection_note: row.try_get("detection_note")?,
    }))
}
