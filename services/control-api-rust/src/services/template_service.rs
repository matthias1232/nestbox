//! Image templates: a saved builder selection that a team can reuse.
//!
//! The rule this module keeps: a template carries choices, never an image.
//! Applying one copies its catalog keys and package names onto a recipe; the
//! recipe's content hash then decides whether a build is a reuse or a rebuild.
//! A template that stored a tag would hand out an image that no longer matches
//! the tools it claims to contain.

use uuid::Uuid;

use super::{require, selection, ServiceContext};
use crate::domain::auth::{Permission, Principal, Role};
use crate::domain::builder::{ImageTemplate, RepositorySpec};
use crate::error::{ApiError, ErrorCode, StoreError};
use crate::store::TenantScope;
use crate::store::postgres::agent_packs as catalog;
use crate::store::postgres::image_templates as store;
use crate::store::postgres::mcp as mcp_store;
use crate::store::postgres::recipes as recipe_store;

/// What a create or replace carries.
#[derive(Debug, Clone)]
pub struct TemplateInput {
    pub name: String,
    pub description: String,
    pub baseline_key: String,
    pub tools: Vec<String>,
    pub extra_packages: Vec<String>,
    pub repositories: Vec<RepositorySpec>,
    pub mcp_server_keys: Vec<String>,
    pub is_default: bool,
    pub is_shared: bool,
}

/// Whether the actor may see and manage every template in the environment.
///
/// An admin supports the environment and is the person a shared template is
/// unshared *to*; everyone else keeps their unshared drafts to themselves.
fn admin(principal: &Principal) -> bool {
    principal.role == Role::Admin
}

/// Every template this actor may use.
pub async fn list(
    ctx: &ServiceContext,
    principal: &Principal,
) -> Result<Vec<ImageTemplate>, ApiError> {
    require(principal, Permission::RecipeRead)?;
    let mut scope = TenantScope::begin_for(&ctx.pool, principal)
        .await
        .map_err(ApiError::from)?;
    let templates = store::list(&mut scope, principal.user_id, admin(principal))
        .await
        .map_err(ApiError::from)?;
    scope.commit().await.map_err(ApiError::from)?;
    Ok(templates)
}

/// One template.
pub async fn get(
    ctx: &ServiceContext,
    principal: &Principal,
    id: Uuid,
) -> Result<ImageTemplate, ApiError> {
    require(principal, Permission::RecipeRead)?;
    let mut scope = TenantScope::begin_for(&ctx.pool, principal)
        .await
        .map_err(ApiError::from)?;
    let template = store::get(&mut scope, id).await.map_err(template_error)?;
    scope.commit().await.map_err(ApiError::from)?;
    Ok(template)
}

/// The environment's default template, when this actor may use it.
///
/// A recipe starts from this selection; an absent default is `None`, not an
/// error, because a fresh environment has no default until one is marked.
pub async fn default_for(
    ctx: &ServiceContext,
    principal: &Principal,
) -> Result<Option<ImageTemplate>, ApiError> {
    require(principal, Permission::RecipeRead)?;
    let mut scope = TenantScope::begin_for(&ctx.pool, principal)
        .await
        .map_err(ApiError::from)?;
    let template = store::default_for(&mut scope, principal.user_id, admin(principal))
        .await
        .map_err(ApiError::from)?;
    scope.commit().await.map_err(ApiError::from)?;
    Ok(template)
}

/// Create a template from the builder's current selection.
pub async fn create(
    ctx: &ServiceContext,
    principal: &Principal,
    input: TemplateInput,
) -> Result<ImageTemplate, ApiError> {
    require(principal, Permission::RecipeWrite)?;
    let mut scope = TenantScope::begin_for(&ctx.pool, principal)
        .await
        .map_err(ApiError::from)?;
    let new = validate(&mut scope, input).await?;
    let template = store::insert(
        &mut scope,
        store::NewTemplate {
            name: &new.name,
            description: &new.description,
            baseline_key: &new.baseline_key,
            tools: &new.tools,
            extra_packages: &new.extra_packages,
            repositories: &new.repositories,
            mcp_server_keys: &new.mcp_server_keys,
            is_default: new.is_default,
            is_shared: new.is_shared,
            created_by: Some(principal.user_id),
        },
    )
    .await
    .map_err(write_error)?;
    audit(&mut scope, principal, "image_template.create", template.id, &template.name).await?;
    scope.commit().await.map_err(ApiError::from)?;
    Ok(template)
}

