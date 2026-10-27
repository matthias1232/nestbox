//! MCP servers: the registry, a sandbox's servers, and discovery.
//!
//! Transport is the fact the routes expose: an `http`/`sse` server is visible in
//! traffic, a `stdio` server is not. The `feat/mcp-registry` package fills the
//! bodies.

use axum::Json;
use axum::extract::{Path, State};
use uuid::Uuid;

use crate::api::principal::Auth;
use crate::error::ApiError;
use crate::state::AppState;

/// Every server in the registry.
pub async fn list_servers(
    State(_state): State<AppState>,
    _auth: Auth,
) -> Result<Json<serde_json::Value>, ApiError> {
    Err(ApiError::not_implemented("GET /mcp/servers"))
}

/// Register a server definition.
pub async fn create_server(
    State(_state): State<AppState>,
    _auth: Auth,
    Json(_body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, ApiError> {
    Err(ApiError::not_implemented("POST /mcp/servers"))
}

/// One server definition.
pub async fn get_server(
    State(_state): State<AppState>,
    _auth: Auth,
    Path(_key): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    Err(ApiError::not_implemented("GET /mcp/servers/{key}"))
}

/// Replace a server definition.
pub async fn update_server(
    State(_state): State<AppState>,
    _auth: Auth,
    Path(_key): Path<String>,
    Json(_body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, ApiError> {
    Err(ApiError::not_implemented("PUT /mcp/servers/{key}"))
}

/// Remove a server definition.
pub async fn delete_server(
    State(_state): State<AppState>,
    _auth: Auth,
    Path(_key): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    Err(ApiError::not_implemented("DELETE /mcp/servers/{key}"))
}

/// The servers attached to one sandbox.
pub async fn sandbox_servers(
    State(_state): State<AppState>,
    _auth: Auth,
    Path(_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
    Err(ApiError::not_implemented("GET /sandboxes/{id}/mcp/servers"))
}

/// Attach a registry server to a sandbox.
pub async fn attach_server(
    State(_state): State<AppState>,
    _auth: Auth,
    Path(_id): Path<Uuid>,
    Json(_body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, ApiError> {
    Err(ApiError::not_implemented("POST /sandboxes/{id}/mcp/servers"))
}

/// Detach a server from a sandbox.
pub async fn detach_server(
    State(_state): State<AppState>,
    _auth: Auth,
    Path((_id, _key)): Path<(Uuid, String)>,
) -> Result<Json<serde_json::Value>, ApiError> {
    Err(ApiError::not_implemented(
        "DELETE /sandboxes/{id}/mcp/servers/{key}",
    ))
}

/// Ask a server for its tool list, through the same policy check as any egress.
pub async fn discover(
    State(_state): State<AppState>,
    _auth: Auth,
    Path((_id, _key)): Path<(Uuid, String)>,
) -> Result<Json<serde_json::Value>, ApiError> {
    Err(ApiError::not_implemented(
        "POST /sandboxes/{id}/mcp/servers/{key}/discover",
    ))
}
