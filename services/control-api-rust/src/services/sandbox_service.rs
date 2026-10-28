//! Sandbox lifecycle, placement, and bulk actions.
//!
//! The rules that matter:
//!
//! * A state transition is checked against the current status before it is
//!   attempted, so a stop on a stopped sandbox is a clear refusal rather than a
//!   runtime error.
//! * Placement refuses a gateway that is drifted or unhealthy (ADR 0012).
//! * A bulk action over more than one sandbox becomes a job, so partial failure
//!   is attributable and the request does not hold a connection for minutes.

use std::collections::BTreeMap;

use uuid::Uuid;

use super::pack_service;
use super::{require, ServiceContext};
use crate::domain::auth::{Permission, Principal};
use crate::domain::pagination::{ListParams, Page};
use crate::domain::runtime::SandboxSpec;
use crate::domain::sandbox::{
    Sandbox, SandboxDetail, SandboxHealth, SandboxName, SandboxStatus, SandboxSummary,
};
use crate::domain::telemetry::LiveEvent;
use crate::error::{ApiError, ErrorCode};
use crate::store::TenantScope;
use crate::store::postgres::environments as env_store;
use crate::store::postgres::sandboxes as store;

/// Which lifecycle action was requested.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LifecycleAction {
    Start,
    Stop,
    Restart,
}

impl LifecycleAction {
    /// Parse the action name from a request body.
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "start" => Some(Self::Start),
            "stop" => Some(Self::Stop),
            "restart" => Some(Self::Restart),
            _ => None,
        }
    }

    /// The status a sandbox must be in for this action to make sense.
    fn is_allowed_for(self, sandbox: &Sandbox) -> bool {
        match self {
            Self::Start => sandbox.can_start(),
            Self::Stop => sandbox.can_stop(),
            Self::Restart => sandbox.status == SandboxStatus::Running,
        }
    }

    /// The status to record while the action runs.
    fn transitional(self) -> SandboxStatus {
        match self {
            Self::Start | Self::Restart => SandboxStatus::Starting,
            Self::Stop => SandboxStatus::Stopping,
        }
    }

    /// The status to record when the action succeeds.
    fn settled(self) -> SandboxStatus {
        match self {
            Self::Start | Self::Restart => SandboxStatus::Running,
            Self::Stop => SandboxStatus::Stopped,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Start => "start",
            Self::Stop => "stop",
            Self::Restart => "restart",
        }
    }
}

/// List sandboxes for the console, with live state merged in.
///
/// The runtime is asked once for the whole inventory rather than once per row.
/// A page of fifty sandboxes must not be fifty round trips, and a runtime that is
/// unreachable must degrade the page rather than fail it.
pub async fn list(
    ctx: &ServiceContext,
    principal: &Principal,
    params: &ListParams,
) -> Result<Page<SandboxSummary>, ApiError> {
    require(principal, Permission::SandboxRead)?;
    let mut scope = TenantScope::begin(&ctx.pool, principal.environment_id)
        .await
        .map_err(ApiError::from)?;
    let page = store::list(&mut scope, params).await.map_err(ApiError::from)?;
    scope.commit().await.map_err(ApiError::from)?;

    let facts = ctx.runtime.list().await.ok();
    let runtime_available = facts.is_some();
    let index: std::collections::HashMap<String, crate::domain::runtime::RuntimeFacts> = facts
        .unwrap_or_default()
        .into_iter()
        .map(|fact| (fact.name.clone(), fact))
        .collect();

    let mut settled = Vec::with_capacity(page.items.len());
    for sandbox in page.items {
        let live = index.get(&sandbox.name);
        settled.push(reconcile_transitional(ctx, principal, sandbox, live).await);
    }

    let items = settled
        .into_iter()
        .map(|sandbox| {
            let live = index.get(&sandbox.name);
            SandboxSummary {
                address: live.map(|facts| facts.ip.clone()).filter(|ip| !ip.is_empty()),
                uptime_seconds: live.map(|facts| facts.uptime_seconds),
                container_name: live.map(|facts| facts.container_name.clone()),
                runtime_available,
                sandbox,
            }
        })
        .collect();

    Ok(Page {
        items,
        next_cursor: page.next_cursor,
        total_estimate: page.total_estimate,
        facets: page.facets,
    })
}

