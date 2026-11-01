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
//! Nothing here rebuilds a container: the mode is a file the launcher reads, so a
//! change reaches the next process of that agent within seconds, and a session
//! that is in the middle of an answer keeps it.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::{Mutex, OnceLock};

use uuid::Uuid;

use super::{
    agent_activity_service, agent_launch_service, policy_service, require, sandbox_service,
    session_service, ServiceContext,
};
use crate::domain::agent_auth::{
    login_hosts, AgentAuthMode, AgentKind, AgentRuntimeState, AgentSession, RestartPolicy,
};
use crate::domain::auth::{Permission, Principal};
use crate::domain::sandbox::{Sandbox, SandboxStatus};
use crate::error::ApiError;
use crate::store::TenantScope;
use crate::store::postgres::agent_auth as auth_store;
use crate::store::postgres::login_rules;
use crate::store::postgres::persistent_roots;
use crate::store::postgres::sandboxes as sandbox_store;

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
    require(principal, Permission::SandboxRead)?;
    let (sandbox, rows) = {
        let mut scope = TenantScope::begin_for(&ctx.pool, principal)
            .await
            .map_err(ApiError::from)?;
        let sandbox = sandbox_store::get(&mut scope, sandbox_id)
            .await
            .map_err(ApiError::from)?;
        let rows = auth_store::list(&mut scope, sandbox_id)
            .await
            .map_err(ApiError::from)?;
        scope.commit().await.map_err(ApiError::from)?;
        (sandbox, rows)
    };
    let modes = modes_for(ctx, sandbox.environment_id, sandbox_id).await?;

    // A stopped sandbox has nothing to look at, and asking the runtime would only
    // produce an error per field. It is listed with no sessions rather than not
    // listed at all, because the console still has to show the chosen modes.
    let mut sessions: Vec<AgentSession> = Vec::new();
    let mut supervised: Vec<SupervisedAgent> = Vec::new();
    let mut accounts: BTreeMap<String, Option<String>> = BTreeMap::new();
    let mut traffic = false;
    if sandbox.status == SandboxStatus::Running {
        sessions = match agent_activity_service::sessions(ctx, &sandbox.name).await {
            Ok(found) => found,
            Err(error) => {
                tracing::debug!(
                    sandbox = %sandbox.name,
                    code = error.code.0,
                    "agent sessions unavailable"
                );
                Vec::new()
            }
        };
        accounts = match session_service::agent_accounts(ctx, &sandbox.name).await {
            Ok(found) => found,
            Err(error) => {
                tracing::debug!(
                    sandbox = %sandbox.name,
                    code = error.code.0,
                    "agent accounts unavailable"
                );
                BTreeMap::new()
            }
        };
        traffic = recent_traffic(ctx, sandbox.environment_id, sandbox_id).await;
        supervised = match supervised_agents(ctx, sandbox.environment_id, &sandbox).await {
            Ok(found) => found,
            Err(error) => {
                tracing::warn!(
                    sandbox = %sandbox.name,
                    code = error.code.0,
                    "supervised agents unknown"
                );
                Vec::new()
            }
        };
    }

    let mut out = Vec::with_capacity(modes.len());
    for (agent, mode) in modes {
        let own: Vec<AgentSession> = sessions
            .iter()
            .filter(|session| session.agent == agent)
            .cloned()
            .collect();
        let runs_as_server = supervised.iter().any(|process| process.agent == agent);
        let pending_restart = rows
            .iter()
            .find(|row| row.agent == agent)
            .map(|row| row.pending_restart)
            .unwrap_or(false);
        let login = accounts.get(agent.as_str());
        out.push(AgentRuntimeState {
            agent,
            mode,
            pending_restart,
            busy: own.iter().any(|session| session.busy) || (runs_as_server && traffic),
            sessions: own,
            signed_in: login.map(|account| account.is_some()),
            account: login.cloned().flatten(),
        });
    }
    Ok(out)
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
    require(principal, Permission::SandboxWrite)?;
    let sandbox = load_sandbox(ctx, principal, sandbox_id).await?;
    check_agent_switched(ctx, &sandbox, agent).await?;

    // The row records the choice. A `later` switch is stored as waiting right
    // away; `idle` and `now` are stored as not waiting, and `apply_restart` below
    // marks the agent as waiting only when something could not be restarted yet.
    {
        let mut scope = TenantScope::begin_for(&ctx.pool, principal)
            .await
            .map_err(ApiError::from)?;
        auth_store::upsert(
            &mut scope,
            sandbox_id,
            agent,
            mode,
            restart == RestartPolicy::Later,
            Some(principal.user_id),
        )
        .await
        .map_err(ApiError::from)?;
        crate::store::postgres::audit::record(
            &mut scope,
            crate::store::postgres::audit::NewAuditEvent {
                actor_id: Some(principal.user_id),
                actor_label: &principal.display_name,
                kind: "sandbox.agent_auth",
                subject_type: Some("sandbox"),
                subject_id: Some(sandbox_id),
                detail: &serde_json::json!({
                    "agent": agent.as_str(),
                    "mode": mode.as_str(),
                    "restart": restart.as_str(),
                }),
                request_id: None,
            },
        )
        .await
        .map_err(ApiError::from)?;
        scope.commit().await.map_err(ApiError::from)?;
    }

    if sandbox.status == SandboxStatus::Running {
        // The launcher's mode file is the part that must work: without it the
        // next process would start in the old mode while the console shows the
        // new one.
        agent_launch_service::write_mode(ctx, &sandbox.name, agent, mode).await?;
        if let Err(error) =
            sandbox_service::write_agent_files(ctx, sandbox.environment_id, sandbox_id).await
        {
            tracing::warn!(
                sandbox = %sandbox.name,
                code = error.code.0,
                "agent config files not rewritten"
            );
        }
    }

    sync_login_rules(ctx, sandbox.environment_id, sandbox_id).await?;
    push_openshell_policies(ctx);
    if sandbox.status == SandboxStatus::Running {
        apply_restart(ctx, &sandbox, agent, restart).await?;
    }

    ctx.bus
        .publish(crate::domain::telemetry::LiveEvent::SandboxControlChanged {
            sandbox_id: Some(sandbox_id),
            environment_id: sandbox.environment_id,
            what: "agent_auth".to_string(),
            detail: serde_json::json!({
                "agent": agent.as_str(),
                "mode": mode.as_str(),
                "restart": restart.as_str(),
            }),
        });
    list(ctx, principal, sandbox_id).await
}

