//! Persistent roots and mounts.
//!
//! Handlers are filled by the `feat/persistent-roots` package.

use axum::Json;
use axum::extract::{Path, State};
use uuid::Uuid;

use crate::api::principal::Auth;
use crate::error::ApiError;
use crate::state::AppState;

/// List roots.
pub async fn list(
    State(_state): State<AppState>,
    _auth: Auth,
) -> Result<Json<serde_json::Value>, ApiError> {
    Err(ApiError::not_implemented("GET /persistent-roots"))
}

/// Create a root.
pub async fn create(
    State(_state): State<AppState>,
    _auth: Auth,
    Json(_body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, ApiError> {
    Err(ApiError::not_implemented("POST /persistent-roots"))
}

/// One root.
pub async fn get(
    State(_state): State<AppState>,
    _auth: Auth,
    Path(_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
    Err(ApiError::not_implemented("GET /persistent-roots/{id}"))
}

/// Archive or delete a root.
pub async fn delete(
    State(_state): State<AppState>,
    _auth: Auth,
    Path(_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
    Err(ApiError::not_implemented("DELETE /persistent-roots/{id}"))
}

/// Mount into a sandbox.
pub async fn mount(
    State(_state): State<AppState>,
    _auth: Auth,
    Path(_id): Path<Uuid>,
    Json(_body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, ApiError> {
    Err(ApiError::not_implemented("POST /persistent-roots/{id}/mount"))
}

/// Dismount from a sandbox.
pub async fn dismount(
    State(_state): State<AppState>,
    _auth: Auth,
    Path(_id): Path<Uuid>,
    Json(_body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, ApiError> {
    Err(ApiError::not_implemented("POST /persistent-roots/{id}/dismount"))
}

/// Disk usage.
pub async fn usage(
    State(_state): State<AppState>,
    _auth: Auth,
    Path(_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
    Err(ApiError::not_implemented("GET /persistent-roots/{id}/usage"))
}