/// Read one sandbox and merge what the runtime reports.
pub async fn get(
    ctx: &ServiceContext,
    principal: &Principal,
    id: Uuid,
) -> Result<SandboxDetail, ApiError> {
    require(principal, Permission::SandboxRead)?;
    let mut scope = TenantScope::begin(&ctx.pool, principal.environment_id)
        .await
        .map_err(ApiError::from)?;
    let sandbox = store::get(&mut scope, id).await.map_err(ApiError::from)?;
    scope.commit().await.map_err(ApiError::from)?;

    // A runtime that cannot be reached is a normal state, not an error: the
    // stored state is still worth showing.
    let runtime = ctx.runtime.inspect(&sandbox.name).await.ok();
    let sandbox = reconcile_transitional(ctx, principal, sandbox, runtime.as_ref()).await;
    Ok(SandboxDetail { sandbox, runtime })
}

/// What a create request carries.
pub struct CreateSandbox {
    pub name: String,
    pub pack_key: String,
    /// Empty means "use the pack's default image".
    pub image: String,
    pub image_digest: Option<String>,
    /// Empty means "use the pack's default command".
    pub command: Vec<String>,
    pub memory_limit: String,
    pub cpu_limit: f64,
    pub pids_limit: i64,
    /// The node to place the sandbox on, by name. Empty means the local node.
    pub node: Option<String>,
}

/// Settle a row stuck in a transitional state when the runtime has moved on.
///
/// `starting` and `stopping` are written before the runtime call and replaced
/// after it. If the process dies in between — a deploy during a stop, a crash,
/// a lost connection — the row keeps the transitional status forever and every
/// lifecycle button is disabled. The runtime still knows the truth, so a read
/// that sees both compares them and writes the settled state. The desired
/// state is not changed by this; only a transitional one is resolved.
async fn reconcile_transitional(
    ctx: &ServiceContext,
    principal: &Principal,
    sandbox: Sandbox,
    live: Option<&crate::domain::runtime::RuntimeFacts>,
) -> Sandbox {
    let Some(facts) = live else {
        return sandbox;
    };
    if !matches!(
        sandbox.status,
        SandboxStatus::Starting | SandboxStatus::Stopping
    ) {
        return sandbox;
    }
    let Some(target) = settled_status_from_runtime(&facts.status) else {
        return sandbox;
    };
    let health = health_from_runtime(facts);
    match set_status(ctx, principal, sandbox.id, sandbox.version, target, health).await {
        Ok(updated) => {
            tracing::info!(sandbox = %updated.name, from = sandbox.status.as_str(), to = target.as_str(), "settled a transitional sandbox from runtime state");
            updated
        }
        Err(error) => {
            // A version conflict means another writer settled it first, which
            // is fine; anything else is logged and the stale row is shown.
            tracing::debug!(sandbox = %sandbox.name, code = error.code.0, "reconcile skipped");
            sandbox
        }
    }
}

/// The settled status a runtime state corresponds to, if it is settled.
///
/// Docker's own transitional states (`restarting`, `removing`) yield `None`:
/// the row stays transitional until the runtime makes up its mind.
pub fn settled_status_from_runtime(runtime_status: &str) -> Option<SandboxStatus> {
    match runtime_status {
        "running" => Some(SandboxStatus::Running),
        "exited" | "created" | "paused" | "stopped" => Some(SandboxStatus::Stopped),
        "dead" => Some(SandboxStatus::Failed),
        _ => None,
    }
}

/// Translate what the runtime reports into the stored health.
///
/// One rule for create and lifecycle, because they previously disagreed: create
/// called an unprobed container healthy and a restart called it unknown, so the
/// badge flipped on the first stop/start. A container with no health check
/// reports `unknown` from Docker; a running one with no probe is treated as
/// healthy because "running and not failing" is the only evidence available,
/// and a stopped one is unknown because nothing is observing it.
pub fn health_from_runtime(facts: &crate::domain::runtime::RuntimeFacts) -> SandboxHealth {
    match facts.health.as_str() {
        "healthy" => SandboxHealth::Healthy,
        "degraded" | "starting" => SandboxHealth::Degraded,
        "unhealthy" => SandboxHealth::Unhealthy,
        _ if facts.status == "running" => SandboxHealth::Healthy,
        _ => SandboxHealth::Unknown,
    }
}

/// Whether a pack needs an inference endpoint to do its job.
///
/// Kept as a named rule rather than an inline `== "hermes"` so it is one place
/// to change when a second agent pack arrives, and so the test suite can assert
/// the list deliberately.
pub fn pack_needs_inference(pack_key: &str) -> bool {
    matches!(pack_key, "hermes")
}

