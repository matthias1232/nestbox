//! Rows of `model_grants`: which models a sandbox may use (ADR 0057).

use chrono::{DateTime, Utc};
use sqlx::Row;
use uuid::Uuid;

use super::super::TenantScope;
use crate::domain::model_grant::{ModelGrant, NewModelGrant};
use crate::error::StoreError;

fn to_grant(row: &sqlx::postgres::PgRow) -> Result<ModelGrant, StoreError> {
    Ok(ModelGrant {
        id: row.try_get("id")?,
        scope: row.try_get("scope")?,
        user_id: row.try_get("user_id")?,
        sandbox_id: row.try_get("sandbox_id")?,
        model_name: row.try_get("model_name")?,
        provider_profile_id: row.try_get("provider_profile_id")?,
        upstream_model: row.try_get("upstream_model")?,
        is_default: row.try_get("is_default")?,
        keep_warm: row.try_get("keep_warm")?,
        created_at: row.try_get::<DateTime<Utc>, _>("created_at")?,
    })
}

const COLUMNS: &str = "id, scope, user_id, sandbox_id, model_name, provider_profile_id, \
                       upstream_model, is_default, keep_warm, created_at";

/// Every grant of this environment, optionally filtered.
pub async fn list(
    scope: &mut TenantScope<'_>,
    sandbox_id: Option<Uuid>,
    user_id: Option<Uuid>,
) -> Result<Vec<ModelGrant>, StoreError> {
    let rows = sqlx::query(&format!(
        "SELECT {COLUMNS} FROM model_grants
          WHERE environment_id = $1
            AND ($2::uuid IS NULL OR sandbox_id = $2)
            AND ($3::uuid IS NULL OR user_id = $3)
          ORDER BY model_name"
    ))
    .bind(scope.environment_id())
    .bind(sandbox_id)
    .bind(user_id)
    .fetch_all(scope.connection())
    .await?;
    rows.iter().map(to_grant).collect()
}

/// The grants that apply to one sandbox: the environment's, its owner's and its own.
pub async fn for_sandbox(
    scope: &mut TenantScope<'_>,
    sandbox_id: Uuid,
    owner_user_id: Option<Uuid>,
) -> Result<Vec<ModelGrant>, StoreError> {
    let rows = sqlx::query(&format!(
        "SELECT {COLUMNS} FROM model_grants
          WHERE environment_id = $1
            AND (scope = 'environment' OR sandbox_id = $2 OR ($3::uuid IS NOT NULL AND user_id = $3))
          ORDER BY model_name"
    ))
    .bind(scope.environment_id())
    .bind(sandbox_id)
    .bind(owner_user_id)
    .fetch_all(scope.connection())
    .await?;
    rows.iter().map(to_grant).collect()
}

/// Grant a model.
pub async fn insert(
    scope: &mut TenantScope<'_>,
    request: &NewModelGrant,
    created_by: Option<Uuid>,
) -> Result<ModelGrant, StoreError> {
    let row = sqlx::query(&format!(
        "INSERT INTO model_grants
            (environment_id, scope, user_id, sandbox_id, model_name, provider_profile_id,
             upstream_model, is_default, keep_warm, created_by)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
         RETURNING {COLUMNS}"
    ))
    .bind(scope.environment_id())
    .bind(&request.scope)
    .bind(request.user_id)
    .bind(request.sandbox_id)
    .bind(&request.model_name)
    .bind(request.provider_profile_id)
    .bind(&request.upstream_model)
    .bind(request.is_default)
    .bind(request.keep_warm)
    .bind(created_by)
    .fetch_one(scope.connection())
    .await?;
    to_grant(&row)
}

/// Revoke a grant, returning the row that was removed.
pub async fn delete(scope: &mut TenantScope<'_>, id: Uuid) -> Result<Option<ModelGrant>, StoreError> {
    let row = sqlx::query(&format!(
        "DELETE FROM model_grants WHERE environment_id = $1 AND id = $2 RETURNING {COLUMNS}"
    ))
    .bind(scope.environment_id())
    .bind(id)
    .fetch_optional(scope.connection())
    .await?;
    match row {
        Some(row) => Ok(Some(to_grant(&row)?)),
        None => Ok(None),
    }
}

/// The id and owner of a sandbox by name, inside this environment.
pub async fn sandbox_by_name(
    scope: &mut TenantScope<'_>,
    name: &str,
) -> Result<Option<(Uuid, Option<Uuid>)>, StoreError> {
    let row = sqlx::query("SELECT id, owner_user_id FROM sandboxes WHERE environment_id = $1 AND name = $2")
        .bind(scope.environment_id())
        .bind(name)
        .fetch_optional(scope.connection())
        .await?;
    Ok(row.map(|row| (row.get::<Uuid, _>("id"), row.get::<Option<Uuid>, _>("owner_user_id"))))
}

/// The owner of one sandbox, for inheritance.
pub async fn owner_of(scope: &mut TenantScope<'_>, sandbox_id: Uuid) -> Result<Option<Uuid>, StoreError> {
    let found: Option<Option<Uuid>> =
        sqlx::query_scalar("SELECT owner_user_id FROM sandboxes WHERE environment_id = $1 AND id = $2")
            .bind(scope.environment_id())
            .bind(sandbox_id)
            .fetch_optional(scope.connection())
            .await?;
    Ok(found.flatten())
}

/// The sandboxes a grant reaches, for the live refresh.
pub async fn affected_sandboxes(
    scope: &mut TenantScope<'_>,
    grant: &ModelGrant,
) -> Result<Vec<Uuid>, StoreError> {
    let rows: Vec<Uuid> = match grant.scope.as_str() {
        "sandbox" => sqlx::query_scalar("SELECT id FROM sandboxes WHERE environment_id = $1 AND id = $2")
            .bind(scope.environment_id())
            .bind(grant.sandbox_id)
            .fetch_all(scope.connection())
            .await?,
        "user" => sqlx::query_scalar(
            "SELECT id FROM sandboxes WHERE environment_id = $1 AND owner_user_id = $2",
        )
        .bind(scope.environment_id())
        .bind(grant.user_id)
        .fetch_all(scope.connection())
        .await?,
        _ => sqlx::query_scalar("SELECT id FROM sandboxes WHERE environment_id = $1")
            .bind(scope.environment_id())
            .fetch_all(scope.connection())
            .await?,
    };
    Ok(rows)
}
