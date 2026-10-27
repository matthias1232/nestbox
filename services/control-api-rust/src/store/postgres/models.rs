//! The model catalog, hardware profiles, and inference rows.

use sqlx::Row;
use uuid::Uuid;

use super::super::TenantScope;
use crate::domain::model::{GpuDevice, HardwareProfile, Model, ModelRuntime, ModelState};
use crate::error::StoreError;

const MODEL_COLUMNS: &str = "id, key, name, family, parameters, context_length, quantization, \
     weights_bytes, min_vram_bytes, recommended_vram_bytes, min_gpu_count, min_ram_bytes, \
     recommended_ram_bytes, min_cpu_cores, recommended_cpu_cores, runtime, arch, \
     artifact_format, publisher, license, description, unified_memory_only, is_cpu_only, \
     source_uri, version, installed_version, state, installed_at, is_remote, provider_key";

fn row_to_model(row: &sqlx::postgres::PgRow) -> Result<Model, StoreError> {
    let state_raw: String = row.try_get("state")?;
    let runtime_raw: String = row.try_get("runtime")?;
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
        recommended_vram_bytes: row.try_get("recommended_vram_bytes")?,
        min_gpu_count: row.try_get("min_gpu_count")?,
        min_ram_bytes: row.try_get("min_ram_bytes")?,
        recommended_ram_bytes: row.try_get("recommended_ram_bytes")?,
        min_cpu_cores: row.try_get("min_cpu_cores")?,
        recommended_cpu_cores: row.try_get("recommended_cpu_cores")?,
        // An unreadable runtime means a row written by an older schema version;
        // treating it as `cuda` keeps the old GPU gate instead of silently
        // widening what the machine may install.
        runtime: ModelRuntime::parse(&runtime_raw).unwrap_or(ModelRuntime::Cuda),
        arch: row.try_get("arch")?,
        artifact_format: row.try_get("artifact_format")?,
        publisher: row.try_get("publisher")?,
        license: row.try_get("license")?,
        description: row.try_get("description")?,
        unified_memory_only: row.try_get("unified_memory_only")?,
        is_cpu_only: row.try_get("is_cpu_only")?,
        source_uri: row.try_get("source_uri")?,
        version: row.try_get("version")?,
        installed_version: row.try_get("installed_version")?,
        state: ModelState::parse(&state_raw).unwrap_or(ModelState::NotInstalled),
        installed_at: row.try_get("installed_at")?,
        is_remote: row.try_get("is_remote")?,
        provider_key: row.try_get("provider_key")?,
    })
}

/// Which catalog rows to load.
///
/// Filtering here, in SQL, keeps the parameter list small and the rows returned
/// to what a caller asked for. The fit filter is *not* here: the verdict is a
/// classification, so it is applied after classification in the service layer,
/// where the one rule lives.
#[derive(Debug, Clone, Default)]
pub struct CatalogFilter {
    pub family: Option<String>,
    pub publisher: Option<String>,
    pub runtime: Option<String>,
    /// Only models that run without a GPU.
    pub cpu_only: bool,
    /// Only models that can never run on a discrete card.
    pub unified_only: bool,
}

/// The catalog: entries visible to this environment plus global ones.
///
/// `environment_id IS NULL` marks a catalog entry shared by every environment,
/// which is the normal case for the seeded list.
pub async fn list_catalog(
    scope: &mut TenantScope<'_>,
    filter: &CatalogFilter,
) -> Result<Vec<Model>, StoreError> {
    let sql = format!(
        "SELECT {MODEL_COLUMNS} FROM models
          WHERE (environment_id IS NULL OR environment_id = $1)
            AND ($2::text IS NULL OR family = $2)
            AND ($3::text IS NULL OR publisher = $3)
            AND ($4::text IS NULL OR runtime = $4)
            AND (NOT $5::boolean OR is_cpu_only)
            AND (NOT $6::boolean OR unified_memory_only)
          ORDER BY min_vram_bytes NULLS LAST, name"
    );
    let rows = sqlx::query(&sql)
        .bind(scope.environment_id())
        .bind(filter.family.as_deref())
        .bind(filter.publisher.as_deref())
        .bind(filter.runtime.as_deref())
        .bind(filter.cpu_only)
        .bind(filter.unified_only)
        .fetch_all(scope.connection())
        .await?;
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        out.push(row_to_model(&row)?);
    }
    Ok(out)
}

