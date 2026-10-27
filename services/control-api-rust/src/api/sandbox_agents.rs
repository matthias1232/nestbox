//! Agents inside a sandbox.
//!
//! Handlers are filled by the `feat/sandbox-agents` package.

use axum::Json;
use axum::extract::{Path, State};
use uuid::Uuid;

use crate::api::principal::Auth;
use crate::error::ApiError;
use crate::state::AppState;

/// Agents of a sandbox.
pub async fn list(
    State(_state): State<AppState>,
    _auth: Auth,
    Path(_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
    Err(ApiError::not_implemented("GET /sandboxes/{id}/agents"))
}

/// Add an agent.
pub async fn add(
    State(_state): State<AppState>,
    _auth: Auth,
    Path(_id): Path<Uuid>,
    Json(_body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, ApiError> {
    Err(ApiError::not_implemented("POST /sandboxes/{id}/agents"))
}

/// Remove an agent.
pub async fn remove(
    State(_state): State<AppState>,
    _auth: Auth,
    Path((_id, _instance)): Path<(Uuid, String)>,
) -> Result<Json<serde_json::Value>, ApiError> {
    Err(ApiError::not_implemented("DELETE /sandboxes/{id}/agents/{instance}"))
}
