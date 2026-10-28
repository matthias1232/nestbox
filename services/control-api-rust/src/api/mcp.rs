//! MCP servers: the registry, a sandbox's servers, and discovery.
//!
//! A handler parses the request, names the principal and delegates. Every rule
//! — validation, the permission check, what discovery does — lives in
//! `services::mcp_service`, so a second caller (a future CLI, the console's
//! bulk action) gets the same answers.
//!
//! Transport is the fact these routes expose: an `http`/`sse` server is visible
//! in traffic, a `stdio` server is not (ADR 0026). The response says which is
//! which rather than leaving the console to infer it from an empty tool column.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use serde::Deserialize;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::api::principal::Auth;
use crate::domain::mcp::{McpServer, SandboxMcpServer};
use crate::error::ApiError;
use crate::services::mcp_service::{self, ServerInput};
use crate::state::AppState;

/// A server definition as a request body.
///
/// Strings rather than enums: `transport` and `auth_type` are validated in the
/// service, which answers 422 with the allowed values. A serde failure would
/// name a Rust type instead.
#[derive(Debug, Deserialize, ToSchema)]
pub struct ServerBody {
    /// Ignored on `PUT`, where the path decides the key. Required on `POST`.
    #[serde(default)]
    pub key: String,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub publisher: Option<String>,
    #[serde(default)]
    pub homepage: Option<String>,
    pub transport: String,
    #[serde(default)]
    pub endpoint: Option<String>,
    #[serde(default)]
    pub command: Option<String>,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub auth_type: String,
    #[serde(default)]
    pub env_slots: Vec<String>,
    #[serde(default)]
    pub packs: Vec<String>,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

fn default_enabled() -> bool {
    true
}

impl From<ServerBody> for ServerInput {
    fn from(body: ServerBody) -> Self {
        Self {
            key: body.key,
            name: body.name,
            description: body.description,
            publisher: body.publisher,
            homepage: body.homepage,
            transport: body.transport,
            endpoint: body.endpoint,
            command: body.command,
            args: body.args,
            auth_type: body.auth_type,
            env_slots: body.env_slots,
            packs: body.packs,
            enabled: body.enabled,
        }
    }
}

/// Every server in the registry.
pub async fn list_servers(
    State(state): State<AppState>,
    auth: Auth,
) -> Result<Json<serde_json::Value>, ApiError> {
    let servers = mcp_service::list(&state.services, &auth.principal).await?;
    Ok(Json(serde_json::json!({ "items": servers })))
}

/// Register a server definition.
pub async fn create_server(
    State(state): State<AppState>,
    auth: Auth,
    Json(body): Json<ServerBody>,
) -> Result<(StatusCode, Json<McpServer>), ApiError> {
    let server = mcp_service::create(&state.services, &auth.principal, body.into()).await?;
    Ok((StatusCode::CREATED, Json(server)))
}

/// One server definition.
pub async fn get_server(
    State(state): State<AppState>,
    auth: Auth,
    Path(key): Path<String>,
) -> Result<Json<McpServer>, ApiError> {
    let server = mcp_service::get(&state.services, &auth.principal, &key).await?;
    Ok(Json(server))
}

/// Replace a server definition. The key comes from the path.
pub async fn update_server(
    State(state): State<AppState>,
    auth: Auth,
    Path(key): Path<String>,
    Json(body): Json<ServerBody>,
) -> Result<Json<McpServer>, ApiError> {
    let server =
        mcp_service::replace(&state.services, &auth.principal, &key, body.into()).await?;
    Ok(Json(server))
}

/// Remove a server definition.
pub async fn delete_server(
    State(state): State<AppState>,
    auth: Auth,
    Path(key): Path<String>,
) -> Result<StatusCode, ApiError> {
    mcp_service::delete(&state.services, &auth.principal, &key).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// The servers attached to one sandbox.
pub async fn sandbox_servers(
    State(state): State<AppState>,
    auth: Auth,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let servers = mcp_service::sandbox_servers(&state.services, &auth.principal, id).await?;
    Ok(Json(serde_json::json!({ "items": servers })))
}

/// An attach request.
#[derive(Debug, Deserialize, ToSchema)]
pub struct AttachBody {
    /// The registry key, for example `nestbox-hub`.
    pub server_key: String,
    /// Detaching with `enabled: false` keeps the row and its discovered tools
    /// but tells the agent to stop using the server.
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

/// Attach a registry server to a sandbox.
pub async fn attach_server(
    State(state): State<AppState>,
    auth: Auth,
    Path(id): Path<Uuid>,
    Json(body): Json<AttachBody>,
) -> Result<(StatusCode, Json<SandboxMcpServer>), ApiError> {
    let attachment = mcp_service::attach(
        &state.services,
        &auth.principal,
        id,
        &body.server_key,
        body.enabled,
    )
    .await?;
    Ok((StatusCode::CREATED, Json(attachment)))
}

/// Detach a server from a sandbox.
pub async fn detach_server(
    State(state): State<AppState>,
    auth: Auth,
    Path((id, key)): Path<(Uuid, String)>,
) -> Result<StatusCode, ApiError> {
    mcp_service::detach(&state.services, &auth.principal, id, &key).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Ask a server for its tool list, through the same policy check as any egress.
///
/// The answer is the attachment state: `status=connected` with the discovered
/// tools, or `status=error` with a reason. A failed discovery is recorded
/// rather than returned as a 5xx, so the console can show why the server is not
/// usable instead of leaving the row at its previous, optimistic state.
pub async fn discover(
    State(state): State<AppState>,
    auth: Auth,
    Path((id, key)): Path<(Uuid, String)>,
) -> Result<Json<SandboxMcpServer>, ApiError> {
    let attachment =
        mcp_service::discover(&state.services, &auth.principal, id, &key).await?;
    Ok(Json(attachment))
}
