//! The launcher inside a sandbox that starts each agent in its chosen sign-in
//! mode (ADR 0054).
//!
//! `install` writes `$HOME/.nestbox/bin/<agent>` (one script, dispatching on its
//! own name), `nestbox-agentctl`, `.nestbox/env.sh` (PATH, proxy and CA trust) and
//! the hook in `.bashrc` / `.profile`. `write_mode` flips one agent's mode file.
//! Nothing restarts, nothing rebuilds: the next process of that agent reads it.
//!
//! Filled by package B01.

use std::collections::BTreeMap;

use super::ServiceContext;
use crate::domain::agent_auth::{AgentAuthMode, AgentKind};
use crate::error::ApiError;

/// Install or refresh the launcher, agentctl and environment in a sandbox.
pub async fn install(
    ctx: &ServiceContext,
    sandbox_name: &str,
    modes: &[(AgentKind, AgentAuthMode)],
    egress_env: &BTreeMap<String, String>,
) -> Result<(), ApiError> {
    let _ = (ctx, sandbox_name, modes, egress_env);
    tracing::debug!("agent launcher not built yet");
    Ok(())
}

/// Write one agent's mode file.
pub async fn write_mode(
    ctx: &ServiceContext,
    sandbox_name: &str,
    agent: AgentKind,
    mode: AgentAuthMode,
) -> Result<(), ApiError> {
    let _ = (ctx, sandbox_name, agent, mode);
    tracing::debug!("agent mode file not written yet");
    Ok(())
}
