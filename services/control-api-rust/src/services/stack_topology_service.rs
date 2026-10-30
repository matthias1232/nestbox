//! Reading the facts for the stack diagram, and the appliance's own components.
//!
//! The arrangement lives in `domain::stack_topology`, which is pure. This module
//! only reads: the stack's sandboxes, their published ports (with what the runner
//! says is open), MCP attachments, supervisors, gateways, the appliance's
//! containers and the last minutes of egress traffic. Anything that cannot be
//! read degrades to "unknown" in the diagram; a slow runner must not make the
//! whole map fail, because the map is what an operator opens when something is
//! wrong.

use std::time::Duration;

use chrono::Utc;
use uuid::Uuid;

use super::{access_service, require, traffic_service, ServiceContext};
use crate::domain::auth::{Permission, Principal};
use crate::domain::runtime::{ComponentLimits, ComponentState};
use crate::domain::stack_topology::{
    self, DnsFacts, EgressFacts, GatewayFacts, HostFacts, McpFacts, PortFacts, SandboxFacts,
    StackInput, StackTopology, SupervisorFacts,
};
use crate::error::{ApiError, ErrorCode};
use crate::store::postgres::access as access_store;
use crate::store::postgres::gateways as gateway_store;
use crate::store::postgres::mcp as mcp_store;
use crate::store::postgres::sandboxes as sandbox_store;
use crate::store::postgres::telemetry as telemetry_store;
use crate::store::TenantScope;

/// How long the runner may take to answer about a container.
const RUNNER_TIMEOUT: Duration = Duration::from_secs(4);
const MAX_LOG_TAIL: u32 = 1000;

/// The window for DNS evidence, in minutes. The same order as the traffic
/// window, so a blocked lookup and the traffic that never followed line up.
const DNS_WINDOW_MINUTES: i64 = 60;