/// Restart one agent without changing its mode.
pub async fn restart(
    ctx: &ServiceContext,
    principal: &Principal,
    sandbox_id: Uuid,
    agent: AgentKind,
    when: RestartPolicy,
) -> Result<Vec<AgentRuntimeState>, ApiError> {
    require(principal, Permission::SandboxWrite)?;
    let sandbox = load_sandbox(ctx, principal, sandbox_id).await?;
    check_agent_switched(ctx, &sandbox, agent).await?;

    {
        let mut scope = TenantScope::begin_for(&ctx.pool, principal)
            .await
            .map_err(ApiError::from)?;
        crate::store::postgres::audit::record(
            &mut scope,
            crate::store::postgres::audit::NewAuditEvent {
                actor_id: Some(principal.user_id),
                actor_label: &principal.display_name,
                kind: "sandbox.agent_restart",
                subject_type: Some("sandbox"),
                subject_id: Some(sandbox_id),
                detail: &serde_json::json!({
                    "agent": agent.as_str(),
                    "restart": when.as_str(),
                }),
                request_id: None,
            },
        )
        .await
        .map_err(ApiError::from)?;
        scope.commit().await.map_err(ApiError::from)?;
    }

    if when == RestartPolicy::Later {
        // "Only for new sessions": nothing restarts, the agent is marked waiting.
        set_pending(ctx, sandbox.environment_id, sandbox_id, agent, true).await?;
    } else if sandbox.status == SandboxStatus::Running {
        apply_restart(ctx, &sandbox, agent, when).await?;
    }

    ctx.bus
        .publish(crate::domain::telemetry::LiveEvent::SandboxControlChanged {
            sandbox_id: Some(sandbox_id),
            environment_id: sandbox.environment_id,
            what: "restart".to_string(),
            detail: serde_json::json!({
                "agent": agent.as_str(),
                "restart": when.as_str(),
            }),
        });
    list(ctx, principal, sandbox_id).await
}

