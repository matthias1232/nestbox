//! Switching an agent between the user's own subscription and the gateway
//! (ADR 0054).
//!
//! `modes_for` answers the mode of every switchable agent (the pack's
//! `login_agents`, plus the agents of a recipe sandbox); `list` adds what the
//! agents are doing; `set` stores the choice, rewrites the launcher mode file and
//! the agent config, keeps the sign-in firewall rules in step and restarts the
//! agent sessions according to the policy. `run_pending_restarts` finishes a
//! restart that waited for a busy agent to become idle.
//!
//! `set`, `restart`, `sync_login_rules` and `run_pending_restarts` are filled by
//! package B02.

use uuid::Uuid;

use super::ServiceContext;
use crate::domain::agent_auth::{AgentAuthMode, AgentKind, AgentRuntimeState, RestartPolicy};
use crate::domain::auth::Principal;
use crate::error::ApiError;
use crate::store::TenantScope;

/// The chosen mode of every switchable agent of a sandbox, defaulting from the
/// sandbox's old create-time login choice.
pub async fn modes_for(
    ctx: &ServiceContext,
    environment_id: Uuid,
    sandbox_id: Uuid,
) -> Result<Vec<(AgentKind, AgentAuthMode)>, ApiError> {
    let mut scope = TenantScope::begin(&ctx.pool, environment_id)
        .await
        .map_err(ApiError::from)?;
    let sandbox = crate::store::postgres::sandboxes::get(&mut scope, sandbox_id)
        .await
        .map_err(ApiError::from)?;
    let login = sandbox.spec.get("login").and_then(|value| value.as_str());
    let mut agents = crate::store::postgres::agent_auth::pack_agents(&mut scope, &sandbox.pack_key)
        .await
        .map_err(ApiError::from)?;
    for pack in crate::store::postgres::agent_auth::sandbox_agent_packs(&mut scope, sandbox_id)
        .await
        .map_err(ApiError::from)?
    {
        for agent in crate::store::postgres::agent_auth::pack_agents(&mut scope, &pack)
            .await
            .map_err(ApiError::from)?
        {
            if !agents.contains(&agent) {
                agents.push(agent);
            }
        }
    }
    let rows = crate::store::postgres::agent_auth::list(&mut scope, sandbox_id)
        .await
        .map_err(ApiError::from)?;
    scope.commit().await.map_err(ApiError::from)?;

    let default = crate::domain::agent_auth::default_mode(login);
    let mut out: Vec<(AgentKind, AgentAuthMode)> = Vec::new();
    for agent in AgentKind::ALL {
        if !agents.contains(&agent) {
            continue;
        }
        let mode = rows
            .iter()
            .find(|row| row.agent == agent)
            .map(|row| row.mode)
            .unwrap_or(default);
        out.push((agent, mode));
    }
    Ok(out)
}

/// Every switchable agent with its mode, sessions and login state.
pub async fn list(
    ctx: &ServiceContext,
    principal: &Principal,
    sandbox_id: Uuid,
) -> Result<Vec<AgentRuntimeState>, ApiError> {
    let _ = (ctx, principal, sandbox_id);
    Ok(Vec::new())
}

/// Switch one agent and restart it as the policy says.
pub async fn set(
    ctx: &ServiceContext,
    principal: &Principal,
    sandbox_id: Uuid,
    agent: AgentKind,
    mode: AgentAuthMode,
    restart: RestartPolicy,
) -> Result<Vec<AgentRuntimeState>, ApiError> {
    let _ = (ctx, principal, sandbox_id, agent, mode, restart);
    Err(ApiError::not_implemented("switching an agent's sign-in"))
}

/// Restart one agent without changing its mode.
pub async fn restart(
    ctx: &ServiceContext,
    principal: &Principal,
    sandbox_id: Uuid,
    agent: AgentKind,
    when: RestartPolicy,
) -> Result<Vec<AgentRuntimeState>, ApiError> {
    let _ = (ctx, principal, sandbox_id, agent, when);
    Err(ApiError::not_implemented("restarting an agent"))
}

/// Keep the sign-in firewall rules in step with the agents' modes.
pub async fn sync_login_rules(
    ctx: &ServiceContext,
    environment_id: Uuid,
    sandbox_id: Uuid,
) -> Result<(), ApiError> {
    let _ = (ctx, environment_id, sandbox_id);
    Ok(())
}

/// Restart agents that waited for a busy session to become idle.
pub async fn run_pending_restarts(ctx: &ServiceContext) -> Result<(), ApiError> {
    let _ = ctx;
    Ok(())
}
