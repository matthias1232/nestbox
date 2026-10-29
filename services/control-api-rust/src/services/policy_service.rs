//! Policy validation, change semantics, rules, and rule drafts.
//!
//! Also the egress decider (`check`): the one place that answers whether one
//! outbound request from one sandbox may proceed. It is here rather than in the
//! proxy so there is one rule set, not two (ADR 0001, 0013, 0022).

use chrono::Utc;
use uuid::Uuid;

use super::{require, ServiceContext};
use crate::domain::auth::{Permission, Principal};
use crate::domain::egress::{
    decide, CheckRequest, CheckResponse, CompiledPolicy, Decision, DecisionReason,
};
use crate::domain::policy::{
    apply_outcome, rules_from_document, validate_document, ApplyOutcome, DnsRule, EgressMode,
    EgressModeSetting, Matcher, NetworkRule, Policy, PolicyAssignment, PolicyScope, PolicySection,
    PolicyTemplate,
};
use crate::error::{ApiError, ErrorCode};
use crate::security::node_token;
use crate::store::TenantScope;
use crate::store::postgres::egress as egress_store;
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
            matchers: Vec::new(),
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

/// Validate a rule and put its matchers into the authoritative form.
///
/// The matchers are what the decider reads, so an unvalidated one would be a
/// rule that silently matches nothing. The flat host check stays: a rule the
/// console shows as "host example.com" must not be creatable without one.
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
    for matcher in &rule.matchers {
        Matcher::new(matcher.kind, matcher.value.clone()).map_err(|error| {
            ApiError::validation(format!("{error}")).with_details(serde_json::json!({
                "code": error.code(),
            }))
        })?;
    }
    Ok(())
}

/// Sections a document touches, for the console's warning label.
pub fn sections_of(spec: &serde_json::Value) -> Vec<PolicySection> {
    validate_document(spec).unwrap_or_default()
}

/// All DNS rules in this environment.
///
/// These are the names the proxy may resolve; they exist in the schema since
/// 0003 and got their API with the matcher change, so the console can offer
/// them like every other rule kind.
pub async fn list_dns_rules(
    ctx: &ServiceContext,
    principal: &Principal,
) -> Result<Vec<DnsRule>, ApiError> {
    require(principal, Permission::PolicyRead)?;
    let mut scope = TenantScope::begin(&ctx.pool, principal.environment_id)
        .await
        .map_err(ApiError::from)?;
    let rules = store::list_dns_rules(&mut scope)
        .await
        .map_err(ApiError::from)?;
    scope.commit().await.map_err(ApiError::from)?;
    Ok(rules)
}

/// Create a DNS rule.
pub async fn create_dns_rule(
    ctx: &ServiceContext,
    principal: &Principal,
    rule: DnsRule,
) -> Result<DnsRule, ApiError> {
    require(principal, Permission::PolicyWrite)?;

    let pattern = rule.pattern.trim().to_string();
    if pattern.is_empty() {
        return Err(ApiError::validation("A DNS rule needs a pattern."));
    }
    if pattern.contains(' ') {
        return Err(ApiError::validation("A pattern is one name or suffix."));
    }
    if rule.policy_id.is_none() && rule.sandbox_id.is_none() {
        return Err(ApiError::validation(
            "A DNS rule must target a policy or a sandbox.",
        ));
    }

    let mut scope = TenantScope::begin(&ctx.pool, principal.environment_id)
        .await
        .map_err(ApiError::from)?;
    let created = store::insert_dns_rule(
        &mut scope,
        &DnsRule { pattern, ..rule },
    )
    .await
    .map_err(ApiError::from)?;
    scope.commit().await.map_err(ApiError::from)?;
    Ok(created)
}

