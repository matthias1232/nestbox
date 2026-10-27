//! What is installed inside a sandbox.
//!
//! The snapshot comes from the fixed script in the runner image; a refresh asks
//! the runner to take a new one. The `feat/sandbox-metrics` package fills the
//! bodies.

use axum::Json;
use axum::extract::{Path, State};
use uuid::Uuid;

use crate::api::principal::Auth;
use crate::error::ApiError;
use crate::state::AppState;

/// The last inventory snapshot, with its age.
pub async fn get(
    State(_state): State<AppState>,
    _auth: Auth,
    Path(_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
    Err(ApiError::not_implemented("GET /sandboxes/{id}/inventory"))
}

/// Take a new inventory snapshot.
pub async fn refresh(
    State(_state): State<AppState>,
    _auth: Auth,
    Path(_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
    Err(ApiError::not_implemented(
        "POST /sandboxes/{id}/inventory/refresh",
    ))
}
