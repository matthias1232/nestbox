//! Resource history and the process table of one sandbox.
//!
//! Routes are registered here so the contract is fixed before the
//! implementation lands; the `feat/sandbox-metrics` package fills the bodies.
//! The ingest route is not session-authenticated: a node presents its own token,
//! so it carries no CSRF requirement and accepts samples only for the sandboxes
//! it runs.

use axum::Json;
use axum::extract::{Path, Query, State};
use serde::Deserialize;
use uuid::Uuid;

use crate::api::principal::Auth;
use crate::error::ApiError;
use crate::state::AppState;

/// Query for one sandbox's metrics.
#[derive(Debug, Deserialize)]
pub struct MetricsQuery {
    /// `1h`, `24h`, or `7d`. Defaults to `1h`.
    pub range: Option<String>,
    /// A step override in seconds. The server clamps it to the window.
    pub step: Option<u32>,
}

/// CPU, memory, network and disk over a window.
pub async fn metrics(
    State(_state): State<AppState>,
    _auth: Auth,
    Path(_id): Path<Uuid>,
    Query(_query): Query<MetricsQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    Err(ApiError::not_implemented("GET /sandboxes/{id}/metrics"))
}

/// Record a batch of samples from a node.
pub async fn ingest(
    State(_state): State<AppState>,
    Json(_batch): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, ApiError> {
    Err(ApiError::not_implemented("POST /internal/metrics/ingest"))
}

/// The sandbox's process table, read live through the runner.
pub async fn processes(
    State(_state): State<AppState>,
    _auth: Auth,
    Path(_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
    Err(ApiError::not_implemented("GET /sandboxes/{id}/processes"))
}
