//! What the agents in a sandbox are doing right now: their tmux panes, whether a
//! pane is in the middle of an answer, and how to restart one without losing the
//! conversation (ADR 0054).
//!
//! Filled by package B03.

use uuid::Uuid;

use super::ServiceContext;
use crate::domain::agent_auth::AgentSession;
use crate::error::ApiError;

/// The agent sessions in a sandbox, from its tmux panes.
pub async fn sessions(ctx: &ServiceContext, sandbox_name: &str) -> Result<Vec<AgentSession>, ApiError> {
    let _ = (ctx, sandbox_name);
    Ok(Vec::new())
}

/// Whether the sandbox talked to a model in the last 30 seconds.
pub async fn recent_model_traffic(
    ctx: &ServiceContext,
    environment_id: Uuid,
    sandbox_id: Uuid,
) -> Result<bool, ApiError> {
    let _ = (ctx, environment_id, sandbox_id);
    Ok(false)
}

/// Restart one pane, starting the agent again where it stopped.
pub async fn restart_session(
    ctx: &ServiceContext,
    sandbox_name: &str,
    session: &AgentSession,
) -> Result<(), ApiError> {
    let _ = (ctx, sandbox_name, session);
    Err(ApiError::not_implemented("restarting an agent pane"))
}

/// Restart a supervised agent (OpenChamber, a recipe agent) by its name.
pub async fn restart_supervised(
    ctx: &ServiceContext,
    sandbox_name: &str,
    name: &str,
) -> Result<(), ApiError> {
    let _ = (ctx, sandbox_name, name);
    Err(ApiError::not_implemented("restarting a supervised agent"))
}
