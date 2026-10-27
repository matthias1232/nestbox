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

/// A catalogue row as a repository stated it, ready to be written.
///
/// This lives in the store rather than in `domain` because it is the shape of a
/// write to `models`, not a concept the rest of the system reasons about: it
/// carries every column the sync fills and nothing else. The gate numbers have
/// already been derived when a row reaches here, so the store never computes a
/// requirement.
#[derive(Debug, Clone)]
pub struct SyncedModel {
    /// One of the values the `models.source` check allows.
    pub source: String,
    /// The identifier the source uses, unique within that source.
    pub source_id: String,
    pub key: String,
    pub name: String,
    pub family: String,
    pub parameters: String,
    pub context_length: Option<i64>,
    pub quantization: Option<String>,
    pub weights_bytes: Option<i64>,
    pub min_vram_bytes: Option<i64>,
    pub recommended_vram_bytes: Option<i64>,
    pub min_gpu_count: i32,
    pub min_ram_bytes: Option<i64>,
    pub recommended_ram_bytes: Option<i64>,
    pub min_cpu_cores: Option<i32>,
    pub recommended_cpu_cores: Option<i32>,
    pub runtime: String,
    pub arch: Option<String>,
    pub artifact_format: Option<String>,
    pub publisher: Option<String>,
    pub license: Option<String>,
    pub description: Option<String>,
    pub unified_memory_only: bool,
    pub is_cpu_only: bool,
    pub source_uri: Option<String>,
    pub version: String,
}

/// What an upsert did, so the caller can count it in the job log.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Upserted {
    /// A row that did not exist before.
    Inserted,
    /// An unpinned row whose numbers were refreshed.
    Updated,
    /// A curated row, left exactly as it was.
    SkippedPinned,
    /// A different row already owns the key this one would need.
    SkippedKeyTaken,
}

