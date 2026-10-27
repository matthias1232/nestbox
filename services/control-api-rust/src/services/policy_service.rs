//! Policy validation, change semantics, rules, and rule drafts.

use uuid::Uuid;

use super::{require, ServiceContext};
use crate::domain::auth::{Permission, Principal};
use crate::domain::policy::{
    apply_outcome, rules_from_document, validate_document, ApplyOutcome, NetworkRule, Policy,
    PolicyScope, PolicySection,
};
use crate::error::{ApiError, ErrorCode};
use crate::store::TenantScope;
use crate::store::postgres::policies as store;
use crate::store::postgres::sandboxes as sandbox_store;

/// What the API reports after a save.
///
/// Three outcomes, because there are three real cases (ADR 0008). Returning a
/// boolean would force the console to guess which one it got.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
pub struct PolicySaveResult {
    pub policy: Policy,
    pub outcome: ApplyOutcome,
    /// The sections the document touches, so the console can name them.
    pub sections: Vec<String>,
    /// Whether any target sandbox is running, which is what made the outcome
    /// what it is.
    pub targets_running: bool,
}

/// List policies.
pub async fn list(
    ctx: &ServiceContext,
    principal: &Principal,
) -> Result<Vec<Policy>, ApiError> {
    require(principal, Permission::PolicyRead)?;
    let mut scope = TenantScope::begin(&ctx.pool, principal.environment_id)
        .await
        .map_err(ApiError::from)?;
    let policies = store::list(&mut scope).await.map_err(ApiError::from)?;
    scope.commit().await.map_err(ApiError::from)?;
    Ok(policies)
}

/// Read one policy.
pub async fn get(
    ctx: &ServiceContext,
    principal: &Principal,
    id: Uuid,
) -> Result<Policy, ApiError> {
    require(principal, Permission::PolicyRead)?;
    let mut scope = TenantScope::begin(&ctx.pool, principal.environment_id)
        .await
        .map_err(ApiError::from)?;
    let policy = store::get(&mut scope, id).await.map_err(ApiError::from)?;
    scope.commit().await.map_err(ApiError::from)?;
    Ok(policy)
}

/// The active global override, if any.
pub async fn global(
    ctx: &ServiceContext,
    principal: &Principal,
) -> Result<Option<Policy>, ApiError> {
    require(principal, Permission::PolicyRead)?;
    let mut scope = TenantScope::begin(&ctx.pool, principal.environment_id)
        .await
        .map_err(ApiError::from)?;
    let policy = store::global(&mut scope).await.map_err(ApiError::from)?;
    scope.commit().await.map_err(ApiError::from)?;
    Ok(policy)
}

/// Create a policy.
pub async fn create(
    ctx: &ServiceContext,
    principal: &Principal,
    name: &str,
    scope_value: PolicyScope,
    spec: &serde_json::Value,
) -> Result<PolicySaveResult, ApiError> {
    require(principal, Permission::PolicyWrite)?;
    if scope_value == PolicyScope::Global {
        // A global policy replaces every sandbox policy, so it needs the higher
        // permission even when the caller may edit ordinary policies.
        require(principal, Permission::PolicyGlobal)?;
    }

    let sections = validate_document(spec).map_err(policy_error)?;

    let mut scope = TenantScope::begin(&ctx.pool, principal.environment_id)
        .await
        .map_err(ApiError::from)?;
    let policy = store::insert(&mut scope, name, scope_value, spec, Some(principal.user_id))
        .await
        .map_err(ApiError::from)?;
    scope.commit().await.map_err(ApiError::from)?;

    Ok(PolicySaveResult {
        policy,
        outcome: ApplyOutcome::AppliesNow,
        sections: sections.iter().map(|s| s.as_str().to_string()).collect(),
        targets_running: false,
    })
}

