//! Connecting a machine by its address and SSH credentials, and the state of
//! every node's tunnel (ADR 0057).
//!
//! `tunnels` and `set_egress_lock` are implemented here from the start; the
//! SSH install, the hub-initiated endpoint and the tunnel refresh are filled by
//! package B20.

use uuid::Uuid;

use super::{require, ServiceContext};
use crate::config::Config;
use crate::domain::auth::{Permission, Principal};
use crate::domain::node_tunnel::{ConnectNodeRequest, TunnelStatus};
use crate::error::ApiError;
use crate::store::TenantScope;

/// Install a machine as a node over SSH. Answers immediately; the work runs in
/// the background and the node appears when its tunnel is up.
pub async fn connect(
    ctx: &ServiceContext,
    config: &Config,
    principal: &Principal,
    request: ConnectNodeRequest,
) -> Result<serde_json::Value, ApiError> {
    let _ = (ctx, config, principal, request);
    Err(ApiError::not_implemented("connecting a machine by address"))
}

/// The tunnel of every node the caller can see.
pub async fn tunnels(
    ctx: &ServiceContext,
    principal: &Principal,
) -> Result<Vec<TunnelStatus>, ApiError> {
    require(principal, Permission::NodeRead)?;
    let mut scope = TenantScope::begin_for(&ctx.pool, principal)
        .await
        .map_err(ApiError::from)?;
    let rows = crate::store::postgres::node_gateway::tunnels(&mut scope)
        .await
        .map_err(ApiError::from)?;
    scope.commit().await.map_err(ApiError::from)?;
    Ok(rows)
}

/// Choose how a node reaches the internet.
pub async fn set_egress_lock(
    ctx: &ServiceContext,
    principal: &Principal,
    node_id: Uuid,
    lock: &str,
) -> Result<serde_json::Value, ApiError> {
    require(principal, Permission::EnvironmentManage)?;
    if !matches!(lock, "strict" | "exit_local") {
        return Err(ApiError::validation("A lock is 'strict' or 'exit_local'."));
    }
    let mut scope = TenantScope::begin_for(&ctx.pool, principal)
        .await
        .map_err(ApiError::from)?;
    crate::store::postgres::node_gateway::set_egress_lock(&mut scope, node_id, lock)
        .await
        .map_err(ApiError::from)?;
    crate::store::postgres::audit::record(
        &mut scope,
        crate::store::postgres::audit::NewAuditEvent {
            actor_id: Some(principal.user_id),
            actor_label: &principal.display_name,
            kind: "node.egress_lock",
            subject_type: Some("node"),
            subject_id: Some(node_id),
            detail: &serde_json::json!({ "lock": lock }),
            request_id: None,
        },
    )
    .await
    .map_err(ApiError::from)?;
    scope.commit().await.map_err(ApiError::from)?;
    Ok(serde_json::json!({
        "node_id": node_id,
        "lock": lock,
        "note": "The node applies it within five minutes (sync timer)."
    }))
}

/// The lock a node should apply: read by the node itself with its token.
pub async fn egress_lock_for_node(
    ctx: &ServiceContext,
    node_id: Uuid,
    presented_token: &str,
) -> Result<String, ApiError> {
    let _ = (ctx, node_id, presented_token);
    Err(ApiError::not_implemented("the node lock endpoint"))
}

/// Re-read the hub's WireGuard state and update the nodes (filled by package B20).
pub async fn refresh_tunnels(ctx: &ServiceContext, config: &Config) -> Result<(), ApiError> {
    let _ = (ctx, config);
    Ok(())
}