/// Insert or refresh a synced catalogue row.
///
/// Three rules decide the outcome, and they are the whole contract of the
/// catalog sync (ADR 0019):
///
/// 1. a row is found by `(source, source_id)`, so a rename upstream refreshes
///    the row instead of adding a second one;
/// 2. a `pinned` row is never touched — not its numbers, not its name, not its
///    `synced_at`, because it is the figure a human checked;
/// 3. nothing is deleted. A model that disappears upstream keeps its row, which
///    is what stops a bad sync from emptying the catalogue.
///
/// Every value is bound. The row's environment is `NULL` because the catalogue
/// is shared, which the `models` row-level policy allows on purpose.
pub async fn upsert_synced(
    scope: &mut TenantScope<'_>,
    model: &SyncedModel,
) -> Result<Upserted, StoreError> {
    let existing = sqlx::query_as::<_, (Uuid, bool)>(
        "SELECT id, pinned FROM models WHERE source = $1 AND source_id = $2",
    )
    .bind(&model.source)
    .bind(&model.source_id)
    .fetch_optional(scope.connection())
    .await?;

    if let Some((id, pinned)) = existing {
        if pinned {
            return Ok(Upserted::SkippedPinned);
        }
        sqlx::query(
            "UPDATE models
                SET name = $2, family = $3, parameters = $4, context_length = $5,
                    quantization = $6, weights_bytes = $7, min_vram_bytes = $8,
                    recommended_vram_bytes = $9, min_gpu_count = $10,
                    min_ram_bytes = $11, recommended_ram_bytes = $12,
                    min_cpu_cores = $13, recommended_cpu_cores = $14,
                    runtime = $15, arch = $16, artifact_format = $17,
                    publisher = $18, license = $19, description = $20,
                    unified_memory_only = $21, is_cpu_only = $22, source_uri = $23,
                    version = $24, synced_at = now(), updated_at = now()
              WHERE id = $1",
        )
        .bind(id)
        .bind(&model.name)
        .bind(&model.family)
        .bind(&model.parameters)
        .bind(model.context_length)
        .bind(model.quantization.as_deref())
        .bind(model.weights_bytes)
        .bind(model.min_vram_bytes)
        .bind(model.recommended_vram_bytes)
        .bind(model.min_gpu_count)
        .bind(model.min_ram_bytes)
        .bind(model.recommended_ram_bytes)
        .bind(model.min_cpu_cores)
        .bind(model.recommended_cpu_cores)
        .bind(&model.runtime)
        .bind(model.arch.as_deref())
        .bind(model.artifact_format.as_deref())
        .bind(model.publisher.as_deref())
        .bind(model.license.as_deref())
        .bind(model.description.as_deref())
        .bind(model.unified_memory_only)
        .bind(model.is_cpu_only)
        .bind(model.source_uri.as_deref())
        .bind(&model.version)
        .execute(scope.connection())
        .await?;
        return Ok(Upserted::Updated);
    }

    let inserted: Option<Uuid> = sqlx::query_scalar(
        "INSERT INTO models (
            environment_id, key, name, family, parameters, context_length,
            quantization, weights_bytes, min_vram_bytes, recommended_vram_bytes,
            min_gpu_count, min_ram_bytes, recommended_ram_bytes, min_cpu_cores,
            recommended_cpu_cores, runtime, arch, artifact_format, publisher,
            license, description, unified_memory_only, is_cpu_only, source_uri,
            version, is_remote, source, source_id, synced_at, pinned
         ) VALUES (
            NULL, $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14,
            $15, $16, $17, $18, $19, $20, $21, $22, $23, $24, false, $25, $26,
            now(), false
         )
         ON CONFLICT (key) DO NOTHING
         RETURNING id",
    )
    .bind(&model.key)
    .bind(&model.name)
    .bind(&model.family)
    .bind(&model.parameters)
    .bind(model.context_length)
    .bind(model.quantization.as_deref())
    .bind(model.weights_bytes)
    .bind(model.min_vram_bytes)
    .bind(model.recommended_vram_bytes)
    .bind(model.min_gpu_count)
    .bind(model.min_ram_bytes)
    .bind(model.recommended_ram_bytes)
    .bind(model.min_cpu_cores)
    .bind(model.recommended_cpu_cores)
    .bind(&model.runtime)
    .bind(model.arch.as_deref())
    .bind(model.artifact_format.as_deref())
    .bind(model.publisher.as_deref())
    .bind(model.license.as_deref())
    .bind(model.description.as_deref())
    .bind(model.unified_memory_only)
    .bind(model.is_cpu_only)
    .bind(model.source_uri.as_deref())
    .bind(&model.version)
    .bind(&model.source)
    .bind(&model.source_id)
    .fetch_optional(scope.connection())
    .await?;

    Ok(match inserted {
        Some(_) => Upserted::Inserted,
        None => Upserted::SkippedKeyTaken,
    })
}

/// The identity of a catalogue row, for matching a second source's ids onto it.
///
/// Only what a comparison needs: the key to write, and the names a listing might
/// use for the same model.
#[derive(Debug, Clone)]
pub struct ModelIdentity {
    pub key: String,
    pub name: String,
    pub source_id: Option<String>,
}

/// Every catalogue row this environment can see, for id matching.
pub async fn identities(scope: &mut TenantScope<'_>) -> Result<Vec<ModelIdentity>, StoreError> {
    let rows = sqlx::query(
        "SELECT key, name, source_id FROM models
          WHERE environment_id IS NULL OR environment_id = $1
          ORDER BY key",
    )
    .bind(scope.environment_id())
    .fetch_all(scope.connection())
    .await?;

    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        out.push(ModelIdentity {
            key: row.try_get("key")?,
            name: row.try_get("name")?,
            source_id: row.try_get("source_id")?,
        });
    }
    Ok(out)
}

/// Record that NVIDIA serves these models as hosted NIMs.
///
/// `nim_available` is a fact about a remote listing, not a catalogue figure, so
/// this is the one column the sync sets on a pinned row. It is write-once and
/// does not touch `updated_at`: the row's edit history is about its curated
/// numbers, and a badge appearing is not an edit.
pub async fn set_nim_available(
    scope: &mut TenantScope<'_>,
    keys: &[String],
) -> Result<u64, StoreError> {
    if keys.is_empty() {
        return Ok(0);
    }
    let result = sqlx::query(
        "UPDATE models SET nim_available = true
          WHERE key = ANY($1)
            AND (environment_id IS NULL OR environment_id = $2)
            AND NOT nim_available",
    )
    .bind(keys)
    .bind(scope.environment_id())
    .execute(scope.connection())
    .await?;
    Ok(result.rows_affected())
}
