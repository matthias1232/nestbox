//! The stack diagram and the appliance's own components.
//!
//! `GET /sandboxes/{id}/stack-topology` answers for the whole stack the sandbox
//! belongs to, so the lead and a member show the same picture. The component
//! routes make the diagram's system cards clickable: their state and their log.

use axum::Json;
use axum::extract::{Path, Query, State};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::api::principal::Auth;
use crate::domain::runtime::ComponentState;
use crate::domain::stack_topology::StackTopology;
use crate::error::ApiError;
use crate::services::stack_topology_service;
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct StackQuery {
    /// The window the egress traffic is summed over, e.g. `5m`.
    pub window: Option<String>,
}

/// The stack a sandbox belongs to, from the operator to the internet.
pub async fn stack_topology(
    State(state): State<AppState>,
    auth: Auth,
    Path(id): Path<Uuid>,
    Query(query): Query<StackQuery>,
) -> Result<Json<StackTopology>, ApiError> {
    let topology =
        stack_topology_service::stack(&state.services, &auth.principal, id, query.window.as_deref())
            .await?;
    Ok(Json(topology))
}

/// The appliance's containers: control API, gateway, runner, proxy, database.
pub async fn components(
    State(state): State<AppState>,
    auth: Auth,
) -> Result<Json<Vec<ComponentState>>, ApiError> {
    Ok(Json(stack_topology_service::components(&state.services, &auth.principal).await?))
}

#[derive(Debug, Deserialize)]
pub struct ComponentLogQuery {
    #[serde(default = "default_tail")]
    pub tail: u32,
}

fn default_tail() -> u32 {
    200
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ComponentLog {
    pub service: String,
    pub content: String,
}

/// The last lines of one component's log.
pub async fn component_logs(
    State(state): State<AppState>,
    auth: Auth,
    Path(service): Path<String>,
    Query(query): Query<ComponentLogQuery>,
) -> Result<Json<ComponentLog>, ApiError> {
    let content = stack_topology_service::component_logs(
        &state.services,
        &auth.principal,
        &service,
        query.tail,
    )
    .await?;
    Ok(Json(ComponentLog { service, content }))
}
