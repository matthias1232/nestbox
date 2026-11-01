//! Switching a sandbox's egress mode while it runs (ADR 0052).
//!
//! The proxy already decides per CONNECT which mode applies, so a change needs
//! no new container: it is stored, the OpenShell policy is pushed, and the
//! console is told. Only `direct` still changes the network itself and therefore
//! keeps the old rebuild path. The revert timer and the "needs a recreate" probe
//! are filled by package B07.

use chrono::{DateTime, Utc};
use uuid::Uuid;

use super::{require, ServiceContext};
use crate::domain::auth::{Permission, Principal};
use crate::domain::policy::{EgressMode, EgressModeSetting};
use crate::error::ApiError;
use crate::store::TenantScope;

/// Change one sandbox's egress mode, live where that is possible.
pub async fn apply(
    ctx: &ServiceContext,
    principal: &Principal,
    sandbox_id: Uuid,
    mode: EgressMode,
    revert_after_minutes: Option<i32>,
) -> Result<EgressModeSetting, ApiError> {
    require(principal, Permission::SandboxPolicy)?;
    if let Some(minutes) = revert_after_minutes {
        if !(1..=10_080).contains(&minutes) {
            return Err(ApiError::validation(
                "A revert timer is 1 to 10080 minutes.",
            ));
        }
    }
    let mut scope = TenantScope::begin_for(&ctx.pool, principal)
        .await
        .map_err(ApiError::from)?;
    let sandbox = crate::store::postgres::sandboxes::get(&mut scope, sandbox_id)
        .await
        .map_err(ApiError::from)?;
    let previous = crate::store::postgres::policies::egress_mode_for_sandbox(&mut scope, sandbox_id)
        .await
        .map_err(ApiError::from)?
        .map(|setting| setting.mode)
        .unwrap_or(EgressMode::ForwardProxy);

    // `direct` is the one mode that changes how the container is attached to the
    // network and which exit proxy it uses; that cannot be done live.
    if mode == EgressMode::Direct || previous == EgressMode::Direct {
        scope.commit().await.map_err(ApiError::from)?;
        return super::policy_service::set_egress_mode_rebuilding(
            ctx, principal, sandbox_id, mode, None,
        )
        .await;
    }

    let revert_at: Option<DateTime<Utc>> =
        revert_after_minutes.map(|minutes| Utc::now() + chrono::Duration::minutes(minutes as i64));
    let revert_to = revert_at.map(|_| previous.as_str());
    let setting = crate::store::postgres::policies::set_egress_mode(
        &mut scope,
        sandbox_id,
        mode,
        None,
        true,
        revert_at,
        revert_to,
    )
    .await
    .map_err(ApiError::from)?;

    crate::store::postgres::audit::record(
        &mut scope,
        crate::store::postgres::audit::NewAuditEvent {
            actor_id: Some(principal.user_id),
            actor_label: &principal.display_name,
            kind: "sandbox.egress_mode",
            subject_type: Some("sandbox"),
            subject_id: Some(sandbox_id),
            detail: &serde_json::json!({
                "from": previous.as_str(),
                "to": mode.as_str(),
                "revert_at": revert_at,
            }),
            request_id: None,
        },
    )
    .await
    .map_err(ApiError::from)?;
    scope.commit().await.map_err(ApiError::from)?;

    // A sandbox without the CA mounted (created before the CA was mounted for
    // every proxied mode) needs one recreate before TLS can be opened.
    let mut setting = setting;
    if sandbox.status == crate::domain::sandbox::SandboxStatus::Running && mode.inspects_tls() {
        let probe = vec![
            "sh".to_string(),
            "-c".to_string(),
            "test -s /run/secrets/egress-ca.crt".to_string(),
        ];
        setting.needs_recreate = match ctx.runtime.exec(&sandbox.name, &probe, 5).await {
            Ok(result) => result.exit_code != Some(0),
            Err(_) => false,
        };
    }

    let ctx2 = ctx.clone();
    tokio::spawn(async move {
        if let Err(error) = super::policy_service::sync_openshell_policies(&ctx2).await {
            tracing::warn!(code = error.code.0, "OpenShell policies not pushed after a mode change");
        }
    });

    ctx.bus.publish(crate::domain::telemetry::LiveEvent::SandboxControlChanged {
        sandbox_id: Some(sandbox_id),
        environment_id: sandbox.environment_id,
        what: "egress_mode".to_string(),
        detail: serde_json::json!({
            "mode": mode.as_str(),
            "revert_at": revert_at,
            "needs_recreate": setting.needs_recreate,
        }),
    });
    Ok(setting)
}

/// Switch back every sandbox whose revert timer has passed (filled by package B07).
pub async fn revert_due(ctx: &ServiceContext) -> Result<(), ApiError> {
    let _ = ctx;
    Ok(())
}
