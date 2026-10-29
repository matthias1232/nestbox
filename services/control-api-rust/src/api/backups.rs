//! Backups of persistent roots.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use serde::Deserialize;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::api::principal::Auth;
use crate::error::ApiError;
use crate::services::backup_service;
use crate::state::AppState;
use crate::store::postgres::backups::Backup;

/// Every backup in the environment, newest first.
pub async fn list_all(State(state): State<AppState>, auth: Auth) -> Result<Json<serde_json::Value>, ApiError> {
    let items = backup_service::list(&state.services, &auth.principal, None).await?;
    Ok(Json(serde_json::json!({ "items": items })))
}

/// The backups of one root.
pub async fn list_for_root(
    State(state): State<AppState>,
    auth: Auth,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let items = backup_service::list(&state.services, &auth.principal, Some(id)).await?;
    Ok(Json(serde_json::json!({ "items": items })))
}

#[derive(Debug, Default, Deserialize, ToSchema)]
pub struct CreateBackupRequest {
    /// A short reminder of why, up to 200 characters.
    #[serde(default)]
    pub note: Option<String>,
}

/// Back up one root now.
pub async fn create(
    State(state): State<AppState>,
    auth: Auth,
    Path(id): Path<Uuid>,
    body: Option<Json<CreateBackupRequest>>,
) -> Result<(StatusCode, Json<Backup>), ApiError> {
    let note = body.and_then(|Json(body)| body.note);
    let backup = backup_service::create(&state.services, &auth.principal, id, note.as_deref()).await?;
    Ok((StatusCode::CREATED, Json(backup)))
}

#[derive(Debug, Default, Deserialize, ToSchema)]
pub struct RestoreRequest {
    /// The root to restore into; by default the one the backup came from.
    #[serde(default)]
    pub root_id: Option<Uuid>,
    /// Move the current content aside first, so the root ends up exactly as it was.
    #[serde(default)]
    pub replace: bool,
    /// Restore even though a running sandbox has the root mounted read-write.
    #[serde(default)]
    pub force: bool,
}

/// Restore a backup.
pub async fn restore(
    State(state): State<AppState>,
    auth: Auth,
    Path(id): Path<Uuid>,
    body: Option<Json<RestoreRequest>>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let body = body.map(|Json(body)| body).unwrap_or_default();
    let answer =
        backup_service::restore(&state.services, &auth.principal, id, body.root_id, body.replace, body.force).await?;
    Ok(Json(answer))
}

/// Delete a backup and its archive.
pub async fn delete(State(state): State<AppState>, auth: Auth, Path(id): Path<Uuid>) -> Result<StatusCode, ApiError> {
    backup_service::delete(&state.services, &auth.principal, id).await?;
    Ok(StatusCode::NO_CONTENT)
}