/// How many catalog entries exist per family, publisher, runtime, and in total.
///
/// The console shows these numbers on the filter chips, so a chip never offers a
/// choice that matches nothing.
pub async fn catalog_facets(
    scope: &mut TenantScope<'_>,
) -> Result<ModelFacets, StoreError> {
    let rows = sqlx::query(
        "SELECT family, publisher, runtime, is_cpu_only, unified_memory_only
           FROM models
          WHERE environment_id IS NULL OR environment_id = $1",
    )
    .bind(scope.environment_id())
    .fetch_all(scope.connection())
    .await?;

    let mut facets = ModelFacets::default();
    for row in rows {
        facets.total += 1;
        if let Some(family) = row.try_get::<Option<String>, _>("family")? {
            *facets.families.entry(family).or_insert(0) += 1;
        }
        if let Some(publisher) = row.try_get::<Option<String>, _>("publisher")? {
            *facets.publishers.entry(publisher).or_insert(0) += 1;
        }
        if let Some(runtime) = row.try_get::<Option<String>, _>("runtime")? {
            *facets.runtimes.entry(runtime).or_insert(0) += 1;
        }
        if row.try_get::<bool, _>("is_cpu_only")? {
            facets.cpu_only += 1;
        }
        if row.try_get::<bool, _>("unified_memory_only")? {
            facets.unified_only += 1;
        }
    }
    Ok(facets)
}

/// Counts per catalog dimension.
#[derive(Debug, Clone, Default)]
pub struct ModelFacets {
    pub total: i64,
    pub families: std::collections::BTreeMap<String, i64>,
    pub publishers: std::collections::BTreeMap<String, i64>,
    pub runtimes: std::collections::BTreeMap<String, i64>,
    pub cpu_only: i64,
    pub unified_only: i64,
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
             unified_memory_source, cuda_version, driver_version, detection_note,
             cpu_model, cpu_cores, cpu_threads, system_ram_bytes, node_id)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)
         RETURNING id",
    )
    .bind(scope.environment_id())
    .bind(profile.hostname.as_deref())
    .bind(gpus)
    .bind(profile.total_vram_bytes)
    .bind(profile.unified_memory_bytes)
    .bind(profile.unified_memory_source.as_deref())
    .bind(profile.cuda_version.as_deref())
    .bind(profile.driver_version.as_deref())
    .bind(profile.detection_note.as_deref())
    .bind(profile.cpu_model.as_deref())
    .bind(profile.cpu_cores)
    .bind(profile.cpu_threads)
    .bind(profile.system_ram_bytes)
    .bind(profile.node_id)
    .fetch_one(scope.connection())
    .await?;
    Ok(id)
}

/// The most recent detection result for a node, if any.
pub async fn latest_hardware(
    scope: &mut TenantScope<'_>,
    node_id: Uuid,
) -> Result<Option<HardwareProfile>, StoreError> {
    let row = sqlx::query(
        "SELECT id, environment_id, node_id, detected_at, hostname, gpus, total_vram_bytes,
                unified_memory_bytes, unified_memory_source, cuda_version,
                driver_version, detection_note, cpu_model, cpu_cores, cpu_threads,
                system_ram_bytes
           FROM hardware_profiles
          WHERE environment_id = $1 AND node_id = $2
          ORDER BY detected_at DESC
          LIMIT 1",
    )
    .bind(scope.environment_id())
    .bind(node_id)
    .fetch_optional(scope.connection())
    .await?;

    let Some(row) = row else { return Ok(None) };

    let gpus_raw: serde_json::Value = row.try_get("gpus")?;
    let gpus: Vec<GpuDevice> = serde_json::from_value(gpus_raw).unwrap_or_default();

    Ok(Some(HardwareProfile {
        id: row.try_get("id")?,
        environment_id: row.try_get("environment_id")?,
        node_id: row.try_get("node_id")?,
        detected_at: row.try_get("detected_at")?,
        hostname: row.try_get("hostname")?,
        gpus,
        total_vram_bytes: row.try_get("total_vram_bytes")?,
        unified_memory_bytes: row.try_get("unified_memory_bytes")?,
        unified_memory_source: row.try_get("unified_memory_source")?,
        cuda_version: row.try_get("cuda_version")?,
        driver_version: row.try_get("driver_version")?,
        detection_note: row.try_get("detection_note")?,
        cpu_model: row.try_get("cpu_model")?,
        cpu_cores: row.try_get("cpu_cores")?,
        cpu_threads: row.try_get("cpu_threads")?,
        system_ram_bytes: row.try_get("system_ram_bytes")?,
    }))
}
