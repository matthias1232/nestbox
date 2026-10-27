//! Gateways: registry, settings, drift, and sync.

use uuid::Uuid;

use super::{require, ServiceContext};
use crate::domain::auth::{Permission, Principal};
use crate::domain::gateway::{Gateway, SettingsApplyMode, Supervisor, SyncOutcome, SyncResult};
use crate::domain::job::JobKind;
use crate::domain::telemetry::LiveEvent;
use crate::error::{ApiError, ErrorCode};
use crate::store::TenantScope;
use crate::store::postgres::gateways as store;

/// List gateways.
pub async fn list(ctx: &ServiceContext, principal: &Principal) -> Result<Vec<Gateway>, ApiError> {
    require(principal, Permission::GatewayRead)?;
    let mut scope = TenantScope::begin(&ctx.pool, principal.environment_id)
        .await
        .map_err(ApiError::from)?;
    let gateways = store::list(&mut scope).await.map_err(ApiError::from)?;
    scope.commit().await.map_err(ApiError::from)?;
    Ok(gateways)
}

/// Read one gateway.
pub async fn get(
    ctx: &ServiceContext,
    principal: &Principal,
    id: Uuid,
) -> Result<Gateway, ApiError> {
    require(principal, Permission::GatewayRead)?;
    let mut scope = TenantScope::begin(&ctx.pool, principal.environment_id)
        .await
        .map_err(ApiError::from)?;
    let gateway = store::get(&mut scope, id).await.map_err(ApiError::from)?;
    scope.commit().await.map_err(ApiError::from)?;
    Ok(gateway)
}

/// Replace a gateway's operator configuration.
///
/// This is not a policy change (ADR 0012): it bumps the desired settings version
/// and deliberately leaves the applied version alone, which is what creates the
/// drift the sync button resolves.
pub async fn update_settings(
    ctx: &ServiceContext,
    principal: &Principal,
    id: Uuid,
    expected_version: i64,
    settings: &serde_json::Value,
) -> Result<Gateway, ApiError> {
    require(principal, Permission::GatewayWrite)?;

    if !settings.is_object() {
        return Err(ApiError::validation("Gateway settings must be an object."));
    }

    let mut scope = TenantScope::begin(&ctx.pool, principal.environment_id)
        .await
        .map_err(ApiError::from)?;
    let gateway = store::update_settings(&mut scope, id, expected_version, settings)
        .await
        .map_err(ApiError::from)?;

    crate::store::postgres::audit::record(
        &mut scope,
        crate::store::postgres::audit::NewAuditEvent {
            actor_id: Some(principal.user_id),
            actor_label: &principal.display_name,
            kind: "gateway.settings_update",
            subject_type: Some("gateway"),
            subject_id: Some(id),
            detail: &serde_json::json!({ "desired_version": gateway.settings_version }),
            request_id: None,
        },
    )
    .await
    .map_err(ApiError::from)?;
    scope.commit().await.map_err(ApiError::from)?;

    ctx.bus.publish(LiveEvent::GatewayHealthChanged {
        gateway_id: gateway.id,
        environment_id: principal.environment_id,
        status: gateway.status.as_str().to_string(),
        drifted: gateway.is_drifted(),
    });

    Ok(gateway)
}

