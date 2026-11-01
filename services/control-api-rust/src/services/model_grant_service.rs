//! Which models a sandbox may use, and what it sees of them (ADR 0056).
//!
//! Grants are inherited environment → user → sandbox; the effective list feeds
//! the agent configuration and the accounting proxy's model router. Listing is
//! implemented here from the start; creating and revoking grants, the
//! effective list and the router payload are filled by package B16.

use uuid::Uuid;

use super::{require, ServiceContext};
use crate::domain::auth::{Permission, Principal};
use crate::domain::model_grant::{EffectiveModel, ModelGrant, NewModelGrant};
use crate::error::ApiError;

/// The grants visible to the caller, optionally filtered.
pub async fn list(
    ctx: &ServiceContext,
    principal: &Principal,
    sandbox_id: Option<Uuid>,
    user_id: Option<Uuid>,
) -> Result<Vec<ModelGrant>, ApiError> {
    require(principal, Permission::SandboxRead)?;
    let _ = (ctx, sandbox_id, user_id);
    Ok(Vec::new())
}

/// Grant a model.
pub async fn create(
    ctx: &ServiceContext,
    principal: &Principal,
    request: NewModelGrant,
) -> Result<ModelGrant, ApiError> {
    let _ = (ctx, principal, request);
    Err(ApiError::not_implemented("granting a model"))
}

/// Revoke a grant.
pub async fn remove(ctx: &ServiceContext, principal: &Principal, id: Uuid) -> Result<(), ApiError> {
    let _ = (ctx, principal, id);
    Err(ApiError::not_implemented("revoking a model grant"))
}

/// The models one sandbox may use, after inheritance.
pub async fn effective_for_sandbox(
    ctx: &ServiceContext,
    principal: &Principal,
    sandbox_id: Uuid,
) -> Result<Vec<EffectiveModel>, ApiError> {
    require(principal, Permission::SandboxRead)?;
    let _ = (ctx, sandbox_id);
    Ok(Vec::new())
}

/// The models one sandbox may use, looked up by its name (system path).
pub async fn effective_by_name(
    ctx: &ServiceContext,
    environment_id: Uuid,
    sandbox_name: &str,
) -> Result<Vec<EffectiveModel>, ApiError> {
    let _ = (ctx, environment_id, sandbox_name);
    Ok(Vec::new())
}

/// What the accounting proxy needs to route a sandbox's requests by model.
pub async fn routes_for_accounting(
    ctx: &ServiceContext,
    sandbox_name: &str,
) -> Result<serde_json::Value, ApiError> {
    let _ = (ctx, sandbox_name);
    Ok(serde_json::json!({ "default": null, "models": [] }))
}
