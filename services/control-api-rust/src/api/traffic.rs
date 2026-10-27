//! Traffic, the live map, effective policies, and the MCP calls the proxy saw.
//!
//! These are the read paths the Admin Center draws from. Each one takes a
//! window, returns a bounded page, and is aggregated on the server where the
//! console would otherwise count rows in the browser. The `feat/live-traffic`
//! package fills the bodies.

use axum::Json;
use axum::extract::{Path, Query, State};
use serde::Deserialize;
use uuid::Uuid;

use crate::api::principal::Auth;
use crate::error::ApiError;
use crate::state::AppState;

/// A window plus the filters the Traffic tab offers.
#[derive(Debug, Deserialize)]
pub struct TrafficQuery {
    /// `5m`, `15m`, `1h`, `24h`. Defaults to `5m`.
    pub window: Option<String>,
    /// `all`, `blocked`, `allowed`, `mcp`. Defaults to `all`.
    pub filter: Option<String>,
    pub host: Option<String>,
    /// Opaque cursor from the previous page.
    pub cursor: Option<String>,
    pub limit: Option<u32>,
}

/// Egress traffic rows for one sandbox.
pub async fn traffic(
    State(_state): State<AppState>,
    _auth: Auth,
    Path(_id): Path<Uuid>,
    Query(_query): Query<TrafficQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    Err(ApiError::not_implemented("GET /sandboxes/{id}/traffic"))
}

/// The redacted request and response body behind one traffic row.
pub async fn body(
    State(_state): State<AppState>,
    _auth: Auth,
    Path((_sandbox, _event)): Path<(Uuid, Uuid)>,
) -> Result<Json<serde_json::Value>, ApiError> {
    Err(ApiError::not_implemented(
        "GET /sandboxes/{id}/traffic/{event_id}/body",
    ))
}

/// Aggregated egress edges for the live map.
pub async fn map(
    State(_state): State<AppState>,
    _auth: Auth,
    Path(_id): Path<Uuid>,
    Query(_query): Query<TrafficQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    Err(ApiError::not_implemented("GET /sandboxes/{id}/map"))
}

/// The rules that apply to this sandbox, with the proxy's deployment state.
pub async fn effective_policies(
    State(_state): State<AppState>,
    _auth: Auth,
    Path(_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
    Err(ApiError::not_implemented(
        "GET /sandboxes/{id}/policies/effective",
    ))
}

/// MCP calls the proxy saw for this sandbox.
pub async fn mcp_calls(
    State(_state): State<AppState>,
    _auth: Auth,
    Path(_id): Path<Uuid>,
    Query(_query): Query<TrafficQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    Err(ApiError::not_implemented("GET /sandboxes/{id}/mcp/calls"))
}