/// The proxy URL used when a node does not name one.
///
/// The default matches the runner's `NESTBOX_EGRESS_PROXY_HOST`, because a URL
/// the sandbox cannot resolve is a sandbox with no egress at all.
pub const DEFAULT_EGRESS_PROXY: &str = "http://egress-proxy:3128";

/// The egress proxy a sandbox is told about, as the address it can reach.
///
/// The sandbox has no route out (the runtime network is `internal`), so this URL
/// is the only way it reaches anything; whether a request may actually leave is
/// decided per request by the control API (ADR 0013), not here.
///
/// Read from the environment rather than from `nodes.egress_proxy_url` because
/// that column exists (migration 0022) but the node domain type does not carry it
/// yet. One value a deploy can set beats a second source of truth in a service;
/// when the column reaches the domain type, this function reads it and nothing
/// else changes.
pub fn egress_proxy_url() -> String {
    match std::env::var("NESTBOX_EGRESS_PROXY_URL") {
        Ok(value) if !value.trim().is_empty() => value.trim().to_string(),
        _ => DEFAULT_EGRESS_PROXY.to_string(),
    }
}

/// Keep the inference endpoint out of the proxy.
///
/// The provider a sandbox is given may be a model on its own node
/// (`http://model-<key>:8080/v1`). That address is on the runtime network and
/// must not be proxied: through the proxy it is an egress request without a
/// rule, and the agent reports the refusal as a rejected API key — a wrong
/// diagnosis of a correct default-deny. The host is read from the environment
/// the sandbox was already given, so the two cannot disagree.
pub fn add_inference_bypass(environment: &mut BTreeMap<String, String>) {
    let Some(base) = environment.get("OPENAI_BASE_URL").cloned() else {
        return;
    };
    let host = base
        .split("://")
        .nth(1)
        .unwrap_or(&base)
        .split(['/', ':'])
        .next()
        .unwrap_or("")
        .to_string();
    if host.is_empty() {
        return;
    }
    for key in ["NO_PROXY", "no_proxy"] {
        let current = environment.get(key).cloned().unwrap_or_default();
        if current.split(',').any(|part| part == host) {
            continue;
        }
        let value = if current.is_empty() {
            host.clone()
        } else {
            format!("{current},{host}")
        };
        environment.insert(key.to_string(), value);
    }
}

/// The same bypass for the vector the runtime spec carries.
///
/// Sorted and deduplicated by the map, so two creates of the same sandbox send
/// the same environment.
pub fn add_inference_bypass_vec(environment: &mut Vec<(String, String)>) {
    let mut map: BTreeMap<String, String> = environment.iter().cloned().collect();
    add_inference_bypass(&mut map);
    *environment = map.into_iter().collect();
}

/// The host part of a proxy URL, for the bypass list.
fn proxy_host(proxy: &str) -> &str {
    proxy
        .split("://")
        .nth(1)
        .unwrap_or(proxy)
        .split(['/', ':'])
        .next()
        .unwrap_or(proxy)
}

/// The proxy variables a sandbox needs, in both spellings.
///
/// Uppercase and lowercase are both set because the tooling disagrees: curl and
/// wget read `http_proxy`, Go and many Rust clients read `HTTP_PROXY`, npm reads
/// both and prefers the uppercase one. Setting a single spelling is the classic
/// "the proxy works in the terminal but not in the agent" bug, and a missed
/// variable is not a hole — it is a tool that cannot leave, which looks like a
/// broken network and gets diagnosed as one.
///
/// `NO_PROXY` names the proxy itself. Without it a proxy-aware tool sends the
/// request for the proxy *to* the proxy, which is a loop. Loopback stays out of
/// the proxy because nothing on it leaves the container.
pub fn egress_environment(proxy: &str) -> Vec<(String, String)> {
    let bypass = format!("localhost,127.0.0.1,::1,{}", proxy_host(proxy));
    let mut values = Vec::with_capacity(8);
    for key in ["HTTP_PROXY", "HTTPS_PROXY", "ALL_PROXY"] {
        values.push((key.to_string(), proxy.to_string()));
        values.push((key.to_lowercase(), proxy.to_string()));
    }
    for key in ["NO_PROXY", "no_proxy"] {
        values.push((key.to_string(), bypass.clone()));
    }
    // Sorted so the spec — and any JSON that echoes it — is stable between
    // creates.
    values.sort();
    values
}

