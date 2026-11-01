//! Which models a sandbox may use, and where they are served (ADR 0057).
//!
//! A grant names a provider profile and the name the agent uses. Inheritance is
//! environment → user (of the owner) → sandbox, and the narrower scope wins. The
//! effective list feeds two readers: the agent configuration (so a new model is
//! visible without a new container) and the accounting proxy's model router (so a
//! request reaches whatever node serves the model).

use uuid::Uuid;

use super::{require, ServiceContext};
use crate::domain::auth::{Permission, Principal, Role};
use crate::domain::model_grant::{effective, EffectiveModel, ModelGrant, NewModelGrant};
use crate::error::{ApiError, ErrorCode};
use crate::store::TenantScope;

/// What the console calls the scopes; anything else is refused.
const SCOPES: [&str; 3] = ["environment", "user", "sandbox"];

fn validate(request: &NewModelGrant) -> Result<(), ApiError> {
    if !SCOPES.contains(&request.scope.as_str()) {
        return Err(ApiError::validation(
            "A grant is for the environment, a user or a sandbox.",
        ));
    }
    let shape = match request.scope.as_str() {
        "environment" => request.user_id.is_none() && request.sandbox_id.is_none(),
        "user" => request.user_id.is_some() && request.sandbox_id.is_none(),
        _ => request.sandbox_id.is_some() && request.user_id.is_none(),
    };
    if !shape {
        return Err(ApiError::validation(
            "An environment grant names no user and no sandbox, a user grant a user, a sandbox grant a sandbox.",
        ));
    }
    if !(1..=120).contains(&request.model_name.len())
        || !request
            .model_name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "._:/@-".contains(c))
    {
        return Err(ApiError::validation(
            "A model name is 1 to 120 characters of letters, digits and ._:/@-.",
        ));
    }
    Ok(())
}

/// The grants the caller may see.
pub async fn list(
    ctx: &ServiceContext,
    principal: &Principal,
    sandbox_id: Option<Uuid>,
    user_id: Option<Uuid>,
) -> Result<Vec<ModelGrant>, ApiError> {
    require(principal, Permission::SandboxRead)?;
    if principal.role != Role::Admin {
        if user_id.is_some() && user_id != Some(principal.user_id) {
            return Err(ApiError::forbidden(Permission::SandboxRead.code()));
        }
        if let Some(id) = sandbox_id {
            let mut scope = TenantScope::begin_for(&ctx.pool, principal)
                .await
                .map_err(ApiError::from)?;
            // Row-level security decides whether this sandbox is the caller's.
            crate::store::postgres::sandboxes::get(&mut scope, id)
                .await
                .map_err(ApiError::from)?;
            scope.commit().await.map_err(ApiError::from)?;
        }
    }
    let mut scope = TenantScope::begin_for(&ctx.pool, principal)
        .await
        .map_err(ApiError::from)?;
    let rows = crate::store::postgres::model_grants::list(
        &mut scope,
        sandbox_id,
        if principal.role == Role::Admin { user_id } else { user_id.or(Some(principal.user_id)) },
    )
    .await
    .map_err(ApiError::from)?;
    scope.commit().await.map_err(ApiError::from)?;
    Ok(rows)
}