/// Keep the sign-in firewall rules in step with the agents' modes.
///
/// The rules are replaced wholesale instead of diffed: the set is small (at most
/// a few dozen rows) and a delete-then-insert inside one transaction cannot leave
/// half a host list behind, which a diff could.
pub async fn sync_login_rules(
    ctx: &ServiceContext,
    environment_id: Uuid,
    sandbox_id: Uuid,
) -> Result<(), ApiError> {
    let modes = modes_for(ctx, environment_id, sandbox_id).await?;
    let mut scope = TenantScope::begin(&ctx.pool, environment_id)
        .await
        .map_err(ApiError::from)?;
    login_rules::delete_for_sandbox(&mut scope, sandbox_id)
        .await
        .map_err(ApiError::from)?;
    for (agent, mode) in modes {
        if mode != AgentAuthMode::Subscription {
            continue;
        }
        for host in login_hosts(agent) {
            login_rules::insert(
                &mut scope,
                sandbox_id,
                &format!("Sign-in ({}): {host}", agent.as_str()),
                host,
            )
            .await
            .map_err(ApiError::from)?;
        }
    }
    scope.commit().await.map_err(ApiError::from)?;
    Ok(())
}

/// Restart agents that waited for a busy session to become idle.
///
/// One pass over every environment, because the loop has no request and therefore
/// no tenant. A failure for one sandbox is logged inside and does not stop the
/// pass: the flag stays set, so the next pass tries again.
pub async fn run_pending_restarts(ctx: &ServiceContext) -> Result<(), ApiError> {
    let environments = login_rules::environment_ids(&ctx.pool)
        .await
        .map_err(ApiError::from)?;
    for environment_id in environments {
        let waiting = {
            let mut scope = TenantScope::begin(&ctx.pool, environment_id)
                .await
                .map_err(ApiError::from)?;
            let rows = auth_store::pending(&mut scope)
                .await
                .map_err(ApiError::from)?;
            scope.commit().await.map_err(ApiError::from)?;
            rows
        };
        for (sandbox_id, agent) in waiting {
            if let Err(error) = finish_restart(ctx, environment_id, sandbox_id, agent).await {
                tracing::warn!(
                    code = error.code.0,
                    %sandbox_id,
                    agent = agent.as_str(),
                    "waiting agent restart not finished"
                );
            }
        }
    }
    Ok(())
}

/// A process of the sandbox that is restarted by name, not by tmux pane.
///
/// OpenChamber and every recipe agent run as a supervised server: they have no
/// pane of their own, so they are addressed by the name their supervisor knows
/// them under.
#[derive(Debug, Clone, PartialEq, Eq)]
struct SupervisedAgent {
    /// The name the supervisor uses, `openchamber` or an agent's instance.
    name: String,
    /// The agent the process is.
    agent: AgentKind,
}

/// Panes already respawned while an agent waited for its busy sibling.
///
/// The loop runs every 5 s until the flag clears. Without this record, the idle
/// panes next to a busy one would be respawned on every pass, and an agent
/// respawned every 5 s never gets to run. The map is a cache: a restart of this
/// process costs one extra respawn, nothing more.
fn respawned_while_waiting() -> &'static Mutex<HashMap<(Uuid, AgentKind), HashSet<i64>>> {
    static RESPAWNED: OnceLock<Mutex<HashMap<(Uuid, AgentKind), HashSet<i64>>>> = OnceLock::new();
    RESPAWNED.get_or_init(|| Mutex::new(HashMap::new()))
}

/// The sandbox of the caller's environment, read under row-level security.
async fn load_sandbox(
    ctx: &ServiceContext,
    principal: &Principal,
    sandbox_id: Uuid,
) -> Result<Sandbox, ApiError> {
    let mut scope = TenantScope::begin_for(&ctx.pool, principal)
        .await
        .map_err(ApiError::from)?;
    let sandbox = sandbox_store::get(&mut scope, sandbox_id)
        .await
        .map_err(ApiError::from)?;
    scope.commit().await.map_err(ApiError::from)?;
    Ok(sandbox)
}