/// Replace a template.
pub async fn update(
    ctx: &ServiceContext,
    principal: &Principal,
    id: Uuid,
    input: TemplateInput,
) -> Result<ImageTemplate, ApiError> {
    require(principal, Permission::RecipeWrite)?;
    let mut scope = TenantScope::begin_for(&ctx.pool, principal)
        .await
        .map_err(ApiError::from)?;
    // The row is read first so an unshared template of another author is a 404
    // for an operator, not a silent overwrite by id.
    let existing = store::get(&mut scope, id).await.map_err(template_error)?;
    if !admin(principal) && existing.created_by != Some(principal.user_id) {
        scope.commit().await.map_err(ApiError::from)?;
        return Err(ApiError::not_found(
            ErrorCode::NOT_FOUND,
            "No such template.",
        ));
    }
    let new = validate(&mut scope, input).await?;
    let template = store::update(
        &mut scope,
        id,
        store::NewTemplate {
            name: &new.name,
            description: &new.description,
            baseline_key: &new.baseline_key,
            tools: &new.tools,
            extra_packages: &new.extra_packages,
            repositories: &new.repositories,
            mcp_server_keys: &new.mcp_server_keys,
            is_default: new.is_default,
            is_shared: new.is_shared,
            created_by: existing.created_by,
        },
    )
    .await
    .map_err(write_error)?;
    audit(&mut scope, principal, "image_template.update", template.id, &template.name).await?;
    scope.commit().await.map_err(ApiError::from)?;
    Ok(template)
}

/// Delete a template. Recipes that used it keep their copy of the selection.
pub async fn delete(
    ctx: &ServiceContext,
    principal: &Principal,
    id: Uuid,
) -> Result<(), ApiError> {
    require(principal, Permission::RecipeWrite)?;
    let mut scope = TenantScope::begin_for(&ctx.pool, principal)
        .await
        .map_err(ApiError::from)?;
    let existing = store::get(&mut scope, id).await.map_err(template_error)?;
    if !admin(principal) && existing.created_by != Some(principal.user_id) {
        scope.commit().await.map_err(ApiError::from)?;
        return Err(ApiError::not_found(
            ErrorCode::NOT_FOUND,
            "No such template.",
        ));
    }
    store::delete(&mut scope, id).await.map_err(template_error)?;
    audit(&mut scope, principal, "image_template.delete", id, &existing.name).await?;
    scope.commit().await.map_err(ApiError::from)?;
    Ok(())
}

/// Copy a template's selection onto a recipe.
///
/// The recipe keeps its name, agents, resources and mounts: a template is about
/// what is installed, not about how much memory the sandbox gets. The revision
/// moves and the built image is cleared, because the previous image does not
/// contain this selection.
pub async fn apply_to_recipe(
    ctx: &ServiceContext,
    principal: &Principal,
    template_id: Uuid,
    recipe_id: Uuid,
) -> Result<crate::domain::builder::Recipe, ApiError> {
    require(principal, Permission::RecipeWrite)?;
    let mut scope = TenantScope::begin_for(&ctx.pool, principal)
        .await
        .map_err(ApiError::from)?;
    let template = store::get(&mut scope, template_id)
        .await
        .map_err(template_error)?;
    if !admin(principal)
        && !template.is_shared
        && template.created_by != Some(principal.user_id)
    {
        scope.commit().await.map_err(ApiError::from)?;
        return Err(ApiError::not_found(
            ErrorCode::NOT_FOUND,
            "No such template.",
        ));
    }
    let recipe = recipe_store::apply_template(&mut scope, recipe_id, &template)
        .await
        .map_err(recipe_error)?;
    audit(
        &mut scope,
        principal,
        "image_template.apply",
        template.id,
        &format!("{} -> {}", template.name, recipe.name),
    )
    .await?;
    scope.commit().await.map_err(ApiError::from)?;
    Ok(recipe)
}