/// Grant a model to the environment, a user or one sandbox.
pub async fn create(
    ctx: &ServiceContext,
    principal: &Principal,
    request: NewModelGrant,
) -> Result<ModelGrant, ApiError> {
    validate(&request)?;
    match request.scope.as_str() {
        "sandbox" => require(principal, Permission::SandboxWrite)?,
        _ => require(principal, Permission::EnvironmentManage)?,
    }
    let mut scope = TenantScope::begin_for(&ctx.pool, principal)
        .await
        .map_err(ApiError::from)?;
    if let Some(sandbox_id) = request.sandbox_id {
        crate::store::postgres::sandboxes::get(&mut scope, sandbox_id)
            .await
            .map_err(ApiError::from)?;
    }
    // A grant may only point at a provider profile this environment knows.
    crate::store::postgres::providers::get(&mut scope, request.provider_profile_id)
        .await
        .map_err(|error| match error {
            crate::error::StoreError::NotFound => {
                ApiError::validation("Unknown provider profile for this grant.")
            }
            other => ApiError::from(other),
        })?;
    let grant = crate::store::postgres::model_grants::insert(&mut scope, &request, Some(principal.user_id))
        .await
        .map_err(ApiError::from)?;
    crate::store::postgres::audit::record(
        &mut scope,
        crate::store::postgres::audit::NewAuditEvent {
            actor_id: Some(principal.user_id),
            actor_label: &principal.display_name,
            kind: "model.grant",
            subject_type: Some("model_grant"),
            subject_id: Some(grant.id),
            detail: &serde_json::json!({
                "scope": grant.scope,
                "model_name": grant.model_name,
                "profile": grant.provider_profile_id,
            }),
            request_id: None,
        },
    )
    .await
    .map_err(ApiError::from)?;
    let affected = crate::store::postgres::model_grants::affected_sandboxes(&mut scope, &grant)
        .await
        .map_err(ApiError::from)?;
    scope.commit().await.map_err(ApiError::from)?;
    apply_change(ctx, principal.environment_id, &affected).await;
    Ok(grant)
}

/// Revoke a grant.
pub async fn remove(ctx: &ServiceContext, principal: &Principal, id: Uuid) -> Result<(), ApiError> {
    let mut scope = TenantScope::begin_for(&ctx.pool, principal)
        .await
        .map_err(ApiError::from)?;
    let existing = crate::store::postgres::model_grants::list(&mut scope, None, None)
        .await
        .map_err(ApiError::from)?
        .into_iter()
        .find(|grant| grant.id == id)
        .ok_or_else(|| ApiError::not_found(ErrorCode::NOT_FOUND, "No such model grant."))?;
    match existing.scope.as_str() {
        "sandbox" => require(principal, Permission::SandboxWrite)?,
        _ => require(principal, Permission::EnvironmentManage)?,
    }
    let removed = crate::store::postgres::model_grants::delete(&mut scope, id)
        .await
        .map_err(ApiError::from)?;
    crate::store::postgres::audit::record(
        &mut scope,
        crate::store::postgres::audit::NewAuditEvent {
            actor_id: Some(principal.user_id),
            actor_label: &principal.display_name,
            kind: "model.revoke",
            subject_type: Some("model_grant"),
            subject_id: Some(id),
            detail: &serde_json::json!({ "model_name": existing.model_name }),
            request_id: None,
        },
    )
    .await
    .map_err(ApiError::from)?;
    let affected = crate::store::postgres::model_grants::affected_sandboxes(&mut scope, &existing)
        .await
        .map_err(ApiError::from)?;
    scope.commit().await.map_err(ApiError::from)?;
    if removed.is_some() {
        apply_change(ctx, principal.environment_id, &affected).await;
    }
    Ok(())
}

/// The models one sandbox may use, after inheritance.
pub async fn effective_for_sandbox(
    ctx: &ServiceContext,
    principal: &Principal,
    sandbox_id: Uuid,
) -> Result<Vec<EffectiveModel>, ApiError> {
    require(principal, Permission::SandboxRead)?;
    let mut scope = TenantScope::begin_for(&ctx.pool, principal)
        .await
        .map_err(ApiError::from)?;
    crate::store::postgres::sandboxes::get(&mut scope, sandbox_id)
        .await
        .map_err(ApiError::from)?;
    let owner = crate::store::postgres::model_grants::owner_of(&mut scope, sandbox_id)
        .await
        .map_err(ApiError::from)?;
    let grants = crate::store::postgres::model_grants::for_sandbox(&mut scope, sandbox_id, owner)
    .await
    .map_err(ApiError::from)?;
    scope.commit().await.map_err(ApiError::from)?;
    Ok(effective(&grants))
}

