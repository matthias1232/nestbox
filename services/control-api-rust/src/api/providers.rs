//! Endpunkte für Provider-Profile.

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;

use crate::domain::provider::{ProviderInput, ProviderProfile};
use crate::error::{ApiError, ErrorEnvelope};
use crate::services::providers as service;
use crate::state::AppState;

#[utoipa::path(
    get,
    path = "/v1/providers",
    tag = "providers",
    summary = "Provider-Profile (ohne Secrets)",
    responses((status = 200, body = Vec<ProviderProfile>))
)]
pub async fn list(State(state): State<AppState>) -> Json<Vec<ProviderProfile>> {
    Json(service::list(&state).await)
}

#[utoipa::path(
    post,
    path = "/v1/providers",
    tag = "providers",
    summary = "OpenAI-kompatibles Provider-Profil anlegen",
    request_body = ProviderInput,
    responses(
        (status = 201, body = ProviderProfile),
        (status = 409, body = ErrorEnvelope),
        (status = 422, body = ErrorEnvelope)
    )
)]
pub async fn create(
    State(state): State<AppState>,
    Json(input): Json<ProviderInput>,
) -> Result<(StatusCode, Json<ProviderProfile>), ApiError> {
    let profile = service::create(&state, input).await?;
    Ok((StatusCode::CREATED, Json(profile)))
}