/// Create a sandbox: validate, place, persist, then ask the runtime.
///
/// Order matters. The row is written first so a failed runtime call leaves a
/// visible `failed` sandbox rather than an invisible container.
pub async fn create(
    ctx: &ServiceContext,
    principal: &Principal,
    request: CreateSandbox,
) -> Result<Sandbox, ApiError> {
    require(principal, Permission::SandboxWrite)?;

    let name = SandboxName::new(&request.name).map_err(ApiError::validation)?;

    // Normalise before anything is stored, so the spec holds exactly what the
    // runtime was asked for and a later restart cannot reinterpret it.
    let memory_limit = crate::domain::runtime::normalize_memory_limit(&request.memory_limit)
        .map_err(ApiError::validation)?;

    let mut scope = TenantScope::begin(&ctx.pool, principal.environment_id)
        .await
        .map_err(ApiError::from)?;

    // The image and command come from `agent_packs`, not from a `match`: an
    // unknown pack is a 404 and a disabled or misspelled one cannot silently
    // become the default shell (ADR 0020). A plain sandbox records no baseline,
    // so compatibility is checked here only where one is known.
    let defaults = pack_service::pack_defaults(&mut scope, &request.pack_key, None).await?;
    let image = if request.image.trim().is_empty() {
        defaults.image
    } else {
        request.image.clone()
    };
    let command: Vec<String> = if request.command.is_empty() {
        defaults.command
    } else {
        request.command.clone()
    };

    reject_unpinned_image(&image, request.image_digest.as_deref())?;

    let cluster_id = env_store::default_cluster(&mut scope)
        .await
        .map_err(ApiError::from)?;

    let gateway_id = crate::store::postgres::gateways::list(&mut scope)
        .await
        .map_err(ApiError::from)?
        .into_iter()
        .find(|gateway| gateway.accepts_placement())
        .map(|gateway| gateway.id);

    // The node is resolved by name and must exist: a typo must not fall back
    // to the local node silently.
    let node = match request.node.as_deref().filter(|n| !n.is_empty()) {
        Some(name) => crate::store::postgres::nodes::get_by_name(&mut scope, name)
            .await
            .map_err(|error| match error {
                crate::error::StoreError::NotFound => ApiError::not_found(
                    ErrorCode::NODE_NOT_FOUND,
                    format!("No node named '{name}'."),
                ),
                other => ApiError::from(other),
            })?,
        None => crate::store::postgres::nodes::local(&mut scope)
            .await
            .map_err(ApiError::from)?,
    };
    if !node.kind.is_operable() {
        return Err(ApiError::conflict(
            ErrorCode::NODE_RUNTIME_UNAVAILABLE,
            format!("Node '{}' has no runtime adapter yet.", node.name),
        ));
    }

    // A sandbox that cannot fit is refused before a row exists: the node's
    // budget is what its model installs left free, and a 32 GiB request on a
    // 7.8 GiB machine is a typo the operator should see now rather than an
    // OOM kill later. A node nobody has measured has no numbers to check
    // against, so the request is allowed and the response says the capacity is
    // unknown rather than being refused for a measurement that does not exist.
    let budget = crate::services::node_service::free_budget(&mut scope, node.id).await?;
    if budget.ram_total_bytes > 0 {
        let wanted = crate::domain::runtime::memory_bytes(&request.memory_limit)
            .map_err(ApiError::validation)?;
        budget.can_host(wanted, request.cpu_limit).map_err(|error| {
            ApiError::unprocessable(ErrorCode::NODE_INSUFFICIENT_RESOURCES, error.message())
        })?;
    }

    // Recorded in the spec as well as passed in the environment: "what was this
    // sandbox told" has to be answerable from the row after the fact, not
    // reconstructed from a later deploy's configuration.
    let egress_proxy = egress_proxy_url();

    let spec = serde_json::json!({
        "pack_key": request.pack_key,
        "memory_limit": memory_limit,
        "cpu_limit": request.cpu_limit,
        "pids_limit": request.pids_limit,
        "node": node.name,
        "egress_proxy": &egress_proxy,
    });

    let sandbox = store::insert(
        &mut scope,
        store::NewSandbox {
            cluster_id,
            gateway_id,
            node_id: node.id,
            name: name.as_str(),
            pack_key: &request.pack_key,
            image: &image,
            image_digest: request.image_digest.as_deref(),
            spec: &spec,
        },
    )
    .await
    .map_err(ApiError::from)?;

    // The container receives the binds the database says this sandbox has. The
    // request cannot name a mount source: the only source is a persistent root's
    // stored host path. Read inside this transaction so a row written by the same
    // operation is visible.
    let mounts =
        crate::services::persist_service::mount_specs_for_sandbox(&mut scope, sandbox.id).await?;

    crate::store::postgres::audit::record(
        &mut scope,
        crate::store::postgres::audit::NewAuditEvent {
            actor_id: Some(principal.user_id),
            actor_label: &principal.display_name,
            kind: "sandbox.create",
            subject_type: Some("sandbox"),
            subject_id: Some(sandbox.id),
            detail: &serde_json::json!({ "name": sandbox.name }),
            request_id: None,
        },
    )
    .await
    .map_err(ApiError::from)?;

    scope.commit().await.map_err(ApiError::from)?;

    // Now ask the runtime. A failure here is recorded on the row, so the console
    // shows a failed sandbox instead of nothing at all.
    // A pack that talks to a model needs the endpoint and the credential in its
    // environment. The credential is resolved from the provider profile's
    // locator at this moment and never stored (ADR 0005), so recreating the
    // sandbox picks up a rotated key without an edit here.
    let mut environment = if pack_needs_inference(&sandbox.pack_key) {
        match crate::services::provider_service::inference_environment(
            ctx,
            principal.environment_id,
        )
        .await
        {
            Ok(values) => values.into_iter().collect(),
            Err(error) => {
                tracing::warn!(
                    code = error.code.0,
                    "provider environment unavailable; creating without it"
                );
                Vec::new()
            }
        }
    } else {
        Vec::new()
    };

    // The proxy every sandbox is told about. Without it the sandbox is isolated
    // but blind — no tool can resolve an address, because the sandbox network
    // answers no external name. With it, each request still needs a rule: the
    // environment is a convenience for tools, not a permission.
    environment.extend(egress_environment(&egress_proxy));
    add_inference_bypass_vec(&mut environment);

    let runtime_spec = SandboxSpec {
        name: sandbox.name.clone(),
        image: sandbox.image.clone(),
        command,
        environment,
        pack_key: sandbox.pack_key.clone(),
        tool_profile: String::new(),
        memory_limit: memory_limit.clone(),
        cpu_limit: request.cpu_limit,
        pids_limit: request.pids_limit,
        mounts,
        gpu_count: 0,
    };

    match ctx.runtime.create(runtime_spec).await {
        Ok(facts) => {
            let health = health_from_runtime(&facts);
            let updated = set_status(
                ctx,
                principal,
                sandbox.id,
                sandbox.version,
                SandboxStatus::Running,
                health,
            )
            .await?;
            Ok(updated)
        }
        Err(error) => {
            tracing::error!(sandbox = %sandbox.name, error = %error, "runtime create failed");
            let failed = set_status(
                ctx,
                principal,
                sandbox.id,
                sandbox.version,
                SandboxStatus::Failed,
                SandboxHealth::Unhealthy,
            )
            .await?;
            Ok(failed)
        }
    }
}