/// Replace a policy document, reporting what the save will actually do.
pub async fn update(
    ctx: &ServiceContext,
    principal: &Principal,
    id: Uuid,
    expected_version: i64,
    spec: &serde_json::Value,
) -> Result<PolicySaveResult, ApiError> {
    require(principal, Permission::PolicyWrite)?;

    let sections = validate_document(spec).map_err(policy_error)?;

    let mut scope = TenantScope::begin(&ctx.pool, principal.environment_id)
        .await
        .map_err(ApiError::from)?;

    let existing = store::get(&mut scope, id).await.map_err(ApiError::from)?;
    if existing.scope == PolicyScope::Global {
        require(principal, Permission::PolicyGlobal)?;
    }

    // Whether the startup-section warning applies depends on whether anything is
    // actually running with this policy. Asking the store is what makes the
    // answer true rather than assumed.
    let targets_running = sandbox_store::count_by_status(&mut scope)
        .await
        .map_err(ApiError::from)?
        .iter()
        .any(|(status, count)| *count > 0 && (status == "running" || status == "starting"));

    let outcome = apply_outcome(&sections, targets_running);

    if outcome == ApplyOutcome::Rejected {
        return Err(ApiError::conflict(
            ErrorCode::POLICY_NEEDS_RECREATE,
            "This change cannot be applied to running sandboxes.",
        ));
    }

    let policy = store::update(&mut scope, id, expected_version, spec, Some(principal.user_id))
        .await
        .map_err(ApiError::from)?;
    scope.commit().await.map_err(ApiError::from)?;

    Ok(PolicySaveResult {
        policy,
        outcome,
        sections: sections.iter().map(|s| s.as_str().to_string()).collect(),
        targets_running,
    })
}

/// A line-by-line comparison of two documents.
///
/// Returned as a list of changed paths rather than a rendered patch: the console
/// decides how to present it, and a text patch would bake in one presentation.
#[derive(Debug, Clone, serde::Serialize, utoipa::ToSchema)]
pub struct PolicyDiff {
    pub policy_id: Uuid,
    pub from_version: i64,
    pub to_version: i64,
    pub changes: Vec<DiffEntry>,
}

#[derive(Debug, Clone, serde::Serialize, utoipa::ToSchema)]
pub struct DiffEntry {
    pub path: String,
    pub before: serde_json::Value,
    pub after: serde_json::Value,
}

/// Diff a policy's stored revision against another revision or its current one.
pub async fn diff(
    ctx: &ServiceContext,
    principal: &Principal,
    id: Uuid,
    against: Option<i64>,
) -> Result<PolicyDiff, ApiError> {
    require(principal, Permission::PolicyRead)?;

    let mut scope = TenantScope::begin(&ctx.pool, principal.environment_id)
        .await
        .map_err(ApiError::from)?;
    let current = store::get(&mut scope, id).await.map_err(ApiError::from)?;
    let from_version = against.unwrap_or(current.version.saturating_sub(1).max(1));
    let before = store::version_spec(&mut scope, id, from_version).await;
    scope.commit().await.map_err(ApiError::from)?;

    let before = match before {
        Ok(value) => value,
        Err(_) => serde_json::json!({}),
    };

    Ok(PolicyDiff {
        policy_id: id,
        from_version,
        to_version: current.version,
        changes: diff_json("", &before, &current.spec),
    })
}

/// Compare two JSON values and collect the changed leaf paths.
fn diff_json(path: &str, before: &serde_json::Value, after: &serde_json::Value) -> Vec<DiffEntry> {
    use serde_json::Value;

    if before == after {
        return Vec::new();
    }

    match (before, after) {
        (Value::Object(a), Value::Object(b)) => {
            let mut out = Vec::new();
            let mut keys: Vec<&String> = a.keys().chain(b.keys()).collect();
            keys.sort();
            keys.dedup();
            for key in keys {
                let child = if path.is_empty() {
                    key.clone()
                } else {
                    format!("{path}.{key}")
                };
                let left = a.get(key).cloned().unwrap_or(Value::Null);
                let right = b.get(key).cloned().unwrap_or(Value::Null);
                out.extend(diff_json(&child, &left, &right));
            }
            out
        }
        _ => vec![DiffEntry {
            path: path.to_string(),
            before: before.clone(),
            after: after.clone(),
        }],
    }
}

/// Delete a policy.
pub async fn delete(
    ctx: &ServiceContext,
    principal: &Principal,
    id: Uuid,
) -> Result<(), ApiError> {
    require(principal, Permission::PolicyWrite)?;
    let mut scope = TenantScope::begin(&ctx.pool, principal.environment_id)
        .await
        .map_err(ApiError::from)?;
    let policy = store::get(&mut scope, id).await.map_err(ApiError::from)?;
    if policy.scope == PolicyScope::Global {
        require(principal, Permission::PolicyGlobal)?;
    }
    store::delete(&mut scope, id).await.map_err(ApiError::from)?;
    scope.commit().await.map_err(ApiError::from)?;
    Ok(())
}

/// All network rules.
pub async fn list_rules(
    ctx: &ServiceContext,
    principal: &Principal,
) -> Result<Vec<NetworkRule>, ApiError> {
    require(principal, Permission::PolicyRead)?;
    let mut scope = TenantScope::begin(&ctx.pool, principal.environment_id)
        .await
        .map_err(ApiError::from)?;
    let rules = store::list_rules(&mut scope).await.map_err(ApiError::from)?;
    scope.commit().await.map_err(ApiError::from)?;
    Ok(rules)
}