/// Refuse an agent the sandbox does not have.
///
/// A pack decides which agents it can switch; asking for another one is a
/// mistake in the request, not a permission problem, so it is a 422 with the
/// agent's name in the message.
async fn check_agent_switched(
    ctx: &ServiceContext,
    sandbox: &Sandbox,
    agent: AgentKind,
) -> Result<(), ApiError> {
    let modes = modes_for(ctx, sandbox.environment_id, sandbox.id).await?;
    if modes.iter().any(|(kind, _)| *kind == agent) {
        Ok(())
    } else {
        Err(ApiError::validation(format!(
            "This sandbox has no {}.",
            agent.as_str()
        )))
    }
}

/// The supervised processes of a sandbox, with the agent each one runs.
///
/// The pack's own server is OpenChamber; a recipe sandbox names its agents by
/// instance, and the instance is usually the agent's own name. When it is not,
/// the pack's first login agent decides, because a pack that must pick one of
/// several agents has that order for exactly this purpose.
async fn supervised_agents(
    ctx: &ServiceContext,
    environment_id: Uuid,
    sandbox: &Sandbox,
) -> Result<Vec<SupervisedAgent>, ApiError> {
    let mut scope = TenantScope::begin(&ctx.pool, environment_id)
        .await
        .map_err(ApiError::from)?;
    let mut out = Vec::new();
    if sandbox.pack_key == "openchamber" {
        out.push(SupervisedAgent {
            name: "openchamber".to_string(),
            agent: AgentKind::Opencode,
        });
    }
    let agents = persistent_roots::agents_for_sandbox(&mut scope, sandbox.id)
        .await
        .map_err(ApiError::from)?;
    for entry in agents {
        let agent = match AgentKind::parse(&entry.instance) {
            Some(agent) => agent,
            None => match auth_store::pack_agents(&mut scope, &entry.pack_key).await {
                Ok(agents) => match agents.first() {
                    Some(agent) => *agent,
                    None => continue,
                },
                Err(error) => {
                    tracing::debug!(
                        sandbox = %sandbox.name,
                        pack = %entry.pack_key,
                        error = %error,
                        "agent of a recipe pack unknown"
                    );
                    continue;
                }
            },
        };
        if !out.iter().any(|process| process.name == entry.instance) {
            out.push(SupervisedAgent {
                name: entry.instance,
                agent,
            });
        }
    }
    scope.commit().await.map_err(ApiError::from)?;
    Ok(out)
}

/// The sessions of one agent in a sandbox; empty when the runtime cannot answer.
///
/// An unreadable pane list is not a reason to fail a mode change: the console
/// then shows no sessions, which is the truth it knows.
async fn agent_sessions(
    ctx: &ServiceContext,
    sandbox_name: &str,
    agent: AgentKind,
) -> Vec<AgentSession> {
    match agent_activity_service::sessions(ctx, sandbox_name).await {
        Ok(found) => found
            .into_iter()
            .filter(|session| session.agent == agent)
            .collect(),
        Err(error) => {
            tracing::debug!(
                sandbox = sandbox_name,
                code = error.code.0,
                "agent sessions unavailable"
            );
            Vec::new()
        }
    }
}

/// The supervised processes of one agent; empty when they cannot be read.
async fn agent_servers(
    ctx: &ServiceContext,
    sandbox: &Sandbox,
    agent: AgentKind,
) -> Vec<SupervisedAgent> {
    match supervised_agents(ctx, sandbox.environment_id, sandbox).await {
        Ok(found) => found
            .into_iter()
            .filter(|process| process.agent == agent)
            .collect(),
        Err(error) => {
            tracing::warn!(
                sandbox = %sandbox.name,
                code = error.code.0,
                "supervised agents unknown"
            );
            Vec::new()
        }
    }
}