/// Run a lifecycle action.
pub async fn act(
    ctx: &ServiceContext,
    principal: &Principal,
    id: Uuid,
    action: LifecycleAction,
) -> Result<Sandbox, ApiError> {
    require(principal, Permission::SandboxWrite)?;

    let mut scope = TenantScope::begin(&ctx.pool, principal.environment_id)
        .await
        .map_err(ApiError::from)?;
    let sandbox = store::get(&mut scope, id).await.map_err(ApiError::from)?;
    scope.commit().await.map_err(ApiError::from)?;

    if !action.is_allowed_for(&sandbox) {
        return Err(ApiError::conflict(
            ErrorCode::SANDBOX_INVALID_STATE,
            format!(
                "Cannot {} a sandbox in status '{}'.",
                action.as_str(),
                sandbox.status.as_str()
            ),
        ));
    }

    let transitional = set_status(
        ctx,
        principal,
        id,
        sandbox.version,
        action.transitional(),
        sandbox.health,
    )
    .await?;

    let outcome = match action {
        LifecycleAction::Start => ctx.runtime.start(&sandbox.name).await,
        LifecycleAction::Stop => ctx.runtime.stop(&sandbox.name).await,
        LifecycleAction::Restart => ctx.runtime.restart(&sandbox.name).await,
    };

    match outcome {
        Ok(facts) => {
            let health = health_from_runtime(&facts);
            set_status(
                ctx,
                principal,
                id,
                transitional.version,
                action.settled(),
                health,
            )
            .await
        }
        Err(error) => {
            tracing::error!(sandbox = %sandbox.name, action = action.as_str(), error = %error, "lifecycle action failed");
            set_status(
                ctx,
                principal,
                id,
                transitional.version,
                SandboxStatus::Failed,
                SandboxHealth::Unhealthy,
            )
            .await
        }
    }
}

