//! Image templates: saved builder selections, scoped to one environment.
//!
//! A template is read and written inside the tenant transaction like any other
//! aggregate; the row-level policy on `image_templates` repeats the environment
//! check so a forgotten filter returns nothing rather than another tenant's
//! repositories.

use sqlx::Row;
use uuid::Uuid;

use super::super::TenantScope;
use crate::domain::builder::{ImageTemplate, RepositorySpec};
use crate::error::StoreError;

const COLUMNS: &str = "id, environment_id, name, description, baseline_key, tools, extra_packages, \
     repositories, mcp_server_keys, is_default, is_shared, created_by, created_at, updated_at";

fn row_to_template(row: &sqlx::postgres::PgRow) -> Result<ImageTemplate, StoreError> {
    let repositories_raw: serde_json::Value = row.try_get("repositories")?;
    let repositories: Vec<RepositorySpec> = serde_json::from_value(repositories_raw).unwrap_or_default();
    Ok(ImageTemplate {
        id: row.try_get("id")?,
        environment_id: row.try_get("environment_id")?,
        name: row.try_get("name")?,
        description: row.try_get("description")?,
        baseline_key: row.try_get("baseline_key")?,
        tools: row.try_get("tools")?,
        extra_packages: row.try_get("extra_packages")?,
        repositories,
        mcp_server_keys: row.try_get("mcp_server_keys")?,
        is_default: row.try_get("is_default")?,
        is_shared: row.try_get("is_shared")?,
        created_by: row.try_get("created_by")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

/// What a create or replace carries.
pub struct NewTemplate<'a> {
    pub name: &'a str,
    pub description: &'a str,
    pub baseline_key: &'a str,
    pub tools: &'a [String],
    pub extra_packages: &'a [String],
    pub repositories: &'a [RepositorySpec],
    pub mcp_server_keys: &'a [String],
    pub is_default: bool,
    pub is_shared: bool,
    pub created_by: Option<Uuid>,
}

/// The templates visible to an actor.
///
/// A shared template is offered to everyone in the environment; an unshared one
/// only to its author and to an admin, who is the person a support case asks.
pub async fn list(
    scope: &mut TenantScope<'_>,
    actor_id: Uuid,
    actor_is_admin: bool,
) -> Result<Vec<ImageTemplate>, StoreError> {
    let sql = format!(
        "SELECT {COLUMNS} FROM image_templates
          WHERE environment_id = $1
            AND ($2 OR is_shared OR created_by = $3)
          ORDER BY is_default DESC, name"
    );
    let rows = sqlx::query(&sql)
        .bind(scope.environment_id())
        .bind(actor_is_admin)
        .bind(actor_id)
        .fetch_all(scope.connection())
        .await?;
    rows.iter().map(row_to_template).collect()
}

/// One template by id.
pub async fn get(scope: &mut TenantScope<'_>, id: Uuid) -> Result<ImageTemplate, StoreError> {
    let sql = format!("SELECT {COLUMNS} FROM image_templates WHERE environment_id = $1 AND id = $2");
    let row = sqlx::query(&sql)
        .bind(scope.environment_id())
        .bind(id)
        .fetch_optional(scope.connection())
        .await?
        .ok_or(StoreError::NotFound)?;
    row_to_template(&row)
}

/// The environment's default template, if one exists.
pub async fn get_default(scope: &mut TenantScope<'_>) -> Result<Option<ImageTemplate>, StoreError> {
    let sql = format!(
        "SELECT {COLUMNS} FROM image_templates WHERE environment_id = $1 AND is_default LIMIT 1"
    );
    let row = sqlx::query(&sql)
        .bind(scope.environment_id())
        .fetch_optional(scope.connection())
        .await?;
    row.as_ref().map(row_to_template).transpose()
}

/// The environment's default template, or `None` when the actor may not see it.
pub async fn default_for(
    scope: &mut TenantScope<'_>,
    actor_id: Uuid,
    actor_is_admin: bool,
) -> Result<Option<ImageTemplate>, StoreError> {
    let template = get_default(scope).await?;
    Ok(template.filter(|entry| actor_is_admin || entry.is_shared || entry.created_by == Some(actor_id)))
}

/// Insert a template. A default replaces the previous default in the same
/// transaction, so the partial unique index can never be violated by an update
/// that clears one and sets another.
pub async fn insert(
    scope: &mut TenantScope<'_>,
    new: NewTemplate<'_>,
) -> Result<ImageTemplate, StoreError> {
    if new.is_default {
        clear_default(scope).await?;
    }
    let repositories =
        serde_json::to_value(new.repositories).unwrap_or(serde_json::Value::Array(vec![]));
    let sql = format!(
        "INSERT INTO image_templates
             (environment_id, name, description, baseline_key, tools, extra_packages,
              repositories, mcp_server_keys, is_default, is_shared, created_by)
          VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
          RETURNING {COLUMNS}"
    );
    let row = sqlx::query(&sql)
        .bind(scope.environment_id())
        .bind(new.name)
        .bind(new.description)
        .bind(new.baseline_key)
        .bind(new.tools)
        .bind(new.extra_packages)
        .bind(repositories)
        .bind(new.mcp_server_keys)
        .bind(new.is_default)
        .bind(new.is_shared)
        .bind(new.created_by)
        .fetch_one(scope.connection())
        .await?;
    row_to_template(&row)
}

/// Replace a template's editable fields.
pub async fn update(
    scope: &mut TenantScope<'_>,
    id: Uuid,
    new: NewTemplate<'_>,
) -> Result<ImageTemplate, StoreError> {
    if new.is_default {
        clear_default(scope).await?;
    }
    let repositories =
        serde_json::to_value(new.repositories).unwrap_or(serde_json::Value::Array(vec![]));
    let sql = format!(
        "UPDATE image_templates
            SET name = $3, description = $4, baseline_key = $5, tools = $6,
                extra_packages = $7, repositories = $8, mcp_server_keys = $9,
                is_default = $10, is_shared = $11, updated_at = now()
          WHERE environment_id = $1 AND id = $2
          RETURNING {COLUMNS}"
    );
    let row = sqlx::query(&sql)
        .bind(scope.environment_id())
        .bind(id)
        .bind(new.name)
        .bind(new.description)
        .bind(new.baseline_key)
        .bind(new.tools)
        .bind(new.extra_packages)
        .bind(repositories)
        .bind(new.mcp_server_keys)
        .bind(new.is_default)
        .bind(new.is_shared)
        .fetch_optional(scope.connection())
        .await?
        .ok_or(StoreError::NotFound)?;
    row_to_template(&row)
}

/// Delete a template.
pub async fn delete(scope: &mut TenantScope<'_>, id: Uuid) -> Result<(), StoreError> {
    let result = sqlx::query("DELETE FROM image_templates WHERE environment_id = $1 AND id = $2")
        .bind(scope.environment_id())
        .bind(id)
        .execute(scope.connection())
        .await?;
    if result.rows_affected() == 0 {
        return Err(StoreError::NotFound);
    }
    Ok(())
}

/// Clear the environment's default so a new one can be set.
async fn clear_default(scope: &mut TenantScope<'_>) -> Result<(), StoreError> {
    sqlx::query("UPDATE image_templates SET is_default = false WHERE environment_id = $1 AND is_default")
        .bind(scope.environment_id())
        .execute(scope.connection())
        .await?;
    Ok(())
}
