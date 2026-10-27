//! Model installs: one row per model per node.

use sqlx::Row;
use uuid::Uuid;

use super::super::TenantScope;
use crate::domain::model_install::{InstallState, ModelInstall};
use crate::error::StoreError;

const INSTALL_COLUMNS: &str = "id, environment_id, model_key, node_id, state, installed_version, \
     reserved_ram_bytes, reserved_vram_bytes, provider_profile_id, job_id, error, \
     created_at, updated_at";

fn row_to_install(row: &sqlx::postgres::PgRow) -> Result<ModelInstall, StoreError> {
    let state_raw: String = row.try_get("state")?;
    Ok(ModelInstall {
        id: row.try_get("id")?,
        environment_id: row.try_get("environment_id")?,
        model_key: row.try_get("model_key")?,
        node_id: row.try_get("node_id")?,
        state: InstallState::parse(&state_raw).unwrap_or(InstallState::Failed),
        installed_version: row.try_get("installed_version")?,
        reserved_ram_bytes: row.try_get("reserved_ram_bytes")?,
        reserved_vram_bytes: row.try_get("reserved_vram_bytes")?,
        provider_profile_id: row.try_get("provider_profile_id")?,
        job_id: row.try_get("job_id")?,
        error: row.try_get("error")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

/// Every install in this environment, optionally on one node.
pub async fn list(
    scope: &mut TenantScope<'_>,
    node_id: Option<Uuid>,
) -> Result<Vec<ModelInstall>, StoreError> {
    let sql = format!(
        "SELECT {INSTALL_COLUMNS} FROM model_installs
          WHERE environment_id = $1 AND ($2::uuid IS NULL OR node_id = $2)
          ORDER BY created_at"
    );
    let rows = sqlx::query(&sql)
        .bind(scope.environment_id())
        .bind(node_id)
        .fetch_all(scope.connection())
        .await?;
    rows.iter().map(row_to_install).collect()
}

/// One install by model and node.
pub async fn get(
    scope: &mut TenantScope<'_>,
    model_key: &str,
    node_id: Uuid,
) -> Result<Option<ModelInstall>, StoreError> {
    let sql = format!(
        "SELECT {INSTALL_COLUMNS} FROM model_installs
          WHERE environment_id = $1 AND model_key = $2 AND node_id = $3"
    );
    let row = sqlx::query(&sql)
        .bind(scope.environment_id())
        .bind(model_key)
        .bind(node_id)
        .fetch_optional(scope.connection())
        .await?;
    row.as_ref().map(row_to_install).transpose()
}

/// One install by id.
pub async fn get_by_id(
    scope: &mut TenantScope<'_>,
    id: Uuid,
) -> Result<ModelInstall, StoreError> {
    let sql = format!(
        "SELECT {INSTALL_COLUMNS} FROM model_installs WHERE environment_id = $1 AND id = $2"
    );
    let row = sqlx::query(&sql)
        .bind(scope.environment_id())
        .bind(id)
        .fetch_optional(scope.connection())
        .await?
        .ok_or(StoreError::NotFound)?;
    row_to_install(&row)
}

/// What an insert carries.
pub struct NewInstall<'a> {
    pub model_key: &'a str,
    pub node_id: Uuid,
    pub reserved_ram_bytes: i64,
    pub reserved_vram_bytes: i64,
    pub job_id: Option<Uuid>,
}

/// Create an install in state `installing`.
pub async fn insert(
    scope: &mut TenantScope<'_>,
    new: NewInstall<'_>,
) -> Result<ModelInstall, StoreError> {
    let sql = format!(
        "INSERT INTO model_installs
            (environment_id, model_key, node_id, state, reserved_ram_bytes,
             reserved_vram_bytes, job_id)
         VALUES ($1, $2, $3, 'installing', $4, $5, $6)
         RETURNING {INSTALL_COLUMNS}"
    );
    let row = sqlx::query(&sql)
        .bind(scope.environment_id())
        .bind(new.model_key)
        .bind(new.node_id)
        .bind(new.reserved_ram_bytes)
        .bind(new.reserved_vram_bytes)
        .bind(new.job_id)
        .fetch_one(scope.connection())
        .await?;
    row_to_install(&row)
}

/// Move an install to a new state.
pub async fn set_state(
    scope: &mut TenantScope<'_>,
    id: Uuid,
    state: InstallState,
    installed_version: Option<&str>,
    error: Option<&str>,
) -> Result<ModelInstall, StoreError> {
    let sql = format!(
        "UPDATE model_installs
            SET state = $3,
                installed_version = COALESCE($4, installed_version),
                error = $5,
                updated_at = now()
          WHERE environment_id = $1 AND id = $2
      RETURNING {INSTALL_COLUMNS}"
    );
    let row = sqlx::query(&sql)
        .bind(scope.environment_id())
        .bind(id)
        .bind(state.as_str())
        .bind(installed_version)
        .bind(error)
        .fetch_optional(scope.connection())
        .await?
        .ok_or(StoreError::NotFound)?;
    row_to_install(&row)
}

/// Attach the provider profile created for this install.
pub async fn set_provider(
    scope: &mut TenantScope<'_>,
    id: Uuid,
    provider_profile_id: Option<Uuid>,
) -> Result<(), StoreError> {
    sqlx::query(
        "UPDATE model_installs SET provider_profile_id = $3, updated_at = now()
          WHERE environment_id = $1 AND id = $2",
    )
    .bind(scope.environment_id())
    .bind(id)
    .bind(provider_profile_id)
    .execute(scope.connection())
    .await?;
    Ok(())
}

/// Attach the job that installs or removes this model.
///
/// The row is inserted before the job exists, so a failure to create the job
/// rolls the whole install back instead of leaving a reservation with nothing
/// working on it.
pub async fn set_job(
    scope: &mut TenantScope<'_>,
    id: Uuid,
    job_id: Uuid,
) -> Result<(), StoreError> {
    sqlx::query(
        "UPDATE model_installs SET job_id = $3, updated_at = now()
          WHERE environment_id = $1 AND id = $2",
    )
    .bind(scope.environment_id())
    .bind(id)
    .bind(job_id)
    .execute(scope.connection())
    .await?;
    Ok(())
}

/// The sandboxes whose provider selection points at this install's profile.
///
/// An uninstall must not remove a profile a workload still selects: the
/// sandbox would keep an endpoint that no longer resolves. The names are
/// returned so `provider.in_use` can list them rather than say "in use".
pub async fn sandboxes_using_profile(
    scope: &mut TenantScope<'_>,
    provider_profile_id: Uuid,
) -> Result<Vec<String>, StoreError> {
    let rows = sqlx::query(
        "SELECT name FROM sandboxes
          WHERE environment_id = $1 AND provider_profile_id = $2 AND status <> 'deleted'
          ORDER BY name",
    )
    .bind(scope.environment_id())
    .bind(provider_profile_id)
    .fetch_all(scope.connection())
    .await?;
    rows.iter()
        .map(|row| row.try_get("name").map_err(StoreError::from))
        .collect()
}

/// Remove an install row; its reservation goes with it.
pub async fn delete(scope: &mut TenantScope<'_>, id: Uuid) -> Result<(), StoreError> {
    let result = sqlx::query("DELETE FROM model_installs WHERE environment_id = $1 AND id = $2")
        .bind(scope.environment_id())
        .bind(id)
        .execute(scope.connection())
        .await?;
    if result.rows_affected() == 0 {
        return Err(StoreError::NotFound);
    }
    Ok(())
}

/// The sum of reservations on a node, excluding failed installs.
pub async fn reserved_on(
    scope: &mut TenantScope<'_>,
    node_id: Uuid,
) -> Result<(i64, i64, i64), StoreError> {
    let row = sqlx::query(
        "SELECT COALESCE(SUM(reserved_ram_bytes), 0)::bigint AS ram,
                COALESCE(SUM(reserved_vram_bytes), 0)::bigint AS vram,
                COUNT(*)::bigint AS n
           FROM model_installs
          WHERE environment_id = $1 AND node_id = $2 AND state <> 'failed'",
    )
    .bind(scope.environment_id())
    .bind(node_id)
    .fetch_one(scope.connection())
    .await?;
    Ok((
        row.try_get("ram")?,
        row.try_get("vram")?,
        row.try_get("n")?,
    ))
}