/// The models one sandbox may use, looked up by name (system path).
pub async fn effective_by_name(
    ctx: &ServiceContext,
    environment_id: Uuid,
    sandbox_name: &str,
) -> Result<Vec<EffectiveModel>, ApiError> {
    let mut scope = TenantScope::begin(&ctx.pool, environment_id)
        .await
        .map_err(ApiError::from)?;
    let Some((sandbox_id, owner)) =
        crate::store::postgres::model_grants::sandbox_by_name(&mut scope, sandbox_name)
            .await
            .map_err(ApiError::from)?
    else {
        scope.commit().await.map_err(ApiError::from)?;
        return Ok(Vec::new());
    };
    let grants = crate::store::postgres::model_grants::for_sandbox(&mut scope, sandbox_id, owner)
        .await
        .map_err(ApiError::from)?;
    scope.commit().await.map_err(ApiError::from)?;
    Ok(effective(&grants))
}

/// What the accounting proxy needs to route a sandbox's requests by model.
///
/// The authorization is resolved here and sent over the internal hop only: the
/// key never reaches a sandbox, a log line or a response the console sees.
pub async fn routes_for_accounting(
    ctx: &ServiceContext,
    sandbox_name: &str,
) -> Result<serde_json::Value, ApiError> {
    let mut scope = TenantScope::begin(&ctx.pool, primary_environment(ctx).await?)
        .await
        .map_err(ApiError::from)?;
    let Some((sandbox_id, owner)) =
        crate::store::postgres::model_grants::sandbox_by_name(&mut scope, sandbox_name)
            .await
            .map_err(ApiError::from)?
    else {
        scope.commit().await.map_err(ApiError::from)?;
        return Ok(serde_json::json!({ "default": null, "models": [] }));
    };
    let environment_id = scope.environment_id();
    let grants = crate::store::postgres::model_grants::for_sandbox(&mut scope, sandbox_id, owner)
        .await
        .map_err(ApiError::from)?;
    let models = effective(&grants);
    let mut out = Vec::new();
    for model in &models {
        let Ok(profile) = crate::store::postgres::providers::get(&mut scope, model.provider_profile_id).await
        else {
            continue;
        };
        let authorization = match profile.secret_ref.as_deref() {
            Some(locator) => crate::services::secret_service::credential(ctx, environment_id, locator)
                .await
                .ok()
                .map(|key| format!("Bearer {key}")),
            None => None,
        };
        if authorization.is_none() {
            // A model whose key cannot be resolved would be a 401 the user cannot
            // explain; it is left out and the caller falls back to the gateway.
            tracing::warn!(provider = %profile.name, "model grant skipped: no credential");
            continue;
        }
        let base = profile
            .endpoint
            .trim_end_matches('/')
            .trim_end_matches("/v1")
            .to_string();
        out.push(serde_json::json!({
            "model_name": model.model_name,
            "upstream_base": base,
            "upstream_model": model.upstream_model,
            "authorization": authorization,
        }));
    }
    let default = models
        .iter()
        .find(|model| model.is_default)
        .map(|model| model.model_name.clone());
    scope.commit().await.map_err(ApiError::from)?;
    Ok(serde_json::json!({ "default": default, "models": out }))
}

/// The first environment: the internal hop names only a sandbox name.
async fn primary_environment(ctx: &ServiceContext) -> Result<Uuid, ApiError> {
    sqlx::query_scalar("SELECT id FROM environments ORDER BY created_at, id LIMIT 1")
        .fetch_one(&ctx.pool)
        .await
        .map_err(|error| ApiError::from(crate::error::StoreError::Database(error)))
}

/// Rewrite the agent configuration of every affected running sandbox.
async fn apply_change(ctx: &ServiceContext, environment_id: Uuid, sandboxes: &[Uuid]) {
    for sandbox_id in sandboxes {
        let ctx2 = ctx.clone();
        let sandbox_id = *sandbox_id;
        tokio::spawn(async move {
            if let Err(error) = super::sandbox_service::write_agent_files(&ctx2, environment_id, sandbox_id).await {
                tracing::warn!(code = error.code.0, %sandbox_id, "agent files not refreshed after a model grant");
            }
            ctx2.bus.publish(crate::domain::telemetry::LiveEvent::ModelsChanged {
                sandbox_id: Some(sandbox_id),
                environment_id,
                detail: serde_json::json!({}),
            });
        });
    }
}