/// Create a network rule.
pub async fn create_rule(
    ctx: &ServiceContext,
    principal: &Principal,
    rule: NetworkRule,
) -> Result<NetworkRule, ApiError> {
    require(principal, Permission::PolicyWrite)?;
    validate_rule(&rule)?;

    let mut scope = TenantScope::begin(&ctx.pool, principal.environment_id)
        .await
        .map_err(ApiError::from)?;
    let created = store::insert_rule(&mut scope, &rule)
        .await
        .map_err(ApiError::from)?;
    scope.commit().await.map_err(ApiError::from)?;
    Ok(created)
}

/// Delete a network rule.
pub async fn delete_rule(
    ctx: &ServiceContext,
    principal: &Principal,
    id: Uuid,
) -> Result<(), ApiError> {
    require(principal, Permission::PolicyWrite)?;
    let mut scope = TenantScope::begin(&ctx.pool, principal.environment_id)
        .await
        .map_err(ApiError::from)?;
    store::delete_rule(&mut scope, id).await.map_err(ApiError::from)?;
    scope.commit().await.map_err(ApiError::from)?;
    Ok(())
}

/// Draft a rule from a live event, without enforcing it (ADR 0011).
pub async fn draft_from_event(
    ctx: &ServiceContext,
    principal: &Principal,
    source_kind: &str,
    event: &serde_json::Value,
    sandbox_id: Option<Uuid>,
) -> Result<serde_json::Value, ApiError> {
    require(principal, Permission::PolicyWrite)?;

    let draft = serde_json::json!({
        "action": "deny",
        "host": event.get("destination").and_then(|v| v.as_str()).unwrap_or_default(),
        "port": event.get("port"),
        "protocol": event.get("protocol").and_then(|v| v.as_str()).unwrap_or("rest"),
        "enforcement": "enforced",
        "sandbox_id": sandbox_id,
        "source_event": event,
    });

    let mut scope = TenantScope::begin(&ctx.pool, principal.environment_id)
        .await
        .map_err(ApiError::from)?;

    let id = store::insert_draft(
        &mut scope,
        event.get("id").and_then(|v| v.as_str()).and_then(|s| Uuid::parse_str(s).ok()),
        source_kind,
        if sandbox_id.is_some() { "sandbox" } else { "policy" },
        sandbox_id,
        &draft,
        None,
        Some(principal.user_id),
    )
    .await
    .map_err(ApiError::from)?;
    scope.commit().await.map_err(ApiError::from)?;

    Ok(serde_json::json!({ "id": id, "draft": draft, "status": "pending" }))
}

/// Pending drafts.
pub async fn pending_drafts(
    ctx: &ServiceContext,
    principal: &Principal,
) -> Result<Vec<serde_json::Value>, ApiError> {
    require(principal, Permission::PolicyRead)?;
    let mut scope = TenantScope::begin(&ctx.pool, principal.environment_id)
        .await
        .map_err(ApiError::from)?;
    let drafts = store::list_pending_drafts(&mut scope)
        .await
        .map_err(ApiError::from)?;
    scope.commit().await.map_err(ApiError::from)?;

    Ok(drafts
        .into_iter()
        .map(|draft| {
            serde_json::json!({
                "id": draft.id,
                "source_kind": draft.source_kind,
                "target_type": draft.target_type,
                "target_id": draft.target_id,
                "draft": draft.draft,
                "diff": draft.diff,
                "status": draft.status,
            })
        })
        .collect())
}

/// Approve or reject a draft.
///
/// Approval is the only path that creates a rule, which is what makes the
/// two-step workflow real rather than cosmetic.
pub async fn decide_draft(
    ctx: &ServiceContext,
    principal: &Principal,
    id: Uuid,
    approve: bool,
    reason: Option<&str>,
) -> Result<serde_json::Value, ApiError> {
    require(principal, Permission::PolicyWrite)?;

    let mut scope = TenantScope::begin(&ctx.pool, principal.environment_id)
        .await
        .map_err(ApiError::from)?;

    let decided = store::decide_draft(
        &mut scope,
        id,
        approve,
        reason,
        Some(principal.user_id),
    )
    .await
    .map_err(|error| match error {
        crate::error::StoreError::VersionConflict => ApiError::conflict(
            ErrorCode::DRAFT_NOT_PENDING,
            "This draft has already been decided.",
        ),
        other => ApiError::from(other),
    })?;

    if approve {
        let draft = &decided.draft;
        let rule = NetworkRule {
            id: Uuid::nil(),
            environment_id: principal.environment_id,
            policy_id: None,
            sandbox_id: decided.target_id,
            name: draft
                .get("host")
                .and_then(|v| v.as_str())
                .unwrap_or("from-event")
                .to_string(),
            action: crate::domain::policy::RuleAction::Deny,
            host: draft
                .get("host")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string(),
            port: draft
                .get("port")
                .and_then(|v| v.as_i64())
                .and_then(|p| i32::try_from(p).ok()),
            protocol: crate::domain::policy::RuleProtocol::Rest,
            methods: Vec::new(),
            binaries: Vec::new(),
            enforcement: crate::domain::policy::Enforcement::Enforced,
            priority: 100,
            enabled: true,
            version: 1,
        };
        store::insert_rule(&mut scope, &rule)
            .await
            .map_err(ApiError::from)?;
    }

    scope.commit().await.map_err(ApiError::from)?;
    Ok(serde_json::json!({ "id": decided.id, "status": decided.status }))
}

