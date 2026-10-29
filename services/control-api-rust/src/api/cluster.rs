//! Gateway, supervisor, and topology endpoints.

use axum::Json;
use axum::http::StatusCode;
use axum::extract::{Path, State};
use serde::Deserialize;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::api::principal::Auth;
use crate::error::ApiError;
use crate::services::gateway_service;
use crate::state::AppState;

/// List gateways.
pub async fn list_gateways(
    State(state): State<AppState>,
    auth: Auth,
) -> Result<Json<serde_json::Value>, ApiError> {
    let gateways = gateway_service::list(&state.services, &auth.principal).await?;
    Ok(Json(serde_json::json!({ "items": gateways })))
}

/// One gateway.
pub async fn get_gateway(
    State(state): State<AppState>,
    auth: Auth,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let gateway = gateway_service::get(&state.services, &auth.principal, id).await?;
    Ok(Json(serde_json::json!({
        "gateway": gateway,
        "drifted": gateway.is_drifted(),
        "free_capacity": gateway.free_capacity(),
    })))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct SettingsRequest {
    pub settings: serde_json::Value,
    pub version: i64,
}

/// Replace a gateway's operator configuration.
///
/// Leaves the applied version untouched on purpose: that difference is the
/// drift the sync button resolves.
pub async fn update_settings(
    State(state): State<AppState>,
    auth: Auth,
    Path(id): Path<Uuid>,
    Json(body): Json<SettingsRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let gateway = gateway_service::update_settings(
        &state.services,
        &auth.principal,
        id,
        body.version,
        &body.settings,
    )
    .await?;
    Ok(Json(serde_json::json!({
        "gateway": gateway,
        "drifted": gateway.is_drifted(),
    })))
}

/// Push the desired configuration and clear drift.
pub async fn sync(
    State(state): State<AppState>,
    auth: Auth,
    Path(id): Path<Uuid>,
) -> Result<Json<crate::domain::gateway::SyncOutcome>, ApiError> {
    Ok(Json(
        gateway_service::sync(&state.services, &auth.principal, id).await?,
    ))
}

/// The body of a supervisor registration.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct CreateSupervisorRequest {
    /// A slug, unique per gateway.
    pub name: String,
    /// The sandbox this supervisor watches, when it has one.
    #[serde(default)]
    pub sandbox_id: Option<Uuid>,
}

/// Register a supervisor for a sandbox.
pub async fn create_supervisor(
    State(state): State<AppState>,
    auth: Auth,
    Json(body): Json<CreateSupervisorRequest>,
) -> Result<(StatusCode, Json<crate::domain::gateway::Supervisor>), ApiError> {
    let supervisor = gateway_service::create_supervisor(
        &state.services,
        &auth.principal,
        &body.name,
        body.sandbox_id,
    )
    .await?;
    Ok((StatusCode::CREATED, Json(supervisor)))
}

/// A report from the supervisor in a sandbox.
///
/// No session: the caller is the process inside the sandbox, and the token it
/// presents is checked inside the service. The body is the measurement, not a
/// claim about the control plane.
pub async fn report_supervisor(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    headers: axum::http::HeaderMap,
    Json(report): Json<crate::domain::gateway::SupervisorReport>,
) -> Result<Json<crate::domain::gateway::Supervisor>, ApiError> {
    let token = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer ").or_else(|| value.strip_prefix("bearer ")))
        .unwrap_or_default();
    let supervisor =
        gateway_service::record_report(&state.services, token, id, report).await?;
    Ok(Json(supervisor))
}

/// The body of a re-bind.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct BindSupervisorRequest {
    pub sandbox_id: Uuid,
}

/// Bind a supervisor to a sandbox.
pub async fn bind_supervisor(
    State(state): State<AppState>,
    auth: Auth,
    Path(id): Path<Uuid>,
    Json(body): Json<BindSupervisorRequest>,
) -> Result<Json<crate::domain::gateway::Supervisor>, ApiError> {
    let supervisor = gateway_service::bind_supervisor(
        &state.services,
        &auth.principal,
        id,
        body.sandbox_id,
    )
    .await?;
    Ok(Json(supervisor))
}

/// List supervisors.
pub async fn supervisors(
    State(state): State<AppState>,
    auth: Auth,
) -> Result<Json<serde_json::Value>, ApiError> {
    let supervisors = gateway_service::supervisors(&state.services, &auth.principal).await?;
    Ok(Json(serde_json::json!({ "items": supervisors })))
}

/// The supervisor of one sandbox, as the gateway and the supervisor report it.
pub async fn sandbox_supervisor(
    State(state): State<AppState>,
    auth: Auth,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let view = gateway_service::supervisor_view(&state.services, &auth.principal, id).await?;
    Ok(Json(view))
}
