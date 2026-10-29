//! Environment membership and summary.
//!
//! These two reads used to sit in the API layer, which the architecture test
//! caught: a handler that queries the store can bypass the tenant scope. They
//! live here so the scope is set in exactly one place.

use uuid::Uuid;

use super::{require, ServiceContext};
use crate::domain::auth::{Permission, Principal, Role};
use crate::domain::environment::EnvironmentSummary;
use crate::error::ApiError;
use crate::store::TenantScope;
use crate::store::postgres::{auth as auth_store, environments as env_store};

/// The environments a user may access, with their role in each.
pub async fn environments_for_user(
    ctx: &ServiceContext,
    principal: &Principal,
) -> Result<Vec<serde_json::Value>, ApiError> {
    let rows = auth_store::environments_for_user(&ctx.pool, principal.user_id)
        .await
        .map_err(ApiError::from)?;

    Ok(rows
        .into_iter()
        .map(|(id, name, role)| {
            serde_json::json!({
                "id": id,
                "name": name,
                "role": role.as_str(),
                "current": id == principal.environment_id,
            })
        })
        .collect())
}

/// Counts for the active environment.
pub async fn summary(
    ctx: &ServiceContext,
    principal: &Principal,
) -> Result<EnvironmentSummary, ApiError> {
    require(principal, Permission::SandboxRead)?;
    let mut scope = TenantScope::begin_for(&ctx.pool, principal)
        .await
        .map_err(ApiError::from)?;
    let summary = env_store::summary(&mut scope).await.map_err(ApiError::from)?;
    scope.commit().await.map_err(ApiError::from)?;
    Ok(summary)
}

/// The role a user holds, or `None` when they are not a member.
pub async fn role_in_environment(
    ctx: &ServiceContext,
    user_id: Uuid,
    environment_id: Uuid,
) -> Result<Option<Role>, ApiError> {
    auth_store::role_in_environment(&ctx.pool, user_id, environment_id)
        .await
        .map_err(ApiError::from)
}