/// Push a gateway's desired configuration and mark it applied.
///
/// Idempotent: syncing a gateway that is already in sync reports
/// `AlreadyInSync` rather than pretending to do work. That is what makes the
/// scheduled run safe to repeat every 20 minutes.
pub async fn sync(
    ctx: &ServiceContext,
    principal: &Principal,
    id: Uuid,
) -> Result<SyncOutcome, ApiError> {
    require(principal, Permission::GatewayWrite)?;

    let mut scope = TenantScope::begin(&ctx.pool, principal.environment_id)
        .await
        .map_err(ApiError::from)?;
    let gateway = store::get(&mut scope, id).await.map_err(ApiError::from)?;

    if !gateway.is_drifted() {
        scope.commit().await.map_err(ApiError::from)?;
        return Ok(SyncOutcome {
            gateway_id: gateway.id,
            job_id: None,
            applied_version: gateway.settings_applied_version,
            outcome: SyncResult::AlreadyInSync,
        });
    }

    // A settings change that needs a restart cannot be pretended into effect.
    // The mapping is explicit so the console can say which one happened.
    let requires_restart = gateway
        .settings
        .get("requires_restart")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let job = crate::store::postgres::jobs::create(
        &mut scope,
        JobKind::GatewayConfigSync,
        &[gateway.id],
        &serde_json::json!({ "gateway_id": gateway.id, "mode": if requires_restart { SettingsApplyMode::RestartRequired } else { SettingsApplyMode::Hot } }),
        None,
        Some(principal.user_id),
    )
    .await
    .map_err(ApiError::from)?;

    store::mark_applied(&mut scope, gateway.id, gateway.settings_version)
        .await
        .map_err(ApiError::from)?;

    crate::store::postgres::audit::record(
        &mut scope,
        crate::store::postgres::audit::NewAuditEvent {
            actor_id: Some(principal.user_id),
            actor_label: &principal.display_name,
            kind: "gateway.sync",
            subject_type: Some("gateway"),
            subject_id: Some(gateway.id),
            detail: &serde_json::json!({ "applied_version": gateway.settings_version }),
            request_id: None,
        },
    )
    .await
    .map_err(ApiError::from)?;

    scope.commit().await.map_err(ApiError::from)?;

    ctx.bus.publish(LiveEvent::GatewayHealthChanged {
        gateway_id: gateway.id,
        environment_id: principal.environment_id,
        status: gateway.status.as_str().to_string(),
        drifted: false,
    });

    Ok(SyncOutcome {
        gateway_id: gateway.id,
        job_id: Some(job.id),
        applied_version: gateway.settings_version,
        outcome: if requires_restart {
            SyncResult::RestartRequired
        } else {
            SyncResult::Queued
        },
    })
}

/// Every gateway that is out of step, for the sync job.
pub async fn drifted(
    ctx: &ServiceContext,
    principal: &Principal,
) -> Result<Vec<Gateway>, ApiError> {
    require(principal, Permission::GatewayRead)?;
    let mut scope = TenantScope::begin(&ctx.pool, principal.environment_id)
        .await
        .map_err(ApiError::from)?;
    let gateways = store::drifted(&mut scope).await.map_err(ApiError::from)?;
    scope.commit().await.map_err(ApiError::from)?;
    Ok(gateways)
}

/// List supervisors.
pub async fn supervisors(
    ctx: &ServiceContext,
    principal: &Principal,
) -> Result<Vec<Supervisor>, ApiError> {
    require(principal, Permission::GatewayRead)?;
    let mut scope = TenantScope::begin(&ctx.pool, principal.environment_id)
        .await
        .map_err(ApiError::from)?;
    let supervisors = store::list_supervisors(&mut scope)
        .await
        .map_err(ApiError::from)?;
    scope.commit().await.map_err(ApiError::from)?;
    Ok(supervisors)
}

/// The cluster graph.
pub async fn topology(
    ctx: &ServiceContext,
    principal: &Principal,
) -> Result<serde_json::Value, ApiError> {
    require(principal, Permission::GatewayRead)?;
    let mut scope = TenantScope::begin(&ctx.pool, principal.environment_id)
        .await
        .map_err(ApiError::from)?;

    let mut topology = store::topology(&mut scope).await.map_err(ApiError::from)?;

    // Attach the sandboxes so the graph shows what runs where, which is the
    // whole point of a topology view.
    let sandboxes = crate::store::postgres::sandboxes::names(&mut scope)
        .await
        .map_err(ApiError::from)?;
    let gateways = store::list(&mut scope).await.map_err(ApiError::from)?;
    scope.commit().await.map_err(ApiError::from)?;

    let mut edges = Vec::new();
    if let Some(nodes) = topology.get_mut("nodes").and_then(|n| n.as_array_mut()) {
        for (sandbox_id, name) in sandboxes {
            nodes.push(serde_json::json!({
                "id": sandbox_id,
                "kind": "sandbox",
                "name": name,
            }));
        }
        for gateway in &gateways {
            edges.push(serde_json::json!({
                "from": gateway.id,
                "to": "cluster",
                "kind": "hosts",
            }));
        }
    }
    if let Some(object) = topology.as_object_mut() {
        object.insert("edges".into(), serde_json::Value::Array(edges));
    }

    Ok(topology)
}

/// Refuse an action when no gateway is healthy, with an explanation.
pub fn require_healthy(gateways: &[Gateway]) -> Result<&Gateway, ApiError> {
    gateways
        .iter()
        .find(|gateway| gateway.accepts_placement())
        .ok_or_else(|| {
            ApiError::new(
                axum::http::StatusCode::SERVICE_UNAVAILABLE,
                ErrorCode::GATEWAY_NOT_READY,
                "No gateway is healthy and in sync; placement is paused.",
            )
        })
}