/// Delete one DNS rule.
pub async fn delete_dns_rule(
    ctx: &ServiceContext,
    principal: &Principal,
    id: Uuid,
) -> Result<(), ApiError> {
    require(principal, Permission::PolicyWrite)?;
    let mut scope = TenantScope::begin(&ctx.pool, principal.environment_id)
        .await
        .map_err(ApiError::from)?;
    store::delete_dns_rule(&mut scope, id)
        .await
        .map_err(ApiError::from)?;
    scope.commit().await.map_err(ApiError::from)?;
    Ok(())
}

/// One sandbox's egress mode. Absent is forward proxy, said as a value.
pub async fn egress_mode(
    ctx: &ServiceContext,
    principal: &Principal,
    sandbox_id: Uuid,
) -> Result<EgressModeSetting, ApiError> {
    require(principal, Permission::PolicyRead)?;
    let mut scope = TenantScope::begin(&ctx.pool, principal.environment_id)
        .await
        .map_err(ApiError::from)?;
    let sandbox = sandbox_store::get(&mut scope, sandbox_id)
        .await
        .map_err(ApiError::from)?;
    let setting = store::egress_mode_for_sandbox(&mut scope, sandbox.id)
        .await
        .map_err(ApiError::from)?
        .unwrap_or(EgressModeSetting {
            sandbox_id: sandbox.id,
            mode: EgressMode::ForwardProxy,
            trust_ca_ref: None,
            enabled: true,
            version: 0,
        });
    scope.commit().await.map_err(ApiError::from)?;
    Ok(setting)
}

/// Set one sandbox's egress mode.
pub async fn set_egress_mode(
    ctx: &ServiceContext,
    principal: &Principal,
    sandbox_id: Uuid,
    mode: EgressMode,
    trust_ca_ref: Option<String>,
) -> Result<EgressModeSetting, ApiError> {
    require(principal, Permission::PolicyWrite)?;
    let mut scope = TenantScope::begin(&ctx.pool, principal.environment_id)
        .await
        .map_err(ApiError::from)?;
    let sandbox = sandbox_store::get(&mut scope, sandbox_id)
        .await
        .map_err(ApiError::from)?;
    let setting = store::set_egress_mode(
        &mut scope,
        sandbox.id,
        mode,
        trust_ca_ref.as_deref(),
        true,
    )
    .await
    .map_err(ApiError::from)?;
    scope.commit().await.map_err(ApiError::from)?;
    Ok(setting)
}

/// The name of the policy a new sandbox gets when nobody names one.
///
/// This is the "safe defaults" promise, stated once: a sandbox starts with a
/// policy that denies what it does not need, and the operator can see it by
/// name and replace it by assigning another policy.
pub const SAFE_DEFAULTS_POLICY_NAME: &str = "Safe defaults (best practice)";

/// Apply a preset: turn its document into a policy, named after the template.
///
/// The document is validated like every save, so a template an operator edited
/// badly is refused with the same sentence any hand-written document gets. An
/// optional sandbox receives the new policy instead of its current one.
///
/// Applying a preset twice is not an error and not a second policy: a policy
/// with that name and that exact document is reused, which is what makes
/// "apply the safe defaults" idempotent. A name that is taken by a *different*
/// document gets a counted suffix, so the operator sees their second copy
/// instead of a 409 they cannot act on.
pub async fn instantiate_template(
    ctx: &ServiceContext,
    principal: &Principal,
    template_id: Uuid,
    assign_to: Option<Uuid>,
) -> Result<serde_json::Value, ApiError> {
    require(principal, Permission::PolicyWrite)?;

    let mut scope = TenantScope::begin(&ctx.pool, principal.environment_id)
        .await
        .map_err(ApiError::from)?;
    let template = store::get_template(&mut scope, template_id)
        .await
        .map_err(ApiError::from)?;
    let sections = validate_document(&template.spec).map_err(policy_error)?;

    let (name, existing) = policy_name_for(&mut scope, &template.name, &template.spec)
        .await
        .map_err(ApiError::from)?;
    let reused = existing.is_some();
    let policy = match existing {
        Some(policy) => policy,
        None => store::insert(
            &mut scope,
            &name,
            PolicyScope::Sandbox,
            &template.spec,
            Some(principal.user_id),
        )
        .await
        .map_err(ApiError::from)?,
    };
    let assignment = match assign_to {
        Some(sandbox_id) => {
            let sandbox = sandbox_store::get(&mut scope, sandbox_id)
                .await
                .map_err(ApiError::from)?;
            Some(
                store::assign_policy(&mut scope, sandbox.id, policy.id)
                    .await
                    .map_err(ApiError::from)?,
            )
        }
        None => None,
    };
    scope.commit().await.map_err(ApiError::from)?;

    Ok(serde_json::json!({
        "policy": policy,
        "template": template.name,
        "outcome": apply_outcome(&sections, false),
        "sections": sections.iter().map(|s| s.as_str().to_string()).collect::<Vec<_>>(),
        "assignment": assignment,
        "reused": reused,
    }))
}

