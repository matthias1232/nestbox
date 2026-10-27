//! Nodes: the machines sandboxes and models run on.
//!
//! Every handler here reads a request, delegates, and formats the answer; the
//! arithmetic that turns a node into a budget lives in
//! `services::node_service`.
//!
//! `probe` is the exception: measuring hardware is the model service's job,
//! because the same measurement decides which models fit. It goes through the
//! runner on that node, never through the control plane's own container.

use axum::Json;
use axum::extract::{Path, State};
use serde::Deserialize;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::api::principal::Auth;
use crate::domain::pagination::Page;
use crate::domain::sandbox::SandboxSummary;
use crate::error::ApiError;
use crate::services::model_service;
use crate::services::node_service::{self, NodeView};
use crate::state::AppState;

/// Every node with its budget and sandbox count.
pub async fn list(
    State(state): State<AppState>,
    auth: Auth,
) -> Result<Json<Vec<NodeView>>, ApiError> {
    let nodes = node_service::list(&state.services, &auth.principal).await?;
    Ok(Json(nodes))
}

/// One node.
pub async fn get(
    State(state): State<AppState>,
    auth: Auth,
    Path(id): Path<Uuid>,
) -> Result<Json<NodeView>, ApiError> {
    let node = node_service::get(&state.services, &auth.principal, id).await?;
    Ok(Json(node))
}

/// Sandboxes on a node.
///
/// The same page shape as `GET /sandboxes`, filtered to the node: the console
/// links here from the node drawer and must not have to learn a second list.
pub async fn sandboxes(
    State(state): State<AppState>,
    auth: Auth,
    Path(id): Path<Uuid>,
) -> Result<Json<Page<SandboxSummary>>, ApiError> {
    let page = node_service::sandboxes(&state.services, &auth.principal, id).await?;
    Ok(Json(page))
}

/// The fields a caller may change on a node.
///
/// Deliberately only the display fields: `name` is the stable identifier in
/// URLs, and status, kind, and `container_gpu` are measured by the runner.
#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateDisplayRequest {
    pub display_name: String,
    #[serde(default)]
    pub description: Option<String>,
}

/// Rename or describe a node.
pub async fn update_display(
    State(state): State<AppState>,
    auth: Auth,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateDisplayRequest>,
) -> Result<Json<NodeView>, ApiError> {
    let node = node_service::update_display(
        &state.services,
        &auth.principal,
        id,
        &body.display_name,
        body.description.as_deref(),
    )
    .await?;
    Ok(Json(node))
}

/// Probe a node's hardware through its runner.
///
/// The answer comes from the runner on the node, not from the control plane that
/// may share a host with it, and needs `node.manage`: this reaches a machine.
pub async fn probe(
    State(state): State<AppState>,
    auth: Auth,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let measured = model_service::probe_node(&state.services, &auth.principal, id).await?;
    Ok(Json(crate::api::models::hardware_json(
        &measured.profile,
        &measured.node,
    )))
}