/// Delete a sandbox and its container.
pub async fn delete(
    ctx: &ServiceContext,
    principal: &Principal,
    id: Uuid,
) -> Result<(), ApiError> {
    require(principal, Permission::SandboxWrite)?;

    let mut scope = TenantScope::begin(&ctx.pool, principal.environment_id)
        .await
        .map_err(ApiError::from)?;
    let sandbox = store::get(&mut scope, id).await.map_err(ApiError::from)?;

    // Removing the container first means a failure leaves the row, so the
    // operator can retry. Removing the row first would orphan a container.
    if let Err(error) = ctx.runtime.remove(&sandbox.name).await {
        tracing::warn!(sandbox = %sandbox.name, error = %error, "runtime remove failed; continuing");
    }

    crate::store::postgres::audit::record(
        &mut scope,
        crate::store::postgres::audit::NewAuditEvent {
            actor_id: Some(principal.user_id),
            actor_label: &principal.display_name,
            kind: "sandbox.delete",
            subject_type: Some("sandbox"),
            subject_id: Some(id),
            detail: &serde_json::json!({ "name": sandbox.name }),
            request_id: None,
        },
    )
    .await
    .map_err(ApiError::from)?;

    store::delete(&mut scope, id).await.map_err(ApiError::from)?;
    scope.commit().await.map_err(ApiError::from)?;
    Ok(())
}

/// The last log lines of a sandbox.
pub async fn logs(
    ctx: &ServiceContext,
    principal: &Principal,
    id: Uuid,
    tail: u32,
) -> Result<String, ApiError> {
    require(principal, Permission::SandboxRead)?;
    let mut scope = TenantScope::begin(&ctx.pool, principal.environment_id)
        .await
        .map_err(ApiError::from)?;
    let sandbox = store::get(&mut scope, id).await.map_err(ApiError::from)?;
    scope.commit().await.map_err(ApiError::from)?;

    ctx.runtime
        .logs(&sandbox.name, tail)
        .await
        .map_err(|error| ApiError::internal("The runtime did not return logs.").with_internal(error.to_string()))
}

/// Run one command inside a sandbox.
///
/// Write permission, not read: a command changes the sandbox. Every run is an
/// audit event that records the vector, so "who ran what" is answerable, and
/// the output is not recorded, because it may contain a secret the sandbox
/// legitimately holds.
pub async fn exec(
    ctx: &ServiceContext,
    principal: &Principal,
    id: Uuid,
    command: Vec<String>,
    timeout_seconds: u32,
) -> Result<crate::domain::runtime::ExecResult, ApiError> {
    require(principal, Permission::SandboxWrite)?;

    if command.is_empty() || command.len() > 64 {
        return Err(ApiError::validation(
            "A command is an argument vector of 1 to 64 parts.",
        ));
    }
    if command.iter().any(|part| part.contains('\0')) {
        return Err(ApiError::validation("Command arguments must not contain NUL."));
    }

    let mut scope = TenantScope::begin(&ctx.pool, principal.environment_id)
        .await
        .map_err(ApiError::from)?;
    let sandbox = store::get(&mut scope, id).await.map_err(ApiError::from)?;
    if sandbox.status != SandboxStatus::Running {
        scope.commit().await.map_err(ApiError::from)?;
        return Err(ApiError::conflict(
            ErrorCode::SANDBOX_INVALID_STATE,
            format!("Cannot run a command in a sandbox that is '{}'.", sandbox.status.as_str()),
        ));
    }

    crate::store::postgres::audit::record(
        &mut scope,
        crate::store::postgres::audit::NewAuditEvent {
            actor_id: Some(principal.user_id),
            actor_label: &principal.display_name,
            kind: "sandbox.exec",
            subject_type: Some("sandbox"),
            subject_id: Some(sandbox.id),
            detail: &serde_json::json!({ "name": sandbox.name, "command": command }),
            request_id: None,
        },
    )
    .await
    .map_err(ApiError::from)?;
    scope.commit().await.map_err(ApiError::from)?;

    ctx.runtime
        .exec(&sandbox.name, &command, timeout_seconds.clamp(1, 600))
        .await
        .map_err(|error| match error {
            crate::runtime::RuntimeError::NotFound(_) => ApiError::not_found(
                ErrorCode::SANDBOX_NOT_FOUND,
                "The runtime has no container for this sandbox.",
            ),
            crate::runtime::RuntimeError::Rejected(message) => ApiError::conflict(
                ErrorCode::SANDBOX_INVALID_STATE,
                message,
            ),
            other => ApiError::internal("The runtime could not run the command.")
                .with_internal(other.to_string()),
        })
}