/// The name a policy derived from a preset should carry, and the policy that
/// already holds it when the document is the same.
///
/// A preset is applied by pressing a button, so pressing it twice must not be a
/// conflict. The name is the preset's until something else uses it.
async fn policy_name_for(
    scope: &mut TenantScope<'_>,
    base: &str,
    spec: &serde_json::Value,
) -> Result<(String, Option<Policy>), crate::error::StoreError> {
    for attempt in 1..=50u32 {
        let candidate = if attempt == 1 {
            base.to_string()
        } else {
            format!("{base} ({attempt})")
        };
        match store::get_policy_by_name(scope, &candidate).await? {
            None => return Ok((candidate, None)),
            Some(policy) if policy.spec == *spec => return Ok((candidate, Some(policy))),
            Some(_) => continue,
        }
    }
    Ok((format!("{base} ({})", Uuid::new_v4()), None))
}

/// Assign a policy document to a sandbox.
///
/// This is how a sandbox gets away from the safe defaults: pick any policy in
/// this environment and it applies instead.
pub async fn assign_policy_to_sandbox(
    ctx: &ServiceContext,
    principal: &Principal,
    sandbox_id: Uuid,
    policy_id: Uuid,
) -> Result<serde_json::Value, ApiError> {
    require(principal, Permission::PolicyWrite)?;
    let mut scope = TenantScope::begin(&ctx.pool, principal.environment_id)
        .await
        .map_err(ApiError::from)?;
    let sandbox = sandbox_store::get(&mut scope, sandbox_id)
        .await
        .map_err(ApiError::from)?;
    // A policy that does not exist here is a 404 before the row is written:
    // assigning by guessed id must not attach a neighbour's policy.
    store::get(&mut scope, policy_id).await.map_err(ApiError::from)?;
    let assignment = store::assign_policy(&mut scope, sandbox.id, policy_id)
        .await
        .map_err(ApiError::from)?;
    scope.commit().await.map_err(ApiError::from)?;
    Ok(serde_json::json!({ "assignment": assignment }))
}

/// Remove a sandbox's policy assignment.
pub async fn unassign_policy_from_sandbox(
    ctx: &ServiceContext,
    principal: &Principal,
    sandbox_id: Uuid,
) -> Result<serde_json::Value, ApiError> {
    require(principal, Permission::PolicyWrite)?;
    let mut scope = TenantScope::begin(&ctx.pool, principal.environment_id)
        .await
        .map_err(ApiError::from)?;
    store::unassign_policy(&mut scope, sandbox_id)
        .await
        .map_err(ApiError::from)?;
    scope.commit().await.map_err(ApiError::from)?;
    Ok(serde_json::json!({ "assigned": false }))
}