/// Restart an agent's sessions according to the policy.
///
/// Every restart is best-effort on purpose: the mode file is already written, so
/// the agent is in its new mode from the next process on, and reporting a failed
/// respawn as a failed mode change would be a lie. A restart that did not happen
/// leaves the pending flag set, which is what makes the loop retry it.
async fn apply_restart(
    ctx: &ServiceContext,
    sandbox: &Sandbox,
    agent: AgentKind,
    policy: RestartPolicy,
) -> Result<(), ApiError> {
    if policy == RestartPolicy::Later {
        set_pending(ctx, sandbox.environment_id, sandbox.id, agent, true).await?;
        return Ok(());
    }
    let servers = agent_servers(ctx, sandbox, agent).await;
    let sessions = agent_sessions(ctx, &sandbox.name, agent).await;
    let busy_pane = sessions.iter().any(|session| session.busy);
    let busy_server =
        !servers.is_empty() && recent_traffic(ctx, sandbox.environment_id, sandbox.id).await;

    let mut complete = true;
    let restart_servers = match policy {
        RestartPolicy::Now => true,
        RestartPolicy::Idle => !busy_pane && !busy_server,
        RestartPolicy::Later => false,
    };
    for session in &sessions {
        // While waiting, the busy pane is the one that must not be touched.
        if policy == RestartPolicy::Idle && session.busy {
            continue;
        }
        if !restart_pane(ctx, &sandbox.name, session).await {
            complete = false;
        }
    }
    if restart_servers {
        for process in &servers {
            if !restart_process(ctx, &sandbox.name, &process.name).await {
                complete = false;
            }
        }
    }

    let waiting = !complete || (policy == RestartPolicy::Idle && (busy_pane || busy_server));
    set_pending(ctx, sandbox.environment_id, sandbox.id, agent, waiting).await?;
    if !waiting {
        forget_respawned(sandbox.id, agent);
    }
    Ok(())
}

/// Finish one waiting restart: respawn what is idle, clear the flag when nothing
/// of that agent is busy any more.
async fn finish_restart(
    ctx: &ServiceContext,
    environment_id: Uuid,
    sandbox_id: Uuid,
    agent: AgentKind,
) -> Result<(), ApiError> {
    let sandbox = {
        let mut scope = TenantScope::begin(&ctx.pool, environment_id)
            .await
            .map_err(ApiError::from)?;
        let sandbox = sandbox_store::get(&mut scope, sandbox_id)
            .await
            .map_err(ApiError::from)?;
        scope.commit().await.map_err(ApiError::from)?;
        sandbox
    };
    if sandbox.status != SandboxStatus::Running {
        // Nothing can be restarted in a stopped sandbox, and keeping the flag
        // would restart it the moment it comes up, which nobody asked for.
        forget_respawned(sandbox_id, agent);
        return set_pending(ctx, environment_id, sandbox_id, agent, false).await;
    }

    let sessions = agent_sessions(ctx, &sandbox.name, agent).await;
    let busy_pane = sessions.iter().any(|session| session.busy);
    let servers = agent_servers(ctx, &sandbox, agent).await;
    let busy_server =
        !servers.is_empty() && recent_traffic(ctx, environment_id, sandbox_id).await;

    let mut respawned = respawned_of(sandbox_id, agent);
    for session in sessions.iter().filter(|session| !session.busy) {
        if respawned.contains(&session.pid) {
            continue;
        }
        if restart_pane(ctx, &sandbox.name, session).await {
            respawned.insert(session.pid);
        }
    }
    remember_respawned(sandbox_id, agent, respawned);

    if busy_pane || busy_server {
        return Ok(());
    }
    let mut complete = true;
    for process in &servers {
        if !restart_process(ctx, &sandbox.name, &process.name).await {
            complete = false;
        }
    }
    if !complete {
        return Ok(());
    }
    forget_respawned(sandbox_id, agent);
    set_pending(ctx, environment_id, sandbox_id, agent, false).await?;
    ctx.bus
        .publish(crate::domain::telemetry::LiveEvent::SandboxControlChanged {
            sandbox_id: Some(sandbox_id),
            environment_id,
            what: "restart".to_string(),
            detail: serde_json::json!({ "agent": agent.as_str() }),
        });
    Ok(())
}