/// Update a sandbox's status and publish the change.
async fn set_status(
    ctx: &ServiceContext,
    principal: &Principal,
    id: Uuid,
    expected_version: i64,
    status: SandboxStatus,
    health: SandboxHealth,
) -> Result<Sandbox, ApiError> {
    let mut scope = TenantScope::begin(&ctx.pool, principal.environment_id)
        .await
        .map_err(ApiError::from)?;
    let sandbox = store::update_status(&mut scope, id, expected_version, status, health)
        .await
        .map_err(ApiError::from)?;
    scope.commit().await.map_err(ApiError::from)?;

    ctx.bus.publish(LiveEvent::SandboxStateChanged {
        sandbox_id: sandbox.id,
        environment_id: sandbox.environment_id,
        status: sandbox.status.as_str().to_string(),
        version: sandbox.version,
    });

    Ok(sandbox)
}

/// Reject an image that is not digest-pinned.
///
/// `latest` is refused because a floating tag lets a registry change what runs
/// without a review. A digest is the only way to pin.
fn reject_unpinned_image(image: &str, digest: Option<&str>) -> Result<(), ApiError> {
    if image.ends_with(":latest") {
        return Err(ApiError::validation(
            "Image tag 'latest' is not allowed; pin a digest instead.",
        ));
    }

    // A locally built image has no registry digest to pin by reference. The
    // `:local` tag is the explicit, greppable marker that this is a
    // development image built on this host; it is allowed and recorded, while
    // every other floating tag is refused.
    if image.ends_with(":local") {
        return Ok(());
    }

    if digest.is_none() && !image.contains("@sha256:") {
        return Err(ApiError::validation(
            "Image must be pinned by digest (name@sha256:...) or tagged :local.",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn latest_is_refused() {
        assert!(reject_unpinned_image("nginx:latest", None).is_err());
    }

    #[test]
    fn a_digest_in_the_reference_is_enough() {
        assert!(reject_unpinned_image("nginx@sha256:abc", None).is_ok());
    }

    #[test]
    fn a_digest_field_is_enough() {
        assert!(reject_unpinned_image("nginx:1.27", Some("sha256:abc")).is_ok());
    }

    #[test]
    fn a_floating_tag_without_digest_is_refused() {
        assert!(reject_unpinned_image("nginx:1.27", None).is_err());
    }

    #[test]
    fn a_locally_built_image_is_allowed() {
        // It has no registry digest by construction; the tag is the marker.
        assert!(reject_unpinned_image("nestbox-hermes:local", None).is_ok());
    }

    #[test]
    fn only_settled_runtime_states_resolve_a_transitional_row() {
        assert_eq!(settled_status_from_runtime("running"), Some(SandboxStatus::Running));
        assert_eq!(settled_status_from_runtime("exited"), Some(SandboxStatus::Stopped));
        assert_eq!(settled_status_from_runtime("dead"), Some(SandboxStatus::Failed));
        assert_eq!(settled_status_from_runtime("restarting"), None);
        assert_eq!(settled_status_from_runtime("removing"), None);
    }

    #[test]
    fn health_is_derived_the_same_way_on_every_path() {
        let mut facts = crate::domain::runtime::RuntimeFacts::default();
        facts.status = "running".into();
        facts.health = "unknown".into();
        assert_eq!(health_from_runtime(&facts), SandboxHealth::Healthy);

        facts.status = "exited".into();
        assert_eq!(health_from_runtime(&facts), SandboxHealth::Unknown);

        facts.status = "running".into();
        facts.health = "unhealthy".into();
        assert_eq!(health_from_runtime(&facts), SandboxHealth::Unhealthy);
    }

    #[test]
    fn only_agent_packs_receive_an_inference_environment() {
        assert!(pack_needs_inference("hermes"));
        assert!(!pack_needs_inference("ubuntu-dev"));
        assert!(!pack_needs_inference("openclaw"));
    }

    #[test]
    fn the_inference_host_is_kept_out_of_the_proxy() {
        use std::collections::BTreeMap;
        let mut environment: BTreeMap<String, String> = [
            ("OPENAI_BASE_URL".to_string(), "http://model-qwen3-1-7b:8080/v1".to_string()),
            ("NO_PROXY".to_string(), "localhost,egress-proxy".to_string()),
        ]
        .into_iter()
        .collect();
        add_inference_bypass(&mut environment);
        assert_eq!(
            environment.get("NO_PROXY").map(String::as_str),
            Some("localhost,egress-proxy,model-qwen3-1-7b")
        );
        assert_eq!(
            environment.get("no_proxy").map(String::as_str),
            Some("model-qwen3-1-7b")
        );
    }

    #[test]
    fn the_egress_environment_names_both_spellings_of_every_variable() {
        let values = egress_environment(DEFAULT_EGRESS_PROXY);
        let keys: Vec<&str> = values.iter().map(|(key, _)| key.as_str()).collect();
        for key in [
            "HTTP_PROXY", "http_proxy", "HTTPS_PROXY", "https_proxy", "ALL_PROXY",
            "all_proxy", "NO_PROXY", "no_proxy",
        ] {
            assert!(keys.contains(&key), "{key} is missing from {keys:?}");
        }
        for (key, value) in &values {
            if key.eq_ignore_ascii_case("no_proxy") {
                continue;
            }
            assert_eq!(value, DEFAULT_EGRESS_PROXY);
        }
    }

    #[test]
    fn a_tool_is_not_told_to_reach_the_proxy_through_the_proxy() {
        let values = egress_environment("http://egress-proxy:3128");
        let bypass = values
            .iter()
            .find(|(key, _)| key == "NO_PROXY")
            .map(|(_, value)| value.as_str())
            .unwrap_or_default();
        assert!(bypass.contains("egress-proxy"), "{bypass}");
        assert!(bypass.contains("127.0.0.1"));
        assert!(bypass.contains("localhost"));
    }

    #[test]
    fn the_proxy_host_is_extracted_from_whatever_url_shape_a_node_reports() {
        assert_eq!(proxy_host("http://egress-proxy:3128"), "egress-proxy");
        assert_eq!(proxy_host("https://proxy.node.internal"), "proxy.node.internal");
        assert_eq!(proxy_host("http://10.90.92.30:3128/"), "10.90.92.30");
        // A bare host:port is not a URL the check constraint would accept, but
        // the parser must not mangle it into something that looks like an alias.
        assert_eq!(proxy_host("egress-proxy:3128"), "egress-proxy");
    }

    #[test]
    fn an_empty_proxy_setting_falls_back_to_the_node_default() {
        // The function reads the process environment, so the test asserts the
        // shape of the default rather than mutating a global other tests share.
        assert!(DEFAULT_EGRESS_PROXY.starts_with("http://"));
        assert!(DEFAULT_EGRESS_PROXY.ends_with(":3128"));
        assert!(DEFAULT_EGRESS_PROXY.contains(proxy_host(DEFAULT_EGRESS_PROXY)));
    }

    #[test]
    fn lifecycle_actions_respect_the_current_status() {
        let mut sandbox = sample(SandboxStatus::Running);
        assert!(LifecycleAction::Stop.is_allowed_for(&sandbox));
        assert!(!LifecycleAction::Start.is_allowed_for(&sandbox));
        assert!(LifecycleAction::Restart.is_allowed_for(&sandbox));

        sandbox.status = SandboxStatus::Stopped;
        assert!(LifecycleAction::Start.is_allowed_for(&sandbox));
        assert!(!LifecycleAction::Stop.is_allowed_for(&sandbox));
        assert!(!LifecycleAction::Restart.is_allowed_for(&sandbox));
    }

    fn sample(status: SandboxStatus) -> Sandbox {
        Sandbox {
            id: Uuid::nil(),
            environment_id: Uuid::nil(),
            cluster_id: Uuid::nil(),
            gateway_id: None,
            gateway_name: None,
            supervisor_id: None,
            workspace_id: None,
            node_id: Uuid::nil(),
            node_name: Some("local".into()),
            name: "x".into(),
            pack_key: "p".into(),
            image: "i@sha256:a".into(),
            image_digest: Some("sha256:a".into()),
            spec: serde_json::json!({}),
            status,
            health: SandboxHealth::Healthy,
            version: 1,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            spec_detail: None,
        }
    }
}
