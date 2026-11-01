//! The one call the Workbox page needs: everything about a sandbox's controls,
//! and the cheap summary the admin list shows for every sandbox (ADR 0052-0056).
//!
//! Filled by package B25.

use uuid::Uuid;

use super::{require, ServiceContext};
use crate::domain::auth::{Permission, Principal};
use crate::error::ApiError;

/// Everything the Workbox shows for one sandbox.
pub async fn summary(
    ctx: &ServiceContext,
    principal: &Principal,
    sandbox_id: Uuid,
) -> Result<serde_json::Value, ApiError> {
    require(principal, Permission::SandboxRead)?;
    let _ = (ctx, sandbox_id);
    Ok(serde_json::json!({}))
}

/// The per-sandbox summary the admin list shows.
pub async fn overview(
    ctx: &ServiceContext,
    principal: &Principal,
) -> Result<serde_json::Value, ApiError> {
    require(principal, Permission::SandboxRead)?;
    let _ = ctx;
    Ok(serde_json::json!([]))
}