/// Validate a template input against the same rules the builder applies.
async fn validate(
    scope: &mut TenantScope<'_>,
    input: TemplateInput,
) -> Result<TemplateInput, ApiError> {
    let name = selection::template_name(&input.name).map_err(ApiError::validation)?;
    let description = input.description.trim().to_string();
    if description.len() > 2000 {
        return Err(ApiError::validation("A description is at most 2000 characters."));
    }
    catalog::get_baseline(scope, &input.baseline_key)
        .await
        .map_err(|error| match error {
            StoreError::NotFound => ApiError::not_found(
                ErrorCode::BASELINE_NOT_FOUND,
                format!("No baseline '{}' exists in this environment.", input.baseline_key),
            ),
            other => ApiError::from(other),
        })?;
    if !input.tools.is_empty() {
        let found = catalog::tools_by_key(scope, &input.tools)
            .await
            .map_err(ApiError::from)?;
        if found.len() != input.tools.len() {
            return Err(ApiError::unprocessable(
                ErrorCode::RECIPE_INVALID,
                "A tool key is not in the catalog.",
            ));
        }
    }
    let extra_packages = selection::packages(&input.extra_packages).map_err(ApiError::validation)?;
    let repositories = input
        .repositories
        .iter()
        .map(selection::repository)
        .collect::<Result<Vec<_>, _>>()
        .map_err(ApiError::validation)?;
    for key in &input.mcp_server_keys {
        let server = mcp_store::get_server(scope, key)
            .await
            .map_err(|error| match error {
                StoreError::NotFound => ApiError::unprocessable(
                    ErrorCode::RECIPE_INVALID,
                    format!("MCP server '{key}' is not in the registry."),
                ),
                other => ApiError::from(other),
            })?;
        if !server.enabled {
            return Err(ApiError::unprocessable(
                ErrorCode::RECIPE_INVALID,
                format!("MCP server '{key}' is disabled."),
            ));
        }
    }
    Ok(TemplateInput {
        name,
        description,
        baseline_key: input.baseline_key,
        tools: input.tools,
        extra_packages,
        repositories,
        mcp_server_keys: input.mcp_server_keys,
        is_default: input.is_default,
        is_shared: input.is_shared,
    })
}

/// Map a template read.
fn template_error(error: StoreError) -> ApiError {
    match error {
        StoreError::NotFound => ApiError::not_found(ErrorCode::NOT_FOUND, "No such template."),
        other => ApiError::from(other),
    }
}

/// Map a template write, where a duplicate name is a conflict.
fn write_error(error: StoreError) -> ApiError {
    match error {
        StoreError::UniqueViolation(_) => ApiError::conflict(
            ErrorCode::CONFLICT,
            "A template with that name already exists in this environment.",
        ),
        other => template_error(other),
    }
}

/// Map a recipe read or write.
fn recipe_error(error: StoreError) -> ApiError {
    match error {
        StoreError::NotFound => {
            ApiError::not_found(ErrorCode::RECIPE_NOT_FOUND, "No such recipe.")
        }
        other => ApiError::from(other),
    }
}

/// Record one template change in the audit trail.
async fn audit(
    scope: &mut TenantScope<'_>,
    principal: &Principal,
    kind: &str,
    subject_id: Uuid,
    detail: &str,
) -> Result<(), ApiError> {
    crate::store::postgres::audit::record(
        scope,
        crate::store::postgres::audit::NewAuditEvent {
            actor_id: Some(principal.user_id),
            actor_label: &principal.display_name,
            kind,
            subject_type: Some("image_template"),
            subject_id: Some(subject_id),
            detail: &serde_json::json!({ "name": detail }),
            request_id: None,
        },
    )
    .await
    .map_err(ApiError::from)
}
