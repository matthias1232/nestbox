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


/// Operator keys and temporary logins of one sandbox.
pub async fn ssh_access(
    State(state): State<AppState>,
    auth: Auth,
    Path(id): Path<Uuid>,
) -> Result<Json<access_service::SshAccessView>, ApiError> {
    Ok(Json(access_service::ssh_access(&state.services, &auth.principal, id).await?))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct AddKeyRequest {
    pub label: String,
    pub public_key: String,
}

pub async fn add_user_key(
    State(state): State<AppState>,
    auth: Auth,
    Path(id): Path<Uuid>,
    Json(body): Json<AddKeyRequest>,
) -> Result<(StatusCode, Json<crate::domain::ssh_access::UserKey>), ApiError> {
    let key = access_service::add_user_key(&state.services, &auth.principal, id, &body.label, &body.public_key).await?;
    Ok((StatusCode::CREATED, Json(key)))
}

pub async fn remove_user_key(
    State(state): State<AppState>,
    auth: Auth,
    Path((id, key)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, ApiError> {
    access_service::remove_user_key(&state.services, &auth.principal, id, key).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct IssueCredentialRequest {
    /// How long the login works, in minutes (1 to 1440; default 60).
    #[serde(default)]
    pub minutes: Option<i64>,
}

pub async fn issue_credential(
    State(state): State<AppState>,
    auth: Auth,
    Path(id): Path<Uuid>,
    Json(body): Json<IssueCredentialRequest>,
) -> Result<(StatusCode, Json<crate::domain::ssh_access::IssuedCredential>), ApiError> {
    let issued = access_service::issue_credential(&state.services, &auth.principal, id, body.minutes).await?;
    Ok((StatusCode::CREATED, Json(issued)))
}

pub async fn revoke_credential(
    State(state): State<AppState>,
    auth: Auth,
    Path((id, credential)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, ApiError> {
    access_service::revoke_credential(&state.services, &auth.principal, id, credential).await?;
    Ok(StatusCode::NO_CONTENT)
}


#[derive(Debug, Deserialize)]
pub struct SshSessionEvent {
    /// `open` or `close`.
    pub event: String,
    pub sandbox: String,
    #[serde(default)]
    pub session_id: Option<Uuid>,
    #[serde(default)]
    pub reason: Option<String>,
    #[serde(default)]
    pub bytes_in: i64,
    #[serde(default)]
    pub bytes_out: i64,
}

/// The runner reports an SSH session opening and closing, authenticated by the node
/// token. Who, when and how long are recorded (`sandbox_sessions`); never content.
pub async fn ssh_session_event(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    Json(body): Json<SshSessionEvent>,
) -> Result<Json<serde_json::Value>, ApiError> {
    use crate::store::TenantScope;
    use crate::store::postgres::access as store;
    let text = |name: &str| {
        headers.get(name).and_then(|v| v.to_str().ok()).map(|v| v.trim().to_string()).filter(|v| !v.is_empty())
    };
    let missing = || ApiError::unauthenticated("A node identifies itself with its id and token.");
    let node_id = text("x-nestbox-node").and_then(|v| Uuid::parse_str(&v).ok()).ok_or_else(missing)?;
    let environment_id = text("x-nestbox-environment").and_then(|v| Uuid::parse_str(&v).ok()).ok_or_else(missing)?;
    let token = text("authorization")
        .and_then(|raw| raw.split_once(' ').map(|(_, token)| token.trim().to_string()))
        .ok_or_else(missing)?;
    crate::services::policy_service::authenticate_node(&state.services, node_id, &token).await?;

    let mut scope = TenantScope::begin(&state.services.pool, environment_id).await.map_err(ApiError::from)?;
    let result = match body.event.as_str() {
        "open" => {
            let sandbox_id = crate::store::postgres::egress::sandbox_id_by_name(&state.services.pool, &body.sandbox)
                .await
                .map_err(ApiError::from)?
                .ok_or_else(|| ApiError::validation("No such sandbox."))?;
            let id = store::open_session(
                &mut scope,
                store::NewSession { sandbox_id, user_id: None, kind: crate::domain::access::SessionKind::Ssh, close_reason: None },
            )
            .await
            .map_err(ApiError::from)?;
            serde_json::json!({ "session_id": id })
        }
        "close" => {
            let id = body.session_id.ok_or_else(|| ApiError::validation("A close names its session."))?;
            store::close_session(&mut scope, id, body.reason.as_deref().unwrap_or("user"), body.bytes_in, body.bytes_out)
                .await
                .map_err(ApiError::from)?;
            serde_json::json!({ "closed": true })
        }
        _ => return Err(ApiError::validation("An event is open or close.")),
    };
    scope.commit().await.map_err(ApiError::from)?;
    Ok(Json(result))
}