/// The policy assigned to one sandbox, or `null` when it has none.
pub async fn policy_of_sandbox(
    ctx: &ServiceContext,
    principal: &Principal,
    sandbox_id: Uuid,
) -> Result<Option<PolicyAssignment>, ApiError> {
    require(principal, Permission::PolicyRead)?;
    let mut scope = TenantScope::begin(&ctx.pool, principal.environment_id)
        .await
        .map_err(ApiError::from)?;
    let assignment = store::assignment_for_sandbox(&mut scope, sandbox_id)
        .await
        .map_err(ApiError::from)?;
    scope.commit().await.map_err(ApiError::from)?;
    Ok(assignment)
}

/// All templates, the preset library the console offers.
pub async fn list_templates(
    ctx: &ServiceContext,
    principal: &Principal,
) -> Result<Vec<PolicyTemplate>, ApiError> {
    require(principal, Permission::PolicyRead)?;
    let mut scope = TenantScope::begin(&ctx.pool, principal.environment_id)
        .await
        .map_err(ApiError::from)?;
    let templates = store::list_templates(&mut scope)
        .await
        .map_err(ApiError::from)?;
    scope.commit().await.map_err(ApiError::from)?;
    Ok(templates)
}

/// Save an operator's own preset.
///
/// The document is validated before the row exists: a template that cannot
/// produce a valid policy is not stored, because a preset list that lies is
/// worse than a short one.
pub async fn create_template(
    ctx: &ServiceContext,
    principal: &Principal,
    name: &str,
    description: &str,
    spec: &serde_json::Value,
) -> Result<PolicyTemplate, ApiError> {
    require(principal, Permission::PolicyWrite)?;
    if name.trim().is_empty() {
        return Err(ApiError::validation("A template needs a name."));
    }
    validate_document(spec).map_err(policy_error)?;

    let mut scope = TenantScope::begin(&ctx.pool, principal.environment_id)
        .await
        .map_err(ApiError::from)?;
    let template = store::insert_template(
        &mut scope,
        name.trim(),
        description,
        spec,
    )
    .await
    .map_err(ApiError::from)?;
    scope.commit().await.map_err(ApiError::from)?;
    Ok(template)
}

/// The safe-defaults policy this environment carries, for the sandbox-create
/// path and for the console to show what a new sandbox starts with.
pub async fn safe_defaults(
    ctx: &ServiceContext,
    principal: &Principal,
) -> Result<Option<Policy>, ApiError> {
    require(principal, Permission::PolicyRead)?;
    let mut scope = TenantScope::begin(&ctx.pool, principal.environment_id)
        .await
        .map_err(ApiError::from)?;
    let policy = store::get_policy_by_name(&mut scope, SAFE_DEFAULTS_POLICY_NAME)
        .await
        .map_err(ApiError::from)?;
    scope.commit().await.map_err(ApiError::from)?;
    Ok(policy)
}

/// Give a sandbox its safe-defaults policy.
///
/// Called from the sandbox-create path. Assigning is best-effort: a sandbox
/// that already carries an assignment is left alone (the caller named one), and
/// a failure to assign must not fail the create, because a sandbox without a
/// policy is the same thing it was before this feature.
pub async fn apply_safe_defaults(
    ctx: &ServiceContext,
    environment_id: Uuid,
    sandbox_id: Uuid,
) {
    let mut scope = match TenantScope::begin(&ctx.pool, environment_id).await {
        Ok(scope) => scope,
        Err(error) => {
            tracing::warn!(
                environment_id = %environment_id,
                error = %error,
                "could not open a scope for the safe-defaults assignment"
            );
            return;
        }
    };
    let Some(policy) = store::get_policy_by_name(&mut scope, SAFE_DEFAULTS_POLICY_NAME)
        .await
        .map_err(ApiError::from)
        .ok()
        .flatten()
    else {
        tracing::warn!(
            environment_id = %environment_id,
            "no safe-defaults policy seeded; the sandbox starts without a policy"
        );
        return;
    };
    if store::assignment_for_sandbox(&mut scope, sandbox_id)
        .await
        .map_err(ApiError::from)
        .ok()
        .flatten()
        .is_some()
    {
        // The caller already chose a policy; their choice wins.
        return;
    }
    if let Err(error) = store::assign_policy(&mut scope, sandbox_id, policy.id).await {
        tracing::warn!(
            sandbox_id = %sandbox_id,
            error = %error,
            "the safe-defaults assignment failed"
        );
        return;
    }
    if let Err(error) = scope.commit().await {
        tracing::warn!(
            sandbox_id = %sandbox_id,
            error = %error,
            "the safe-defaults assignment did not commit"
        );
    }
}

