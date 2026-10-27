//! Recipes: the builder's desired state.

use sqlx::Row;
use uuid::Uuid;

use super::super::TenantScope;
use crate::domain::builder::{AgentSpec, ProjectMount, Recipe, RecipeState, ResourceLimits};
use crate::error::StoreError;

const RECIPE_COLUMNS: &str = "id, environment_id, owner_user_id, name, baseline_key, \
     tool_profile_id, tools, agents, resources, project_mounts, node_id, state, image_ref, \
     content_hash, revision, build_job_id, build_error, created_at, updated_at";

fn row_to_recipe(row: &sqlx::postgres::PgRow) -> Result<Recipe, StoreError> {
    let state_raw: String = row.try_get("state")?;
    let agents_raw: serde_json::Value = row.try_get("agents")?;
    let resources_raw: serde_json::Value = row.try_get("resources")?;
    let mounts_raw: serde_json::Value = row.try_get("project_mounts")?;
    let agents: Vec<AgentSpec> = serde_json::from_value(agents_raw).unwrap_or_default();
    let resources: ResourceLimits = serde_json::from_value(resources_raw).unwrap_or_default();
    let project_mounts: Vec<ProjectMount> = serde_json::from_value(mounts_raw).unwrap_or_default();
    Ok(Recipe {
        id: row.try_get("id")?,
        environment_id: row.try_get("environment_id")?,
        owner_user_id: row.try_get("owner_user_id")?,
        name: row.try_get("name")?,
        baseline_key: row.try_get("baseline_key")?,
        tool_profile_id: row.try_get("tool_profile_id")?,
        tools: row.try_get("tools")?,
        agents,
        resources,
        project_mounts,
        node_id: row.try_get("node_id")?,
        state: RecipeState::parse(&state_raw).unwrap_or(RecipeState::Draft),
        image_ref: row.try_get("image_ref")?,
        content_hash: row.try_get("content_hash")?,
        revision: row.try_get("revision")?,
        build_job_id: row.try_get("build_job_id")?,
        build_error: row.try_get("build_error")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

/// Every recipe in this environment.
pub async fn list(scope: &mut TenantScope<'_>) -> Result<Vec<Recipe>, StoreError> {
    let sql = format!(
        "SELECT {RECIPE_COLUMNS} FROM recipes WHERE environment_id = $1 ORDER BY updated_at DESC"
    );
    let rows = sqlx::query(&sql)
        .bind(scope.environment_id())
        .fetch_all(scope.connection())
        .await?;
    rows.iter().map(row_to_recipe).collect()
}

/// One recipe by id.
pub async fn get(scope: &mut TenantScope<'_>, id: Uuid) -> Result<Recipe, StoreError> {
    let sql = format!("SELECT {RECIPE_COLUMNS} FROM recipes WHERE environment_id = $1 AND id = $2");
    let row = sqlx::query(&sql)
        .bind(scope.environment_id())
        .bind(id)
        .fetch_optional(scope.connection())
        .await?
        .ok_or(StoreError::NotFound)?;
    row_to_recipe(&row)
}

/// What an insert carries.
pub struct NewRecipe<'a> {
    pub owner_user_id: Option<Uuid>,
    pub name: &'a str,
    pub baseline_key: &'a str,
    pub tool_profile_id: Option<Uuid>,
    pub tools: &'a [String],
    pub agents: &'a [AgentSpec],
    pub resources: &'a ResourceLimits,
    pub project_mounts: &'a [ProjectMount],
    pub node_id: Uuid,
}

/// Create a draft recipe.
pub async fn insert(scope: &mut TenantScope<'_>, new: NewRecipe<'_>) -> Result<Recipe, StoreError> {
    let agents = serde_json::to_value(new.agents).unwrap_or(serde_json::Value::Array(vec![]));
    let resources = serde_json::to_value(new.resources).unwrap_or(serde_json::json!({}));
    let mounts = serde_json::to_value(new.project_mounts).unwrap_or(serde_json::Value::Array(vec![]));
    let sql = format!(
        "INSERT INTO recipes
            (environment_id, owner_user_id, name, baseline_key, tool_profile_id, tools,
             agents, resources, project_mounts, node_id)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
         RETURNING {RECIPE_COLUMNS}"
    );
    let row = sqlx::query(&sql)
        .bind(scope.environment_id())
        .bind(new.owner_user_id)
        .bind(new.name)
        .bind(new.baseline_key)
        .bind(new.tool_profile_id)
        .bind(new.tools)
        .bind(agents)
        .bind(resources)
        .bind(mounts)
        .bind(new.node_id)
        .fetch_one(scope.connection())
        .await?;
    row_to_recipe(&row)
}

/// Replace the editable fields; the revision increases and the state returns
/// to draft because the image no longer matches. The previous build error is
/// cleared: it described content that no longer exists.
pub async fn update(
    scope: &mut TenantScope<'_>,
    id: Uuid,
    new: NewRecipe<'_>,
) -> Result<Recipe, StoreError> {
    let agents = serde_json::to_value(new.agents).unwrap_or(serde_json::Value::Array(vec![]));
    let resources = serde_json::to_value(new.resources).unwrap_or(serde_json::json!({}));
    let mounts = serde_json::to_value(new.project_mounts).unwrap_or(serde_json::Value::Array(vec![]));
    let sql = format!(
        "UPDATE recipes
            SET name = $3, baseline_key = $4, tool_profile_id = $5, tools = $6, agents = $7,
                resources = $8, project_mounts = $9, node_id = $10,
                state = 'draft', revision = revision + 1, build_error = NULL,
                updated_at = now()
          WHERE environment_id = $1 AND id = $2
      RETURNING {RECIPE_COLUMNS}"
    );
    let row = sqlx::query(&sql)
        .bind(scope.environment_id())
        .bind(id)
        .bind(new.name)
        .bind(new.baseline_key)
        .bind(new.tool_profile_id)
        .bind(new.tools)
        .bind(agents)
        .bind(resources)
        .bind(mounts)
        .bind(new.node_id)
        .fetch_optional(scope.connection())
        .await?
        .ok_or(StoreError::NotFound)?;
    row_to_recipe(&row)
}

/// Record a build outcome.
pub async fn set_build(
    scope: &mut TenantScope<'_>,
    id: Uuid,
    state: RecipeState,
    image_ref: Option<&str>,
    content_hash: Option<&str>,
    job_id: Option<Uuid>,
    error: Option<&str>,
) -> Result<Recipe, StoreError> {
    let sql = format!(
        "UPDATE recipes
            SET state = $3, image_ref = COALESCE($4, image_ref),
                content_hash = COALESCE($5, content_hash),
                build_job_id = COALESCE($6, build_job_id), build_error = $7, updated_at = now()
          WHERE environment_id = $1 AND id = $2
      RETURNING {RECIPE_COLUMNS}"
    );
    let row = sqlx::query(&sql)
        .bind(scope.environment_id())
        .bind(id)
        .bind(state.as_str())
        .bind(image_ref)
        .bind(content_hash)
        .bind(job_id)
        .bind(error)
        .fetch_optional(scope.connection())
        .await?
        .ok_or(StoreError::NotFound)?;
    row_to_recipe(&row)
}

/// Record which recipe a sandbox was created from.
///
/// Kept in this file rather than in `sandboxes.rs` because the recipe is the
/// subject: the sandbox remembers its source so "rebuild from recipe" has an
/// answer, and deleting the recipe only clears the link.
pub async fn attach_sandbox(
    scope: &mut TenantScope<'_>,
    recipe_id: Uuid,
    sandbox_id: Uuid,
) -> Result<(), StoreError> {
    let result = sqlx::query(
        "UPDATE sandboxes SET recipe_id = $3, updated_at = now()
          WHERE environment_id = $1 AND id = $2",
    )
    .bind(scope.environment_id())
    .bind(sandbox_id)
    .bind(recipe_id)
    .execute(scope.connection())
    .await?;
    if result.rows_affected() == 0 {
        return Err(StoreError::NotFound);
    }
    Ok(())
}

/// Delete a recipe. Sandboxes keep running; their `recipe_id` becomes null.
pub async fn delete(scope: &mut TenantScope<'_>, id: Uuid) -> Result<(), StoreError> {
    let result = sqlx::query("DELETE FROM recipes WHERE environment_id = $1 AND id = $2")
        .bind(scope.environment_id())
        .bind(id)
        .execute(scope.connection())
        .await?;
    if result.rows_affected() == 0 {
        return Err(StoreError::NotFound);
    }
    Ok(())
}