/// The diagram of the stack a sandbox belongs to.
pub async fn stack(
    ctx: &ServiceContext,
    principal: &Principal,
    sandbox_id: Uuid,
    window: Option<&str>,
) -> Result<StackTopology, ApiError> {
    require(principal, Permission::SandboxRead)?;

    let mut scope = TenantScope::begin_for(&ctx.pool, principal)
        .await
        .map_err(ApiError::from)?;
    let subject = sandbox_store::get(&mut scope, sandbox_id)
        .await
        .map_err(|error| match error {
            crate::error::StoreError::NotFound => ApiError::not_found(
                ErrorCode::SANDBOX_NOT_FOUND,
                "No such sandbox in this environment.",
            ),
            other => ApiError::from(other),
        })?;
    let lead_id = subject.parent_id.unwrap_or(subject.id);
    let members = sandbox_store::stack_members(&mut scope, lead_id)
        .await
        .map_err(ApiError::from)?;

    let endpoint = access_store::node_endpoint(&mut scope, subject.node_id)
        .await
        .map_err(ApiError::from)?;
    let servers = mcp_store::list_servers(&mut scope).await.map_err(ApiError::from)?;
    let mut attachments = Vec::new();
    for sandbox in &members {
        let rows = mcp_store::list_attachments(&mut scope, sandbox.id)
            .await
            .map_err(ApiError::from)?;
        attachments.extend(rows);
    }
    let supervisors = gateway_store::list_supervisors(&mut scope)
        .await
        .map_err(ApiError::from)?;
    let gateways = gateway_store::list(&mut scope).await.map_err(ApiError::from)?;
    let mut roles = Vec::new();
    for sandbox in &members {
        let role = sandbox_store::pack_role(&mut scope, &sandbox.pack_key)
            .await
            .map_err(ApiError::from)?
            .unwrap_or_else(|| "agent".to_string());
        roles.push((sandbox.id, role));
    }
    // A blocked DNS decision leaves no connection behind, so the lookups are
    // read beside the members rather than from the traffic map. A telemetry
    // query that fails is simply no facts: the map is what an operator opens
    // when something is wrong, and a read gap must not turn that into an error.
    let mut dns = Vec::new();
    for sandbox in &members {
        if let Ok(rows) = telemetry_store::recent_dns(&mut scope, sandbox.id, DNS_WINDOW_MINUTES).await {
            dns.extend(rows.into_iter().map(|(host, decision, count)| DnsFacts {
                host,
                decision,
                count,
            }));
        }
    }
    scope.commit().await.map_err(ApiError::from)?;

    // Ports come from the same function the Access panel uses, so the two cannot
    // describe one port in different words or disagree about whether it is open.
    let mut ports = Vec::new();
    let mut summaries = std::collections::HashMap::new();
    for sandbox in &members {
        if let Ok(connect) = access_service::connect(ctx, principal, sandbox.id).await {
            summaries.insert(sandbox.id, connect.summary.clone());
            for port in connect.ports {
                ports.push(PortFacts {
                    sandbox_id: sandbox.id,
                    forward_id: port.forward_id,
                    purpose: port.purpose.as_str().to_string(),
                    label: port.label.clone().unwrap_or_else(|| port.purpose.as_str().to_string()),
                    kind: port.kind,
                    description: port.description,
                    target_port: port.target_port,
                    public_port: port.public_port,
                    dial: port.dial,
                    url: port.url,
                    listening: port.listening,
                    target_up: port.target_up,
                });
            }
        }
    }

    let components = tokio::time::timeout(RUNNER_TIMEOUT, ctx.runtime.components())
        .await
        .ok()
        .and_then(Result::ok);

    let mut sandboxes = Vec::new();
    for sandbox in &members {
        let facts = tokio::time::timeout(RUNNER_TIMEOUT, ctx.runtime.inspect(&sandbox.name))
            .await
            .ok()
            .and_then(Result::ok);
        let openshell = facts
            .as_ref()
            .is_some_and(|facts| facts.container_name.starts_with("openshell-default--"));
        let internal_address = facts.map(|facts| facts.ip).filter(|ip| !ip.is_empty());
        sandboxes.push(SandboxFacts {
            id: sandbox.id,
            name: sandbox.name.clone(),
            pack_key: sandbox.pack_key.clone(),
            role: roles
                .iter()
                .find(|(id, _)| *id == sandbox.id)
                .map(|(_, role)| role.clone())
                .unwrap_or_else(|| "agent".to_string()),
            summary: summaries.get(&sandbox.id).cloned().flatten(),
            status: sandbox.status.as_str().to_string(),
            health: sandbox.health.as_str().to_string(),
            parent_id: sandbox.parent_id,
            internal_address,
            gateway_name: sandbox.gateway_name.clone(),
            openshell,
        });
    }

    let mut egress = Vec::new();
    let mut window_label = window.map(str::to_string).unwrap_or_else(|| "5m".to_string());
    for sandbox in &members {
        // A sandbox nobody may read traffic for (a role without telemetry) simply
        // contributes no edges; it must not fail the diagram.
        if let Ok(view) = traffic_service::map(ctx, principal, sandbox.id, window, Some(50)).await {
            window_label = view.window.clone();
            egress.extend(view.edges.into_iter().map(|edge| EgressFacts {
                sandbox_id: sandbox.id,
                host: edge.host,
                port: edge.port,
                protocol: edge.protocol,
                requests: edge.requests,
                allowed: edge.allowed,
                denied: edge.denied,
            }));
        }
    }

    let member_ids: Vec<Uuid> = members.iter().map(|sandbox| sandbox.id).collect();
    let mut mcp = Vec::new();
    for attachment in attachments {
        let server = servers.iter().find(|server| server.key == attachment.server_key);
        mcp.push(McpFacts {
            sandbox_id: attachment.sandbox_id,
            key: attachment.server_key.clone(),
            name: server.map(|server| server.name.clone()).unwrap_or_else(|| attachment.server_key.clone()),
            transport: server.map(|server| server.transport.as_str().to_string()).unwrap_or_default(),
            endpoint: server.and_then(|server| server.endpoint.clone()),
            status: attachment.status.as_str().to_string(),
            tools: attachment.tools.as_array().map(Vec::len).unwrap_or(0),
            enabled: attachment.enabled,
            last_error: attachment.last_error.clone(),
        });
    }

    let supervisor_facts = supervisors
        .into_iter()
        .filter(|supervisor| supervisor.sandbox_id.is_some_and(|id| member_ids.contains(&id)))
        .map(|supervisor| {
            let count = |state: &str| {
                supervisor
                    .reported
                    .as_ref()
                    .and_then(|report| report.get("checks"))
                    .and_then(serde_json::Value::as_array)
                    .map(|checks| {
                        checks
                            .iter()
                            .filter(|check| check.get("state").and_then(serde_json::Value::as_str) == Some(state))
                            .count()
                    })
                    .unwrap_or(0)
            };
            SupervisorFacts {
                id: supervisor.id,
                name: supervisor.name.clone(),
                sandbox_id: supervisor.sandbox_id,
                status: supervisor.status.as_str().to_string(),
                version: supervisor.version.clone(),
                config_dirtiness: supervisor.config_dirtiness,
                last_seen: supervisor.last_seen,
                checks_failing: count("fail"),
                checks_warning: count("warn"),
            }
        })
        .collect();

    let placed: Vec<String> = members.iter().filter_map(|sandbox| sandbox.gateway_name.clone()).collect();
    let gateway_facts = gateways
        .into_iter()
        .filter(|gateway| placed.contains(&gateway.name))
        .map(|gateway| GatewayFacts {
            id: gateway.id,
            drifted: gateway.is_drifted(),
            name: gateway.name,
            status: gateway.status.as_str().to_string(),
            endpoint: gateway.endpoint,
            used: gateway.used,
            capacity: gateway.capacity,
        })
        .collect();

    Ok(stack_topology::build(&StackInput {
        subject: subject.id,
        now: Utc::now(),
        window: window_label,
        host: HostFacts {
            name: endpoint.name,
            address: endpoint.address,
            hostname: endpoint.hostname,
        },
        sandboxes,
        ports,
        mcp,
        supervisors: supervisor_facts,
        gateways: gateway_facts,
        components,
        egress,
        dns,
    }))
}

