//! Image templates: the builder's saved selections.
//!
//! A handler parses the body and delegates; the sharing rule, the baseline and
//! the catalog validation live in `services::template_service`, so a second
//! caller — a CLI, a scheduled task — gets the same answers.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use serde::Deserialize;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::api::principal::Auth;
use crate::domain::builder::{ImageTemplate, RepositorySpec};
use crate::error::ApiError;
use crate::services::template_service::{self, TemplateInput};
use crate::state::AppState;

/// A create or replace body: the whole selection, never a patch.
#[derive(Debug, Deserialize, ToSchema)]
pub struct TemplateBody {
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub baseline_key: String,
    #[serde(default)]
    pub tools: Vec<String>,
    #[serde(default)]
    pub extra_packages: Vec<String>,
    #[serde(default)]
    pub repositories: Vec<RepositorySpec>,
    #[serde(default)]
    pub mcp_server_keys: Vec<String>,
    #[serde(default)]
    pub is_default: bool,
    #[serde(default = "default_shared")]
    pub is_shared: bool,
}

/// Sharing is on unless the operator says otherwise: a template that is useful
/// only to its author is a recipe, and the point of the feature is reuse.
fn default_shared() -> bool {
    true
}

impl TemplateBody {
    fn into_input(self) -> TemplateInput {
        TemplateInput {
            name: self.name,
            description: self.description,
            baseline_key: self.baseline_key,
            tools: self.tools,
            extra_packages: self.extra_packages,
            repositories: self.repositories,
            mcp_server_keys: self.mcp_server_keys,
            is_default: self.is_default,
            is_shared: self.is_shared,
        }
    }
}

/// Every template this actor may use.
pub async fn list(
    State(state): State<AppState>,
    auth: Auth,
) -> Result<Json<serde_json::Value>, ApiError> {
    let templates = template_service::list(&state.services, &auth.principal).await?;
    Ok(Json(serde_json::json!({ "items": templates })))
}

/// The environment's default template, if one is set.
pub async fn default_template(
    State(state): State<AppState>,
    auth: Auth,
) -> Result<Json<serde_json::Value>, ApiError> {
    let template = template_service::default_for(&state.services, &auth.principal).await?;
    Ok(Json(serde_json::json!({ "template": template })))
}

/// Create a template.
pub async fn create(
    State(state): State<AppState>,
    auth: Auth,
    Json(body): Json<TemplateBody>,
) -> Result<(StatusCode, Json<ImageTemplate>), ApiError> {
    let template =
        template_service::create(&state.services, &auth.principal, body.into_input()).await?;
    Ok((StatusCode::CREATED, Json(template)))
}

/// Read a template.
pub async fn get(
    State(state): State<AppState>,
    auth: Auth,
    Path(id): Path<Uuid>,
) -> Result<Json<ImageTemplate>, ApiError> {
    Ok(Json(
        template_service::get(&state.services, &auth.principal, id).await?,
    ))
}

/// Replace a template's selection.
pub async fn update(
    State(state): State<AppState>,
    auth: Auth,
    Path(id): Path<Uuid>,
    Json(body): Json<TemplateBody>,
) -> Result<Json<ImageTemplate>, ApiError> {
    Ok(Json(
        template_service::update(&state.services, &auth.principal, id, body.into_input()).await?,
    ))
}

/// Delete a template. Recipes keep the copy they applied.
pub async fn delete(
    State(state): State<AppState>,
    auth: Auth,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    template_service::delete(&state.services, &auth.principal, id).await?;
    Ok(StatusCode::NO_CONTENT)
}
