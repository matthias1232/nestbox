//! Endpunkte für Sandboxes.

use axum::Json;
use axum::extract::{Path, Query, State};
use serde::Deserialize;
use utoipa::IntoParams;

use crate::domain::sandbox::{LifecycleRequest, Sandbox, SandboxInput, SandboxLogs};
use crate::error::{ApiError, ErrorEnvelope};
use crate::services::sandboxes as service;
use crate::state::AppState;

/// Query-Parameter für Logs.
#[derive(Debug, Deserialize, IntoParams)]
pub struct LogQuery {
    /// Anzahl der Zeilen (1–1000).
    pub tail: Option<u32>,
}

#[utoipa::path(
    get,
    path = "/v1/sandboxes",
    tag = "sandboxes",
    summary = "Alle verwalteten Sandboxes mit Live-Daten",
    responses((status = 200, body = Vec<Sandbox>))
)]
pub async fn list(State(state): State<AppState>) -> Json<Vec<Sandbox>> {
    Json(service::list(&state).await)
}

#[utoipa::path(
    get,
    path = "/v1/sandboxes/{id}",
    tag = "sandboxes",
    summary = "Eine Sandbox mit Live-Daten",
    params(("id" = String, Path, description = "ID oder Name der Sandbox")),
    responses(
        (status = 200, body = Sandbox),
        (status = 404, body = ErrorEnvelope)
    )
)]
pub async fn get(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Sandbox>, ApiError> {
    Ok(Json(service::get(&state, &id).await?))
}

#[utoipa::path(
    post,
    path = "/v1/sandboxes",
    tag = "sandboxes",
    summary = "Sandbox aus einem Pack erzeugen",
    request_body = SandboxInput,
    responses(
        (status = 201, body = Sandbox),
        (status = 409, body = ErrorEnvelope),
        (status = 422, body = ErrorEnvelope)
    )
)]
pub async fn create(
    State(state): State<AppState>,
    Json(input): Json<SandboxInput>,
) -> Result<(axum::http::StatusCode, Json<Sandbox>), ApiError> {
    let sandbox = service::create(&state, input).await?;
    Ok((axum::http::StatusCode::CREATED, Json(sandbox)))
}

#[utoipa::path(
    post,
    path = "/v1/sandboxes/{id}/lifecycle",
    tag = "sandboxes",
    summary = "Start, Stop, Restart oder Remove",
    params(("id" = String, Path, description = "ID oder Name der Sandbox")),
    request_body = LifecycleRequest,
    responses(
        (status = 200, body = Sandbox),
        (status = 404, body = ErrorEnvelope)
    )
)]
pub async fn lifecycle(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<LifecycleRequest>,
) -> Result<Json<Sandbox>, ApiError> {
    Ok(Json(service::lifecycle(&state, &id, request.action).await?))
}

#[utoipa::path(
    get,
    path = "/v1/sandboxes/{id}/logs",
    tag = "sandboxes",
    summary = "Letzte Logzeilen einer Sandbox",
    params(("id" = String, Path, description = "ID oder Name der Sandbox"), LogQuery),
    responses(
        (status = 200, body = SandboxLogs),
        (status = 404, body = ErrorEnvelope)
    )
)]
pub async fn logs(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<LogQuery>,
) -> Result<Json<SandboxLogs>, ApiError> {
    Ok(Json(service::logs(&state, &id, query.tail.unwrap_or(200)).await?))
}
