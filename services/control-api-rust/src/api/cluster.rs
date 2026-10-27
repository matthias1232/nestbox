//! Gateway, supervisor, and topology endpoints.

use axum::Json;
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

/// List supervisors.
pub async fn supervisors(
    State(state): State<AppState>,
    auth: Auth,
) -> Result<Json<serde_json::Value>, ApiError> {
    let supervisors = gateway_service::supervisors(&state.services, &auth.principal).await?;
    Ok(Json(serde_json::json!({ "items": supervisors })))
}
