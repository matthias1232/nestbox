//! User administration (admin only).

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use serde::Deserialize;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::api::principal::Auth;
use crate::error::ApiError;
use crate::services::user_service::{self, UserView};
use crate::state::AppState;

/// The users of this environment, with their role and how many sandboxes they own.
pub async fn list(State(state): State<AppState>, auth: Auth) -> Result<Json<serde_json::Value>, ApiError> {
    let items = user_service::list(&state.services, &auth.principal).await?;
    Ok(Json(serde_json::json!({ "items": items })))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateUserRequest {
    pub email: String,
    pub display_name: String,
    /// At least 12 characters. Sent once, never returned.
    pub password: String,
    /// `admin`, `operator` or `viewer`.
    pub role: String,
}

/// Create a user in this environment.
pub async fn create(
    State(state): State<AppState>,
    auth: Auth,
    Json(body): Json<CreateUserRequest>,
) -> Result<(StatusCode, Json<UserView>), ApiError> {
    let user = user_service::create(
        &state.services,
        &auth.principal,
        &body.email,
        &body.display_name,
        &body.password,
        &body.role,
    )
    .await?;
    Ok((StatusCode::CREATED, Json(user)))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateUserRequest {
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub role: Option<String>,
    /// `active` or `disabled`.
    #[serde(default)]
    pub status: Option<String>,
}

/// Change a user's name, role or status.
pub async fn update(
    State(state): State<AppState>,
    auth: Auth,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateUserRequest>,
) -> Result<StatusCode, ApiError> {
    user_service::update(
        &state.services,
        &auth.principal,
        id,
        body.display_name.as_deref(),
        body.role.as_deref(),
        body.status.as_deref(),
    )
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct SetPasswordRequest {
    pub password: String,
}

/// Set a new password; the user's sessions end.
pub async fn set_password(
    State(state): State<AppState>,
    auth: Auth,
    Path(id): Path<Uuid>,
    Json(body): Json<SetPasswordRequest>,
) -> Result<StatusCode, ApiError> {
    user_service::set_password(&state.services, &auth.principal, id, &body.password).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Take a user out of this environment.
pub async fn remove(State(state): State<AppState>, auth: Auth, Path(id): Path<Uuid>) -> Result<StatusCode, ApiError> {
    user_service::remove(&state.services, &auth.principal, id).await?;
    Ok(StatusCode::NO_CONTENT)
}
