//! Reaching a sandbox from outside: SSH key, published ports, sessions.
//!
//! The key download is the only route that returns private material, and it is a
//! `GET` that answers exactly once. Every other route here returns the record,
//! which carries a fingerprint and never the key (ADR 0024).

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use serde::Deserialize;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::api::principal::Auth;
use crate::domain::access::{PublishedPort, SandboxSession, SshKey, SshKeyDownload};
use crate::error::ApiError;
use crate::services::access_service;
use crate::state::AppState;

/// The SSH key record: public half, fingerprint, whether it was downloaded.
pub async fn ssh_key(
    State(state): State<AppState>,
    auth: Auth,
    Path(id): Path<Uuid>,
) -> Result<Json<SshKey>, ApiError> {
    let key = access_service::key(&state.services, &auth.principal, id).await?;
    Ok(Json(key))
}

/// The one-time private key download. Answers 410 afterwards.
pub async fn download_ssh_key(
    State(state): State<AppState>,
    auth: Auth,
    Path(id): Path<Uuid>,
) -> Result<Json<SshKeyDownload>, ApiError> {
    let download = access_service::download(&state.services, &auth.principal, id).await?;
    Ok(Json(download))
}

/// Replace the key pair. The old private key file is deleted.
pub async fn rotate_ssh_key(
    State(state): State<AppState>,
    auth: Auth,
    Path(id): Path<Uuid>,
) -> Result<Json<SshKey>, ApiError> {
    let key = access_service::rotate(&state.services, &auth.principal, id).await?;
    Ok(Json(key))
}

/// Ports published on the node for this sandbox.
pub async fn ports(
    State(state): State<AppState>,
    auth: Auth,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<PublishedPort>>, ApiError> {
    let ports = access_service::ports(&state.services, &auth.principal, id).await?;
    Ok(Json(ports))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct AddPortRequest {
    /// The port the container listens on.
    pub target_port: i32,
    /// What the port is for, shown in the Access table.
    #[serde(default)]
    pub label: Option<String>,
}

/// Publish another port from the node's pool.
pub async fn add_port(
    State(state): State<AppState>,
    auth: Auth,
    Path(id): Path<Uuid>,
    Json(body): Json<AddPortRequest>,
) -> Result<(StatusCode, Json<PublishedPort>), ApiError> {
    let label = body
        .label
        .map(|label| label.trim().to_string())
        .filter(|label| !label.is_empty());
    let port = access_service::publish_port(
        &state.services,
        &auth.principal,
        id,
        body.target_port,
        label,
    )
    .await?;
    Ok((StatusCode::CREATED, Json(port)))
}

/// Withdraw a published port.
pub async fn remove_port(
    State(state): State<AppState>,
    auth: Auth,
    Path((sandbox, forward)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, ApiError> {
    access_service::withdraw_port(&state.services, &auth.principal, sandbox, forward).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Terminal and SSH sessions, for the audit trail.
pub async fn sessions(
    State(state): State<AppState>,
    auth: Auth,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<SandboxSession>>, ApiError> {
    let sessions = access_service::sessions(&state.services, &auth.principal, id).await?;
    Ok(Json(sessions))
}

/// How to reach this sandbox: the node's address and hostname, every published
/// port with its dial address, and the SSH key's state.
pub async fn connect(
    State(state): State<AppState>,
    auth: Auth,
    Path(id): Path<Uuid>,
) -> Result<Json<crate::domain::access::ConnectInfo>, ApiError> {
    Ok(Json(
        access_service::connect(&state.services, &auth.principal, id).await?,
    ))
}

/// The body of an endpoint change. `null` or empty clears a field.
#[derive(Debug, Deserialize, ToSchema)]
pub struct NodeEndpointRequest {
    #[serde(default)]
    pub public_address: Option<String>,
    #[serde(default)]
    pub public_hostname: Option<String>,
}

/// Set the address and hostname clients dial on a node.
pub async fn put_node_endpoint(
    State(state): State<AppState>,
    auth: Auth,
    Path(id): Path<Uuid>,
    Json(body): Json<NodeEndpointRequest>,
) -> Result<StatusCode, ApiError> {
    access_service::set_node_endpoint(
        &state.services,
        &auth.principal,
        id,
        body.public_address.as_deref(),
        body.public_hostname.as_deref(),
    )
    .await?;
    Ok(StatusCode::NO_CONTENT)
}