/// The document schema the console's editor renders from.
///
/// A schema is not tenant data, but reading it is still an authenticated act:
/// the editor is an operator tool, so the permission is the same one that lets
/// a caller read a policy.
pub fn policy_schema(
    _ctx: &ServiceContext,
    principal: &Principal,
) -> Result<serde_json::Value, ApiError> {
    require(principal, Permission::PolicyRead)?;
    Ok(crate::domain::policy::policy_schema())
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
    // The effective view: exactly the rules the decider reads for this sandbox,
    // in the order it reads them (stored rules, then those compiled from the
    // governing policy's document). Showing only the sandbox's own rows hid the
    // policy that does most of the work.
    let governing = match store::global(&mut scope).await.map_err(ApiError::from)? {
        Some(global) => Some(global),
        None => match store::assignment_for_sandbox(&mut scope, sandbox_id)
            .await
            .map_err(ApiError::from)?
        {
            Some(assignment) => Some(
                store::get(&mut scope, assignment.policy_id)
                    .await
                    .map_err(ApiError::from)?,
            ),
            None => None,
        },
    };
    let mut rules = crate::domain::policy::rules_for_sandbox_view(
        store::list_rules(&mut scope).await.map_err(ApiError::from)?,
        sandbox_id,
        governing.as_ref().map(|policy| policy.id),
    );
    if let Some(policy) = &governing {
        rules.extend(crate::domain::policy::rules_from_policy(policy));
    }
    rules.sort_by_key(|rule| rule.priority);
    scope.commit().await.map_err(ApiError::from)?;
    Ok(rules)
}

// ---------------------------------------------------------------------------
// The egress decider
// ---------------------------------------------------------------------------

/// What the proxy asked, and what it told us about itself.
///
/// The token is not here: it is verified before a request reaches `check`, so a
/// decision can never be taken for a caller that has not been authenticated.
pub struct CheckInput {
    /// The node that is asking, from its token. A decision is only given about
    /// a sandbox that runs on this node.
    pub node_id: Uuid,
    /// The request as the proxy saw it.
    pub request: CheckRequest,
    /// Whether the proxy says it answered this from its own cache. We only see
    /// the calls it did not cache, so this is `false` unless it says otherwise;
    /// recording it keeps the column honest instead of always writing `false`.
    pub cache_hit: bool,
    /// The hash of the rule set the proxy says it is enforcing. Written to
    /// `nodes.policy_hash_reported`, which is what the console compares against
    /// `policy_compilations.hash` to show "not deployed".
    pub reported_hash: Option<String>,
}

/// One decision, with the rule set it came from.
pub struct CheckOutcome {
    pub response: CheckResponse,
    pub cache_hit: bool,
}

/// Verify the bearer token a node presents.
///
/// A service function rather than a handler step so the check cannot be wired to
/// a route that forgets it, and so the store lookup and the file read happen
/// behind one refusal message. Every failure is a 401: a caller must not learn
/// whether a node exists, has a token, or merely presented the wrong one.
pub async fn authenticate_node(
    ctx: &ServiceContext,
    node_id: Uuid,
    presented: &str,
) -> Result<(), ApiError> {
    let locator = egress_store::node_token_locator(&ctx.pool, node_id)
        .await
        .map_err(ApiError::from)?;

    let Some(locator) = locator else {
        tracing::warn!(%node_id, "policy check from a node without a token locator");
        return Err(node_token::NodeTokenError::NotConfigured.to_api());
    };

    node_token::verify(&locator, &ctx.secret_dir, presented).map_err(|error| {
        tracing::warn!(%node_id, reason = ?error, "node token refused");
        error.to_api()
    })
}

