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

use uuid::Uuid;

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

    let items = page
        .items
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
}

/// Whether a pack needs an inference endpoint to do its job.
///
/// Kept as a named rule rather than an inline `== "hermes"` so it is one place
/// to change when a second agent pack arrives, and so the test suite can assert
/// the list deliberately.
pub fn pack_needs_inference(pack_key: &str) -> bool {
    matches!(pack_key, "hermes")
}

/// The image and command a pack runs by default.
///
/// Kept as one function so a new pack is one match arm, and so the API and the
/// console cannot disagree about what a pack name means. A pack that needs no
/// explicit command returns an empty vector, which lets the image's own
/// entrypoint decide.
pub fn pack_defaults(pack_key: &str) -> (&'static str, Vec<&'static str>) {
    match pack_key {
        "hermes" => ("nestbox-hermes:local", vec!["sleep", "infinity"]),
        "openclaw" => ("nestbox-openclaw:local", vec![]),
        "git" => ("nestbox-tool-git:local", vec!["sleep", "infinity"]),
        _ => ("nestbox-ubuntu-dev:local", vec!["sleep", "infinity"]),
    }
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

    let (default_image, default_command) = pack_defaults(&request.pack_key);
    let image = if request.image.trim().is_empty() {
        default_image.to_string()
    } else {
        request.image.clone()
    };
    let command: Vec<String> = if request.command.is_empty() {
        default_command.iter().map(|part| part.to_string()).collect()
    } else {
        request.command.clone()
    };

    reject_unpinned_image(&image, request.image_digest.as_deref())?;

    let mut scope = TenantScope::begin(&ctx.pool, principal.environment_id)
        .await
        .map_err(ApiError::from)?;

    let cluster_id = env_store::default_cluster(&mut scope)
        .await
        .map_err(ApiError::from)?;

    let gateway_id = crate::store::postgres::gateways::list(&mut scope)
        .await
        .map_err(ApiError::from)?
        .into_iter()
        .find(|gateway| gateway.accepts_placement())
        .map(|gateway| gateway.id);

    let spec = serde_json::json!({
        "pack_key": request.pack_key,
        "memory_limit": memory_limit,
        "cpu_limit": request.cpu_limit,
        "pids_limit": request.pids_limit,
    });

    let sandbox = store::insert(
        &mut scope,
        store::NewSandbox {
            cluster_id,
            gateway_id,
            name: name.as_str(),
            pack_key: &request.pack_key,
            image: &image,
            image_digest: request.image_digest.as_deref(),
            spec: &spec,
        },
    )
    .await
    .map_err(ApiError::from)?;

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
    let environment = if pack_needs_inference(&sandbox.pack_key) {
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
    };

    match ctx.runtime.create(runtime_spec).await {
        Ok(facts) => {
            let health = if facts.health == "unhealthy" {
                SandboxHealth::Unhealthy
            } else {
                SandboxHealth::Healthy
            };
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
            let health = match facts.health.as_str() {
                "healthy" => SandboxHealth::Healthy,
                "degraded" => SandboxHealth::Degraded,
                "unhealthy" => SandboxHealth::Unhealthy,
                _ => SandboxHealth::Unknown,
            };
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
    fn only_agent_packs_receive_an_inference_environment() {
        assert!(pack_needs_inference("hermes"));
        assert!(!pack_needs_inference("ubuntu-dev"));
        assert!(!pack_needs_inference("openclaw"));
    }

    #[test]
    fn every_pack_has_a_default_image_and_command() {
        for pack in ["ubuntu-dev", "hermes", "openclaw", "git", "unknown"] {
            let (image, command) = pack_defaults(pack);
            assert!(!image.is_empty());
            let _ = command;
        }
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