/// Open a temporary traffic window.
pub async fn open_window(
    ctx: &ServiceContext,
    principal: &Principal,
    sandbox_id: Option<Uuid>,
    minutes: i32,
    reason: &str,
) -> Result<serde_json::Value, ApiError> {
    require(principal, Permission::PolicyWrite)?;
    if !(1..=1440).contains(&minutes) {
        return Err(ApiError::validation(
            "The window must last between 1 and 1440 minutes.",
        ));
    }
    if reason.trim().is_empty() {
        return Err(ApiError::validation(
            "A reason is required for a temporary traffic window.",
        ));
    }

    let mut scope = TenantScope::begin(&ctx.pool, principal.environment_id)
        .await
        .map_err(ApiError::from)?;
    let (id, expires_at) = store::open_window(
        &mut scope,
        sandbox_id,
        if sandbox_id.is_some() { "sandbox" } else { "environment" },
        reason,
        minutes,
        Some(principal.user_id),
    )
    .await
    .map_err(ApiError::from)?;
    scope.commit().await.map_err(ApiError::from)?;

    Ok(serde_json::json!({
        "id": id,
        "expires_at": expires_at,
        "minutes": minutes,
        "reason": reason,
    }))
}

/// Close a window early.
pub async fn close_window(
    ctx: &ServiceContext,
    principal: &Principal,
    id: Uuid,
) -> Result<(), ApiError> {
    require(principal, Permission::PolicyWrite)?;
    let mut scope = TenantScope::begin(&ctx.pool, principal.environment_id)
        .await
        .map_err(ApiError::from)?;
    store::revoke_window(&mut scope, id)
        .await
        .map_err(ApiError::from)?;
    scope.commit().await.map_err(ApiError::from)?;
    Ok(())
}

/// Project a policy document into rules, for the editor's read view.
pub fn project_rules(spec: &serde_json::Value) -> Vec<crate::domain::policy::NetworkRuleSeed> {
    rules_from_document(spec)
}

fn policy_error(error: crate::domain::policy::PolicyError) -> ApiError {
    ApiError::new(
        axum::http::StatusCode::UNPROCESSABLE_ENTITY,
        ErrorCode::POLICY_INVALID,
        error.to_string(),
    )
    .with_details(serde_json::json!({ "code": error.code() }))
}

fn validate_rule(rule: &NetworkRule) -> Result<(), ApiError> {
    if rule.host.trim().is_empty() {
        return Err(ApiError::validation("A rule needs a host."));
    }
    if rule.host.contains(' ') {
        return Err(ApiError::validation("A host must not contain spaces."));
    }
    if rule.policy_id.is_none() && rule.sandbox_id.is_none() {
        return Err(ApiError::validation(
            "A rule must target a policy or a sandbox.",
        ));
    }
    Ok(())
}

/// Sections a document touches, for the console's warning label.
pub fn sections_of(spec: &serde_json::Value) -> Vec<PolicySection> {
    validate_document(spec).unwrap_or_default()
}

/// Network rules attached to one sandbox.
pub async fn rules_for_sandbox(
    ctx: &ServiceContext,
    principal: &Principal,
    sandbox_id: Uuid,
) -> Result<Vec<NetworkRule>, ApiError> {
    require(principal, Permission::PolicyRead)?;
    let mut scope = TenantScope::begin(&ctx.pool, principal.environment_id)
        .await
        .map_err(ApiError::from)?;
    let rules = store::rules_for_sandbox(&mut scope, sandbox_id)
        .await
        .map_err(ApiError::from)?;
    scope.commit().await.map_err(ApiError::from)?;
    Ok(rules)
}