/// Decide one egress request and record the decision.
///
/// The order is the rule (see `domain::egress`): an open window allows, then the
/// first matching rule by ascending priority decides, and nothing matching
/// denies. The read of rules and windows, the compilation record, and the
/// evidence row share one transaction, so the row the operator sees later
/// describes the rule set that was actually applied.
///
/// Two cases never reach the rule set, because there is no honest answer to
/// give: a request without a sandbox (the proxy could not map the connection)
/// and a request about a sandbox on another node. Both are refused with
/// `default_deny` and are not written to `policy_checks`, which has no
/// environment to be scoped to — the warning in the log is the evidence.
pub async fn check(ctx: &ServiceContext, input: CheckInput) -> Result<CheckOutcome, ApiError> {
    let request = input.request;
    let cache_hit = input.cache_hit;

    if request.host.trim().is_empty() {
        return Ok(CheckOutcome {
            response: refusal(
                "The proxy sent no host, so no rule can be matched.".to_string(),
            ),
            cache_hit,
        });
    }

    let Some(sandbox_id) = request.sandbox_id else {
        return Ok(CheckOutcome {
            response: refusal(
                "The proxy did not identify a sandbox, so no window or rule applies.".to_string(),
            ),
            cache_hit,
        });
    };

    let Some(owner) = egress_store::sandbox_scope(&ctx.pool, sandbox_id)
        .await
        .map_err(ApiError::from)?
    else {
        tracing::warn!(%sandbox_id, %input.node_id, "policy check for an unknown sandbox");
        return Ok(CheckOutcome {
            response: refusal("The sandbox is not known to this control plane.".to_string()),
            cache_hit,
        });
    };

    if owner.node_id != input.node_id {
        tracing::warn!(
            %sandbox_id,
            asking_node = %input.node_id,
            owning_node = %owner.node_id,
            "policy check from a node that does not run the sandbox"
        );
        return Ok(CheckOutcome {
            response: refusal("The sandbox does not run on the node that asked.".to_string()),
            cache_hit,
        });
    }

    let mut scope = TenantScope::begin(&ctx.pool, owner.environment_id)
        .await
        .map_err(ApiError::from)?;

    // Which policy governs this sandbox: the global override when there is one,
    // otherwise the one assigned to it. Its document is compiled into rules here
    // (`rules_from_policy`), and stored rules are narrowed to the ones that apply
    // to this sandbox, so what the operator edits is what the proxy enforces.
    let governing = match store::global(&mut scope).await.map_err(ApiError::from)? {
        Some(global) => Some(global),
        None => match store::assignment_for_sandbox(&mut scope, sandbox_id)
            .await
            .map_err(ApiError::from)?
        {
            Some(assignment) => Some(
                store::get(&mut scope, assignment.policy_id)
                    .await
                    .map_err(ApiError::from)?,
            ),
            None => None,
        },
    };
    let mut rules = crate::domain::policy::rules_for_sandbox_view(
        store::list_rules(&mut scope).await.map_err(ApiError::from)?,
        sandbox_id,
        governing.as_ref().map(|policy| policy.id),
    );
    if let Some(policy) = &governing {
        rules.extend(crate::domain::policy::rules_from_policy(policy));
    }
    let windows = egress_store::open_windows(&mut scope, Some(sandbox_id))
        .await
        .map_err(ApiError::from)?;
    let hash = store::compiled_hash(&rules);

    let changed = egress_store::record_compilation(
        &mut scope,
        &hash,
        rules.len() as i64,
        windows.len() as i64,
    )
    .await
    .map_err(ApiError::from)?;
    if changed {
        tracing::info!(
            environment_id = %owner.environment_id,
            hash = %hash,
            rules = rules.len(),
            "compiled a new egress rule set"
        );
    }

    let now = Utc::now();
    let policy = CompiledPolicy {
        environment_id: owner.environment_id,
        hash: hash.clone(),
        compiled_at: now,
        rules,
        windows,
    };
    let mut response = decide(&request, &policy, now);
    // A rule compiled from a document has no row; a decision must not cite an id
    // that points at nothing.
    if response.rule_id == Some(Uuid::nil()) {
        response.rule_id = None;
    }
    // A DNS block is stronger than an allow: a name the operator has said a sandbox
    // must not resolve is not reachable by any network rule or window. (An allow
    // DNS rule adds nothing; the network rules still decide.)
    if response.decision != Decision::Deny {
        let dns_rules = store::list_dns_rules(&mut scope).await.map_err(ApiError::from)?;
        if let Some(blocked) = crate::domain::policy::dns_rule_for(
            &dns_rules,
            sandbox_id,
            governing.as_ref().map(|policy| policy.id),
            &request.host,
        )
        .filter(|rule| rule.action == crate::domain::policy::RuleAction::Deny)
        {
            response.decision = Decision::Deny;
            response.reason = DecisionReason::Rule;
            response.rule_id = None;
            response.window_id = None;
            response.explanation = format!(
                "The name {} is blocked by the DNS rule {}.",
                request.host, blocked.pattern
            );
        }
    }

    egress_store::record_decision(
        &mut scope,
        egress_store::NewDecision {
            sandbox_id: Some(sandbox_id),
            node_id: Some(input.node_id),
            ts: now,
            host: &request.host,
            port: request.port,
            protocol: request.protocol.as_deref().unwrap_or("tcp"),
            method: request.method.as_deref(),
            path: request.path.as_deref(),
            decision: response.decision.as_str(),
            reason: response.reason.as_str(),
            rule_id: response.rule_id,
            window_id: response.window_id,
            cache_hit,
        },
    )
    .await
    .map_err(ApiError::from)?;

    // The node reports what it enforces. Writing it on every call is what makes
    // a node that returns to the current rule set green again; the update only
    // touches a row whose value actually differs, so `reported_at` still means
    // "when this node changed what it enforces".
    if let Some(reported) = input.reported_hash.as_deref() {
        egress_store::record_reported_hash(&mut scope, input.node_id, reported)
            .await
            .map_err(ApiError::from)?;
    }

    scope.commit().await.map_err(ApiError::from)?;

    Ok(CheckOutcome {
        response,
        cache_hit,
    })
}

