//! Nodes: the machines sandboxes and models run on.
//!
//! Handlers are filled by the `feat/nodes-api` package. The routes are
//! registered here so the contract is fixed before the implementation lands.

use axum::Json;
use axum::extract::{Path, State};
use uuid::Uuid;

use crate::api::principal::Auth;
use crate::error::ApiError;
use crate::state::AppState;

/// Every node with its budget and sandbox count.
pub async fn list(
    State(_state): State<AppState>,
    _auth: Auth,
) -> Result<Json<serde_json::Value>, ApiError> {
    Err(ApiError::not_implemented("GET /nodes"))
}

/// One node.
pub async fn get(
    State(_state): State<AppState>,
    _auth: Auth,
    Path(_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
    Err(ApiError::not_implemented("GET /nodes/{id}"))
}

/// Sandboxes on a node.
pub async fn sandboxes(
    State(_state): State<AppState>,
    _auth: Auth,
    Path(_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
    Err(ApiError::not_implemented("GET /nodes/{id}/sandboxes"))
}

/// Rename or describe a node.
pub async fn update_display(
    State(_state): State<AppState>,
    _auth: Auth,
    Path(_id): Path<Uuid>,
    Json(_body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, ApiError> {
    Err(ApiError::not_implemented("PUT /nodes/{id}/display"))
}

/// Probe a node's hardware through its runner.
pub async fn probe(
    State(_state): State<AppState>,
    _auth: Auth,
    Path(_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
    Err(ApiError::not_implemented("POST /nodes/{id}/probe"))
}
