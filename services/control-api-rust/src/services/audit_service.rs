//! Reading the audit trail.
//!
//! Writing is done by the store at the point of the change; this module is only
//! the read side, so the API never touches the store for it.

use super::{require, ServiceContext};
use crate::domain::auth::{Permission, Principal};
use crate::error::ApiError;
use crate::store::TenantScope;
use crate::store::postgres::audit;

/// Recent audit events.
pub async fn list(
    ctx: &ServiceContext,
    principal: &Principal,
    limit: i64,
) -> Result<Vec<serde_json::Value>, ApiError> {
    require(principal, Permission::AuditRead)?;
    let mut scope = TenantScope::begin_for(&ctx.pool, principal)
        .await
        .map_err(ApiError::from)?;
    let events = audit::list(&mut scope, limit.clamp(1, 500))
        .await
        .map_err(ApiError::from)?;
    scope.commit().await.map_err(ApiError::from)?;
    Ok(events)
}
