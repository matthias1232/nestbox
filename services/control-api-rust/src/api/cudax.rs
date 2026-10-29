//! CUDA-X catalogue and per-node installation endpoints.

use axum::Json;
use axum::extract::{Path, Query, State};
use serde::Deserialize;
use serde_json::Value;

use crate::api::principal::Auth;
use crate::error::ApiError;
use crate::services::cudax_service;
use crate::state::AppState;

/// Query of the installation list.
#[derive(Debug, Deserialize)]
pub struct InstallsQuery {
    /// Only this node; every node when omitted.
    pub node: Option<String>,
}

/// Which node an action targets, and whether the operator overrode a warning.
#[derive(Debug, Default, Deserialize)]
pub struct NodeRequest {
    pub node: Option<String>,
    /// Recorded with the action. Nothing is blocked without it, but an explicit
    /// force makes the intent audit-visible.
    #[serde(default)]
    pub force: bool,
}

/// Every CUDA-X component the appliance knows.
pub async fn components(
    State(state): State<AppState>,
    auth: Auth,
) -> Result<Json<Value>, ApiError> {
    let items = cudax_service::components(&state.services, &auth.principal).await?;
    Ok(Json(serde_json::json!({ "items": items })))
}

/// Installations per node, with the assessment recorded at assignment time.
pub async fn installs(
    State(state): State<AppState>,
    auth: Auth,
    Query(query): Query<InstallsQuery>,
) -> Result<Json<Value>, ApiError> {
    let items = cudax_service::installs(
        &state.services,
        &auth.principal,
        query.node.as_deref(),
    )
    .await?;
    Ok(Json(serde_json::json!({ "items": items })))
}

/// Assign a component to a node; warnings are returned, never a refusal.
pub async fn assign(
    State(state): State<AppState>,
    auth: Auth,
    Path(key): Path<String>,
    body: Option<Json<NodeRequest>>,
) -> Result<Json<Value>, ApiError> {
    let request = body.map(|Json(value)| value).unwrap_or_default();
    let install = cudax_service::assign(
        &state.services,
        &auth.principal,
        &key,
        request.node.as_deref(),
        request.force,
    )
    .await?;
    Ok(Json(serde_json::to_value(install).unwrap_or(Value::Null)))
}

/// Install a component on a node, even when its assessment carries warnings.
pub async fn install(
    State(state): State<AppState>,
    auth: Auth,
    Path(key): Path<String>,
    body: Option<Json<NodeRequest>>,
) -> Result<Json<Value>, ApiError> {
    let request = body.map(|Json(value)| value).unwrap_or_default();
    Ok(Json(
        cudax_service::install(
            &state.services,
            &auth.principal,
            &key,
            request.node.as_deref(),
            request.force,
        )
        .await?,
    ))
}

/// Remove the assignment; installed software stays on the node.
pub async fn remove(
    State(state): State<AppState>,
    auth: Auth,
    Path(key): Path<String>,
    body: Option<Json<NodeRequest>>,
) -> Result<Json<Value>, ApiError> {
    let request = body.map(|Json(value)| value).unwrap_or_default();
    Ok(Json(
        cudax_service::remove(
            &state.services,
            &auth.principal,
            &key,
            request.node.as_deref(),
        )
        .await?,
    ))
}
