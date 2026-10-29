//! The secret store: write-only keys.
//!
//! A value goes in once; the answer, and every later read, carries only its last
//! four characters. There is no endpoint that returns a value.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use serde::Deserialize;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::api::principal::Auth;
use crate::error::ApiError;
use crate::services::secret_service;
use crate::state::AppState;
use crate::store::postgres::vault::SecretMeta;

/// Stored keys, without their values.
pub async fn list(State(state): State<AppState>, auth: Auth) -> Result<Json<serde_json::Value>, ApiError> {
    let items = secret_service::list(&state.services, &auth.principal).await?;
    Ok(Json(serde_json::json!({ "items": items })))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct PutSecretRequest {
    /// Lowercase letters, digits, `.`, `-`, `_`. A provider points at it as `db://<name>`.
    pub name: String,
    /// `api_key` (default), `token` or `password`.
    #[serde(default = "default_kind")]
    pub kind: String,
    /// The secret itself. Sent once; never returned.
    pub value: String,
}

fn default_kind() -> String {
    "api_key".to_string()
}

/// Store a key, or replace the value under an existing name.
pub async fn put(
    State(state): State<AppState>,
    auth: Auth,
    Json(request): Json<PutSecretRequest>,
) -> Result<(StatusCode, Json<SecretMeta>), ApiError> {
    let meta = secret_service::put(&state.services, &auth.principal, &request.name, &request.kind, &request.value).await?;
    Ok((StatusCode::CREATED, Json(meta)))
}

/// Delete a key nothing uses.
pub async fn delete(
    State(state): State<AppState>,
    auth: Auth,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    secret_service::delete(&state.services, &auth.principal, id).await?;
    Ok(StatusCode::NO_CONTENT)
}
