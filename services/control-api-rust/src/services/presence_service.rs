//! Who is connected to a sandbox right now (ADR 0055).
//!
//! Presence is read from the session rows, so the owner and an admin see the
//! same list: terminals, SSH logins and desktop viewers, with the tab each is
//! in. Closing a tab for everyone is filled by package B14.

use uuid::Uuid;

use super::{require, ServiceContext};
use crate::domain::auth::{Permission, Principal};
use crate::domain::presence::PresenceEntry;
use crate::error::ApiError;
use crate::store::TenantScope;

/// How long an open row counts as present without a fresh heartbeat (seconds).
const PRESENT_SECONDS: i64 = 35 * 60;

/// Everyone connected to this sandbox now.
pub async fn list(
    ctx: &ServiceContext,
    principal: &Principal,
    sandbox_id: Uuid,
) -> Result<Vec<PresenceEntry>, ApiError> {
    require(principal, Permission::SandboxRead)?;
    let mut scope = TenantScope::begin_for(&ctx.pool, principal)
        .await
        .map_err(ApiError::from)?;
    crate::store::postgres::sandboxes::get(&mut scope, sandbox_id)
        .await
        .map_err(ApiError::from)?;
    let entries = crate::store::postgres::presence::list_open(&mut scope, sandbox_id, PRESENT_SECONDS)
        .await
        .map_err(ApiError::from)?;
    scope.commit().await.map_err(ApiError::from)?;
    Ok(entries)
}

/// Tell every open console that the people in this sandbox changed.
pub fn announce(ctx: &ServiceContext, environment_id: Uuid, sandbox_id: Uuid) {
    ctx.bus.publish(crate::domain::telemetry::LiveEvent::SandboxControlChanged {
        sandbox_id: Some(sandbox_id),
        environment_id,
        what: "presence".to_string(),
        detail: serde_json::json!({}),
    });
}

/// Close one terminal tab for everyone in the sandbox (filled by package B14).
pub async fn close_tab_for_all(
    ctx: &ServiceContext,
    principal: &Principal,
    sandbox_id: Uuid,
    tab_id: &str,
) -> Result<(), ApiError> {
    let _ = (ctx, principal, sandbox_id, tab_id);
    Err(ApiError::not_implemented("closing a shared tab"))
}
