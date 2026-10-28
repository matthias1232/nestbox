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
    Ok(Json(settings))
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
