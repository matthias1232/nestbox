//! Systemendpunkte: Health, Runtime-Status, Schema, Audit-Events.

use axum::Json;
use axum::extract::State;
use serde::Serialize;
use utoipa::{OpenApi, ToSchema};

use crate::audit::AuditEvent;
use crate::error::ApiError;
use crate::openapi::ApiDoc;
use crate::state::AppState;

/// Antwort von `/v1/healthz`.
#[derive(Debug, Serialize, ToSchema)]
pub struct HealthInfo {
    pub status: String,
    pub implementation: String,
    pub api_version: String,
    pub mode: String,
}

/// Antwort von `/v1/runtime`.
#[derive(Debug, Serialize, ToSchema)]
pub struct RuntimeStatus {
    pub status: String,
    pub adapter: String,
    pub network: String,
    pub implementation: String,
}

#[utoipa::path(
    get,
    path = "/v1/healthz",
    tag = "system",
    summary = "Liveness und Implementierungskennung",
    responses((status = 200, body = HealthInfo))
)]
pub async fn health() -> Json<HealthInfo> {
    Json(HealthInfo {
        status: "ok".to_string(),
        implementation: "rust-axum".to_string(),
        api_version: "v1".to_string(),
        mode: "phase-1-reference".to_string(),
    })
}

#[utoipa::path(
    get,
    path = "/v1/runtime",
    tag = "system",
    summary = "Status des Runtime-Adapters",
    responses((status = 200, body = RuntimeStatus))
)]
pub async fn runtime_status(State(state): State<AppState>) -> Json<RuntimeStatus> {
    let reachable = state.runtime.health().await.is_ok();
    Json(RuntimeStatus {
        status: if reachable { "ready" } else { "unavailable" }.to_string(),
        adapter: state.runtime.name().to_string(),
        network: state.runtime.network().to_string(),
        implementation: "rust-axum".to_string(),
    })
}

#[utoipa::path(
    get,
    path = "/v1/events",
    tag = "system",
    summary = "Audit-Ereignisse (neueste zuletzt)",
    responses((status = 200, body = Vec<AuditEvent>))
)]
pub async fn events(State(state): State<AppState>) -> Json<Vec<AuditEvent>> {
    Json(state.data.read().await.events.clone())
}

#[utoipa::path(
    get,
    path = "/v1/schema",
    tag = "system",
    summary = "OpenAPI-Dokument, erzeugt aus den Typen",
    responses((status = 200, body = Object))
)]
pub async fn schema() -> Result<Json<utoipa::openapi::OpenApi>, ApiError> {
    Ok(Json(ApiDoc::openapi()))
}