/// The appliance's own containers: state and health.
pub async fn components(
    ctx: &ServiceContext,
    principal: &Principal,
) -> Result<Vec<ComponentState>, ApiError> {
    require(principal, Permission::NodeRead)?;
    match tokio::time::timeout(RUNNER_TIMEOUT, ctx.runtime.components()).await {
        Ok(Ok(rows)) => Ok(rows),
        Ok(Err(error)) => Err(ApiError::internal("The runner did not describe the components.")
            .with_internal(error.to_string())),
        Err(_) => Err(ApiError::internal("The runner did not answer in time.")),
    }
}

/// Apply CPU/memory/process limits to one platform container.
///
/// Admin-only: these containers run every tenant's sandboxes, so a user editing
/// them would be editing shared infrastructure. The runner re-checks the name
/// against its own fixed list, and refuses a container that would cut the very
/// request asking for the change.
pub async fn set_component_limits(
    ctx: &ServiceContext,
    principal: &Principal,
    service: &str,
    limits: ComponentLimits,
) -> Result<(), ApiError> {
    require(principal, Permission::GatewayWrite)?;
    if limits.is_empty() {
        return Err(ApiError::validation(
            "Name a memory, CPU or process limit; an empty change does nothing.",
        ));
    }
    check_service_name(service)?;
    ctx.runtime
        .set_component_limits(service, limits)
        .await
        .map_err(component_error)
}

/// Restart one platform container, admin-only, with the reason a refusal names.
pub async fn restart_component(
    ctx: &ServiceContext,
    principal: &Principal,
    service: &str,
) -> Result<(), ApiError> {
    require(principal, Permission::GatewayWrite)?;
    check_service_name(service)?;
    ctx.runtime
        .restart_component(service)
        .await
        .map_err(component_error)
}

/// A component name is lowercase letters, digits and dashes — the same rule the
/// runner enforces before it looks anything up.
fn check_service_name(service: &str) -> Result<(), ApiError> {
    let valid = !service.is_empty()
        && service
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
    if valid {
        Ok(())
    } else {
        Err(ApiError::validation(
            "A component name is lowercase letters, digits and dashes.",
        ))
    }
}

/// A refusal from the runner is the operator's problem (409 with the reason), a
/// missing runtime is ours (500).
fn component_error(error: crate::runtime::RuntimeError) -> ApiError {
    match error {
        crate::runtime::RuntimeError::NotFound(_) => {
            ApiError::not_found(ErrorCode::NOT_FOUND, "No such platform component.")
        }
        crate::runtime::RuntimeError::Rejected(reason) => ApiError::validation(reason),
        crate::runtime::RuntimeError::Unavailable(reason) => {
            ApiError::internal("The runner could not change that component.").with_internal(reason)
        }
    }
}

/// The last lines of one component. The runner only reads the fixed list of
/// services; this layer rejects anything else first so a typo is a 404, not a
/// round trip.
pub async fn component_logs(
    ctx: &ServiceContext,
    principal: &Principal,
    service: &str,
    tail: u32,
) -> Result<String, ApiError> {
    // The platform's own logs name every user's sandboxes, hosts and actions: an admin
    // reads them (a user reads the logs of their own sandboxes instead).
    require(principal, Permission::GatewayWrite)?;
    if !service.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-') || service.is_empty() {
        return Err(ApiError::validation("A component name is lowercase letters, digits and dashes."));
    }
    let tail = tail.clamp(1, MAX_LOG_TAIL);
    ctx.runtime.component_logs(service, tail).await.map_err(|error| match error {
        crate::runtime::RuntimeError::NotFound(_) => {
            ApiError::not_found(ErrorCode::COMPONENT_NOT_FOUND, "That component has no container in this project.")
        }
        other => ApiError::internal("The runner did not return the component's log.")
            .with_internal(other.to_string()),
    })
}
