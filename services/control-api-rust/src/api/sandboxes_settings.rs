//! The operator settings document of one sandbox's supervisor.
//!
//! The document is opaque to this API on purpose: the supervisor owns its shape,
//! the console owns its editing, and the control plane only has to keep it safe
//! and keep it per sandbox. One row per supervisor means a settings change never
//! reaches a neighbour's sandbox.

use axum::Json;
use axum::extract::{Path, State};
use uuid::Uuid;

use crate::api::principal::Auth;
use crate::error::ApiError;
use crate::services::gateway_service;
use crate::state::AppState;

/// Read the settings document, `{}` when nothing has been set yet.
pub async fn get(
    State(state): State<AppState>,
    auth: Auth,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let settings = gateway_service::sandbox_settings(&state.services, &auth.principal, id).await?;
    Ok(Json(serde_json::json!({ "settings": settings })))
}

/// Replace the settings document. The previous version is not kept.
pub async fn update(
    State(state): State<AppState>,
    auth: Auth,
    Path(id): Path<Uuid>,
    Json(settings): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let settings =
        gateway_service::update_sandbox_settings(&state.services, &auth.principal, id, &settings)
            .await?;
    Ok(Json(settings))
}

/// One action the supervisor may be asked to run.
#[derive(Debug, serde::Deserialize, utoipa::ToSchema)]
pub struct ActionRequest {
    /// `apply_policy`, `restart_agent`, or `run_check`.
    pub kind: String,
    #[serde(default)]
    pub payload: serde_json::Value,
}

/// Queue a whitelisted action for this sandbox's supervisor.
///
/// Accepted means "written into the sandbox", not "done": the supervisor
/// executes on its next tick and its result is visible in the report.
pub async fn action(
    State(state): State<AppState>,
    auth: Auth,
    Path(id): Path<Uuid>,
    Json(body): Json<ActionRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let outcome = gateway_service::queue_supervisor_action(
        &state.services,
        &auth.principal,
        id,
        &body.kind,
        &body.payload,
    )
    .await?;
    Ok(Json(outcome))
}