/// A refusal that came from the input rather than from a rule set.
///
/// `policy_hash` is empty because no rule set was consulted, and the cache TTL
/// is zero: caching "I could not tell" would pin a mistake for as long as the
/// proxy's TTL, and the sandbox may legitimately appear a second later.
fn refusal(explanation: String) -> CheckResponse {
    CheckResponse {
        decision: Decision::Deny,
        reason: DecisionReason::DefaultDeny,
        rule_id: None,
        window_id: None,
        explanation,
        policy_hash: String::new(),
        cache_ttl_seconds: 0,
    }
}

/// Whether each node runs the rule set that is compiled now.
///
/// The console's "deployed" badge is this call, not its own comparison: two
/// hashes read at different times can look equal while a compile happened in
/// between.
pub async fn deployment(
    ctx: &ServiceContext,
    principal: &Principal,
) -> Result<egress_store::PolicyDeployment, ApiError> {
    require(principal, Permission::PolicyRead)?;
    let mut scope = TenantScope::begin(&ctx.pool, principal.environment_id)
        .await
        .map_err(ApiError::from)?;
    let deployment = egress_store::deployment(&mut scope)
        .await
        .map_err(ApiError::from)?;
    scope.commit().await.map_err(ApiError::from)?;
    Ok(deployment)
}

