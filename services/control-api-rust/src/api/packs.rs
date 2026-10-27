//! Baselines, agent packs, and the tool catalog.
//!
//! Handlers are filled by the `feat/packs-as-data` package.

use axum::Json;
use axum::extract::State;

use crate::api::principal::Auth;
use crate::error::ApiError;
use crate::state::AppState;

/// Base operating systems.
pub async fn baselines(
    State(_state): State<AppState>,
    _auth: Auth,
) -> Result<Json<serde_json::Value>, ApiError> {
    Err(ApiError::not_implemented("GET /baselines"))
}

/// Agent packs.
pub async fn agent_packs(
    State(_state): State<AppState>,
    _auth: Auth,
) -> Result<Json<serde_json::Value>, ApiError> {
    Err(ApiError::not_implemented("GET /agent-packs"))
}

/// The tool catalog.
pub async fn tool_catalog(
    State(_state): State<AppState>,
    _auth: Auth,
) -> Result<Json<serde_json::Value>, ApiError> {
    Err(ApiError::not_implemented("GET /tool-catalog"))
}
