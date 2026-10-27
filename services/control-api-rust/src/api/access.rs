//! Reaching a sandbox from outside: SSH key, published ports, sessions.
//!
//! The key download is the only route that returns private material, and it is a
//! `GET` that answers exactly once. The `feat/sandbox-access` package fills the
//! bodies.

use axum::Json;
use axum::extract::{Path, State};
use uuid::Uuid;

use crate::api::principal::Auth;
use crate::error::ApiError;
use crate::state::AppState;

/// The SSH key record: public half, fingerprint, whether it was downloaded.
pub async fn ssh_key(
    State(_state): State<AppState>,
    _auth: Auth,
    Path(_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
    Err(ApiError::not_implemented("GET /sandboxes/{id}/ssh-key"))
}

/// The one-time private key download. Answers 410 afterwards.
pub async fn download_ssh_key(
    State(_state): State<AppState>,
    _auth: Auth,
    Path(_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
    Err(ApiError::not_implemented(
        "GET /sandboxes/{id}/ssh-key/download",
    ))
}

/// Replace the key pair. The old private key file is deleted.
pub async fn rotate_ssh_key(
    State(_state): State<AppState>,
    _auth: Auth,
    Path(_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
    Err(ApiError::not_implemented(
        "POST /sandboxes/{id}/ssh-key/rotate",
    ))
}

/// Ports published on the node for this sandbox.
pub async fn ports(
    State(_state): State<AppState>,
    _auth: Auth,
    Path(_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
    Err(ApiError::not_implemented("GET /sandboxes/{id}/ports"))
}

/// Publish another port from the node's pool.
pub async fn add_port(
    State(_state): State<AppState>,
    _auth: Auth,
    Path(_id): Path<Uuid>,
    Json(_body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, ApiError> {
    Err(ApiError::not_implemented("POST /sandboxes/{id}/ports"))
}

/// Withdraw a published port.
pub async fn remove_port(
    State(_state): State<AppState>,
    _auth: Auth,
    Path((_sandbox, _forward)): Path<(Uuid, Uuid)>,
) -> Result<Json<serde_json::Value>, ApiError> {
    Err(ApiError::not_implemented(
        "DELETE /sandboxes/{id}/ports/{forward_id}",
    ))
}

/// Terminal and SSH sessions, for the audit trail.
pub async fn sessions(
    State(_state): State<AppState>,
    _auth: Auth,
    Path(_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
    Err(ApiError::not_implemented("GET /sandboxes/{id}/sessions"))
}
