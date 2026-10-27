//! Agents inside a sandbox.
//!
//! A handler parses, delegates, and formats. Everything that decides — may this
//! pack join, may this instance take the name, what happens to the data — is in
//! `services::agent_service`.

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use serde::Deserialize;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::api::principal::Auth;
use crate::domain::builder::{HomeMode, Persistence};
use crate::error::ApiError;
use crate::services::agent_service::{self, AddAgentRequest, AgentView, SandboxAgents};
use crate::state::AppState;

/// Agents of a sandbox, with the command that starts them.
pub async fn list(
    State(state): State<AppState>,
    auth: Auth,
    Path(id): Path<Uuid>,
) -> Result<Json<SandboxAgents>, ApiError> {
    let agents = agent_service::list(&state.services, &auth.principal, id).await?;
    Ok(Json(agents))
}

/// A request to add one agent to a sandbox.
///
/// `home_mode` and `persistence` arrive as strings and are parsed here, so an
/// unknown value is a 422 with a message about the field rather than a database
/// error about a `CHECK`.
#[derive(Debug, Deserialize, ToSchema)]
pub struct AddRequest {
    pub pack_key: String,
    pub instance: String,
    #[serde(default = "default_home_mode")]
    pub home_mode: String,
    #[serde(default = "default_persistence")]
    pub persistence: String,
    /// The port the agent listens on, when it serves one.
    #[serde(default)]
    pub port: Option<u16>,
}

fn default_home_mode() -> String {
    "shared".to_string()
}

fn default_persistence() -> String {
    "ephemeral".to_string()
}

/// Add an agent.
pub async fn add(
    State(state): State<AppState>,
    auth: Auth,
    Path(id): Path<Uuid>,
    Json(body): Json<AddRequest>,
) -> Result<(StatusCode, Json<AgentView>), ApiError> {
    let home_mode = HomeMode::parse(body.home_mode.trim()).ok_or_else(|| {
        ApiError::validation("home_mode is 'shared' or 'isolated'.")
    })?;
    let persistence = Persistence::parse(body.persistence.trim()).ok_or_else(|| {
        ApiError::validation("persistence is 'persistent' or 'ephemeral'.")
    })?;

    let agent = agent_service::add(
        &state.services,
        &auth.principal,
        id,
        AddAgentRequest {
            pack_key: body.pack_key,
            instance: body.instance,
            home_mode,
            persistence,
            port: body.port,
        },
    )
    .await?;
    Ok((StatusCode::CREATED, Json(agent)))
}

/// Query parameters of a removal.
#[derive(Debug, Deserialize)]
pub struct RemoveQuery {
    /// Keep the persistent home and its data. Omitted means keep: a destructive
    /// default is a mistake waiting to happen.
    #[serde(default)]
    pub keep_persistent: Option<bool>,
}

/// Remove an agent, deciding what happens to its persistent home.
pub async fn remove(
    State(state): State<AppState>,
    auth: Auth,
    Path((id, instance)): Path<(Uuid, String)>,
    Query(query): Query<RemoveQuery>,
) -> Result<StatusCode, ApiError> {
    let keep_persistent = query.keep_persistent.unwrap_or(true);
    agent_service::remove(
        &state.services,
        &auth.principal,
        id,
        &instance,
        keep_persistent,
    )
    .await?;
    Ok(StatusCode::NO_CONTENT)
}