/// The panes of one waiting agent that were already respawned.
fn respawned_of(sandbox_id: Uuid, agent: AgentKind) -> HashSet<i64> {
    match respawned_while_waiting().lock() {
        Ok(guard) => guard
            .get(&(sandbox_id, agent))
            .cloned()
            .unwrap_or_default(),
        Err(poisoned) => poisoned
            .into_inner()
            .get(&(sandbox_id, agent))
            .cloned()
            .unwrap_or_default(),
    }
}

/// Record the panes of one waiting agent that were respawned.
fn remember_respawned(sandbox_id: Uuid, agent: AgentKind, panes: HashSet<i64>) {
    match respawned_while_waiting().lock() {
        Ok(mut guard) => {
            guard.insert((sandbox_id, agent), panes);
        }
        Err(poisoned) => {
            poisoned.into_inner().insert((sandbox_id, agent), panes);
        }
    }
}

/// Forget the record of one waiting agent, because its wait is over.
fn forget_respawned(sandbox_id: Uuid, agent: AgentKind) {
    match respawned_while_waiting().lock() {
        Ok(mut guard) => {
            guard.remove(&(sandbox_id, agent));
        }
        Err(poisoned) => {
            poisoned.into_inner().remove(&(sandbox_id, agent));
        }
    }
}

/// Restart one tmux pane; `false` and a log line when it did not happen.
async fn restart_pane(ctx: &ServiceContext, sandbox_name: &str, session: &AgentSession) -> bool {
    match agent_activity_service::restart_session(ctx, sandbox_name, session).await {
        Ok(()) => true,
        Err(error) => {
            tracing::warn!(
                sandbox = sandbox_name,
                pane = %session.pane_id,
                code = error.code.0,
                "agent pane not restarted"
            );
            false
        }
    }
}

/// Restart one supervised agent; `false` and a log line when it did not happen.
async fn restart_process(ctx: &ServiceContext, sandbox_name: &str, name: &str) -> bool {
    match agent_activity_service::restart_supervised(ctx, sandbox_name, name).await {
        Ok(()) => true,
        Err(error) => {
            tracing::warn!(
                sandbox = sandbox_name,
                agent = name,
                code = error.code.0,
                "supervised agent not restarted"
            );
            false
        }
    }
}

/// Whether a server agent of this sandbox just talked to a model.
async fn recent_traffic(ctx: &ServiceContext, environment_id: Uuid, sandbox_id: Uuid) -> bool {
    match agent_activity_service::recent_model_traffic(ctx, environment_id, sandbox_id).await {
        Ok(value) => value,
        Err(error) => {
            tracing::debug!(code = error.code.0, "model traffic unknown");
            false
        }
    }
}

/// Write or clear one agent's pending-restart flag.
async fn set_pending(
    ctx: &ServiceContext,
    environment_id: Uuid,
    sandbox_id: Uuid,
    agent: AgentKind,
    pending: bool,
) -> Result<(), ApiError> {
    let mut scope = TenantScope::begin(&ctx.pool, environment_id)
        .await
        .map_err(ApiError::from)?;
    let updated = auth_store::set_pending(&mut scope, sandbox_id, agent, pending)
        .await
        .map_err(ApiError::from)?;
    if updated == 0 {
        // No row yet: the sandbox still has the create-time default, which this
        // change does not alter, so the row is written with that mode.
        let mode = modes_for(ctx, environment_id, sandbox_id)
            .await?
            .into_iter()
            .find(|(kind, _)| *kind == agent)
            .map(|(_, mode)| mode)
            .unwrap_or(AgentAuthMode::Gateway);
        auth_store::upsert(&mut scope, sandbox_id, agent, mode, pending, None)
            .await
            .map_err(ApiError::from)?;
    }
    scope.commit().await.map_err(ApiError::from)?;
    Ok(())
}

/// Push the OpenShell policies in the background.
///
/// The push talks to every running gateway, so it must not be on the path of the
/// answer to the operator; a failure is logged because the reconcile loop sends
/// the same document again on its next pass.
fn push_openshell_policies(ctx: &ServiceContext) {
    let ctx = ctx.clone();
    tokio::spawn(async move {
        if let Err(error) = policy_service::sync_openshell_policies(&ctx).await {
            tracing::warn!(
                code = error.code.0,
                "OpenShell policies not pushed after an agent change"
            );
        }
    });
}
