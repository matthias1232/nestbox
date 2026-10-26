//! Endpunkte für Service-Bindings.

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;

use crate::domain::binding::{Binding, BindingInput};
use crate::error::{ApiError, ErrorEnvelope};
use crate::services::topology as service;
use crate::state::AppState;

#[utoipa::path(
    get,
    path = "/v1/bindings",
    tag = "topology",
    summary = "Alle deklarierten Service-Bindings",
    responses((status = 200, body = Vec<Binding>))
)]
pub async fn list(State(state): State<AppState>) -> Json<Vec<Binding>> {
    Json(service::list(&state).await)
}

#[utoipa::path(
    post,
    path = "/v1/bindings",
    tag = "topology",
    summary = "Service-Binding deklarieren",
    request_body = BindingInput,
    responses(
        (status = 201, body = Binding),
        (status = 409, body = ErrorEnvelope),
        (status = 422, body = ErrorEnvelope)
    )
)]
pub async fn create(
    State(state): State<AppState>,
    Json(input): Json<BindingInput>,
) -> Result<(StatusCode, Json<Binding>), ApiError> {
    let binding = service::create(&state, input).await?;
    Ok((StatusCode::CREATED, Json(binding)))
}
