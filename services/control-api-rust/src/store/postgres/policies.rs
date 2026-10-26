//! Policies, versions, network rules, drafts, and traffic windows.

use sqlx::Row;
use uuid::Uuid;

use super::super::TenantScope;
use crate::domain::policy::{
    Enforcement, NetworkRule, Policy, PolicyScope, RuleAction, RuleProtocol,
};
use crate::error::StoreError;

fn row_to_policy(row: &sqlx::postgres::PgRow) -> Result<Policy, StoreError> {
    let scope_raw: String = row.try_get("scope")?;
    Ok(Policy {
        id: row.try_get("id")?,
        environment_id: row.try_get("environment_id")?,
        name: row.try_get("name")?,
        scope: PolicyScope::parse(&scope_raw).unwrap_or(PolicyScope::Sandbox),
        spec: row.try_get("spec")?,
        version: row.try_get("version")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

const POLICY_COLUMNS: &str =
    "id, environment_id, name, scope, spec, version, created_at, updated_at";

/// All policies in this environment.
pub async fn list(scope: &mut TenantScope<'_>) -> Result<Vec<Policy>, StoreError> {
    let sql = format!(
        "SELECT {POLICY_COLUMNS} FROM policies WHERE environment_id = $1 ORDER BY name"
    );
    let rows = sqlx::query(&sql)
        .bind(scope.environment_id())
        .fetch_all(scope.connection())
        .await?;

    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        out.push(row_to_policy(&row)?);
    }
    Ok(out)
}

/// One policy by id.
pub async fn get(scope: &mut TenantScope<'_>, id: Uuid) -> Result<Policy, StoreError> {
    let sql = format!(
        "SELECT {POLICY_COLUMNS} FROM policies WHERE environment_id = $1 AND id = $2"
    );
    let row = sqlx::query(&sql)
        .bind(scope.environment_id())
        .bind(id)
        .fetch_optional(scope.connection())
        .await?
        .ok_or(StoreError::NotFound)?;
    row_to_policy(&row)
}

/// The active global policy, if one exists.
///
/// At most one can exist; the schema enforces it with a partial unique index.
pub async fn global(scope: &mut TenantScope<'_>) -> Result<Option<Policy>, StoreError> {
    let sql = format!(
        "SELECT {POLICY_COLUMNS} FROM policies
          WHERE environment_id = $1 AND scope = 'global' LIMIT 1"
    );
    let row = sqlx::query(&sql)
        .bind(scope.environment_id())
        .fetch_optional(scope.connection())
        .await?;
    match row {
        Some(row) => Ok(Some(row_to_policy(&row)?)),
        None => Ok(None),
    }
}

/// Insert a policy and record its first version.
pub async fn insert(
    scope: &mut TenantScope<'_>,
    name: &str,
    scope_value: PolicyScope,
    spec: &serde_json::Value,
    created_by: Option<Uuid>,
) -> Result<Policy, StoreError> {
    let sql = format!(
        "WITH inserted AS (
            INSERT INTO policies (environment_id, name, scope, spec, version, created_by)
            VALUES ($1, $2, $3, $4, 1, $5)
            RETURNING {POLICY_COLUMNS}
         )
         SELECT {POLICY_COLUMNS} FROM inserted"
    );
    let row = sqlx::query(&sql)
        .bind(scope.environment_id())
        .bind(name)
        .bind(scope_value.as_str())
        .bind(spec)
        .bind(created_by)
        .fetch_one(scope.connection())
        .await?;

    let policy = row_to_policy(&row)?;
    sqlx::query(
        "INSERT INTO policy_versions (policy_id, version, spec, created_by)
         VALUES ($1, $2, $3, $4)",
    )
    .bind(policy.id)
    .bind(policy.version)
    .bind(&policy.spec)
    .bind(created_by)
    .execute(scope.connection())
    .await?;

    Ok(policy)
}

/// Replace a policy document, guarded by the expected version.
///
/// Writes a new row into `policy_versions` so rollback has something to read.
pub async fn update(
    scope: &mut TenantScope<'_>,
    id: Uuid,
    expected_version: i64,
    spec: &serde_json::Value,
    actor: Option<Uuid>,
) -> Result<Policy, StoreError> {
    let sql = format!(
        "WITH updated AS (
            UPDATE policies
               SET spec = $3, version = version + 1, updated_at = now()
             WHERE environment_id = $1 AND id = $2 AND version = $4
         RETURNING {POLICY_COLUMNS}
         )
         SELECT {POLICY_COLUMNS} FROM updated"
    );
    let row = sqlx::query(&sql)
        .bind(scope.environment_id())
        .bind(id)
        .bind(spec)
        .bind(expected_version)
        .fetch_optional(scope.connection())
        .await?;

    let Some(row) = row else {
        let exists: Option<Uuid> = sqlx::query_scalar(
            "SELECT id FROM policies WHERE environment_id = $1 AND id = $2",
        )
        .bind(scope.environment_id())
        .bind(id)
        .fetch_optional(scope.connection())
        .await?;
        return if exists.is_some() {
            Err(StoreError::VersionConflict)
        } else {
            Err(StoreError::NotFound)
        };
    };

    let policy = row_to_policy(&row)?;
    sqlx::query(
        "INSERT INTO policy_versions (policy_id, version, spec, created_by)
         VALUES ($1, $2, $3, $4)",
    )
    .bind(policy.id)
    .bind(policy.version)
    .bind(&policy.spec)
    .bind(actor)
    .execute(scope.connection())
    .await?;

    Ok(policy)
}

/// A stored revision, for the diff view.
pub async fn version_spec(
    scope: &mut TenantScope<'_>,
    policy_id: Uuid,
    version: i64,
) -> Result<serde_json::Value, StoreError> {
    // The join to policies is what keeps this tenant-scoped: a version row alone
    // does not carry an environment id.
    let row = sqlx::query(
        "SELECT v.spec
           FROM policy_versions v
           JOIN policies p ON p.id = v.policy_id
          WHERE p.environment_id = $1 AND v.policy_id = $2 AND v.version = $3",
    )
    .bind(scope.environment_id())
    .bind(policy_id)
    .bind(version)
    .fetch_optional(scope.connection())
    .await?
    .ok_or(StoreError::NotFound)?;
    Ok(row.try_get("spec")?)
}

/// Delete a policy.
pub async fn delete(scope: &mut TenantScope<'_>, id: Uuid) -> Result<(), StoreError> {
    let affected = sqlx::query("DELETE FROM policies WHERE environment_id = $1 AND id = $2")
        .bind(scope.environment_id())
        .bind(id)
        .execute(scope.connection())
        .await?
        .rows_affected();
    if affected == 0 {
        Err(StoreError::NotFound)
    } else {
        Ok(())
    }
}

fn row_to_rule(row: &sqlx::postgres::PgRow) -> Result<NetworkRule, StoreError> {
    let action_raw: String = row.try_get("action")?;
    let protocol_raw: String = row.try_get("protocol")?;
    let enforcement_raw: String = row.try_get("enforcement")?;
    Ok(NetworkRule {
        id: row.try_get("id")?,
        environment_id: row.try_get("environment_id")?,
        policy_id: row.try_get("policy_id")?,
        sandbox_id: row.try_get("sandbox_id")?,
        name: row.try_get("name")?,
        action: RuleAction::parse(&action_raw).unwrap_or(RuleAction::Allow),
        host: row.try_get("host")?,
        port: row.try_get("port")?,
        protocol: RuleProtocol::parse(&protocol_raw).unwrap_or(RuleProtocol::Rest),
        methods: row.try_get("methods")?,
        binaries: row.try_get("binaries")?,
        enforcement: Enforcement::parse(&enforcement_raw).unwrap_or(Enforcement::Enforced),
        priority: row.try_get("priority")?,
        enabled: row.try_get("enabled")?,
        version: row.try_get("version")?,
    })
}

const RULE_COLUMNS: &str = "id, environment_id, policy_id, sandbox_id, name, action, \
     host, port, protocol, methods, binaries, enforcement, priority, enabled, version";

/// All network rules in this environment.
pub async fn list_rules(scope: &mut TenantScope<'_>) -> Result<Vec<NetworkRule>, StoreError> {
    let sql = format!(
        "SELECT {RULE_COLUMNS} FROM network_rules
          WHERE environment_id = $1 ORDER BY priority, name"
    );
    let rows = sqlx::query(&sql)
        .bind(scope.environment_id())
        .fetch_all(scope.connection())
        .await?;
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        out.push(row_to_rule(&row)?);
    }
    Ok(out)
}

/// Rules attached to one sandbox, for the detail drawer.
pub async fn rules_for_sandbox(
    scope: &mut TenantScope<'_>,
    sandbox_id: Uuid,
) -> Result<Vec<NetworkRule>, StoreError> {
    let sql = format!(
        "SELECT {RULE_COLUMNS} FROM network_rules
          WHERE environment_id = $1 AND sandbox_id = $2 ORDER BY priority, name"
    );
    let rows = sqlx::query(&sql)
        .bind(scope.environment_id())
        .bind(sandbox_id)
        .fetch_all(scope.connection())
        .await?;
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        out.push(row_to_rule(&row)?);
    }
    Ok(out)
}

/// Insert a network rule.
pub async fn insert_rule(
    scope: &mut TenantScope<'_>,
    rule: &NetworkRule,
) -> Result<NetworkRule, StoreError> {
    let sql = format!(
        "WITH inserted AS (
            INSERT INTO network_rules
                (environment_id, policy_id, sandbox_id, name, action, host, port,
                 protocol, methods, binaries, enforcement, priority, enabled)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
            RETURNING {RULE_COLUMNS}
         )
         SELECT {RULE_COLUMNS} FROM inserted"
    );
    let row = sqlx::query(&sql)
        .bind(scope.environment_id())
        .bind(rule.policy_id)
        .bind(rule.sandbox_id)
        .bind(&rule.name)
        .bind(rule.action.as_str())
        .bind(&rule.host)
        .bind(rule.port)
        .bind(rule.protocol.as_str())
        .bind(&rule.methods)
        .bind(&rule.binaries)
        .bind(rule.enforcement.as_str())
        .bind(rule.priority)
        .bind(rule.enabled)
        .fetch_one(scope.connection())
        .await?;
    row_to_rule(&row)
}

/// Delete a network rule.
pub async fn delete_rule(scope: &mut TenantScope<'_>, id: Uuid) -> Result<(), StoreError> {
    let affected = sqlx::query("DELETE FROM network_rules WHERE environment_id = $1 AND id = $2")
        .bind(scope.environment_id())
        .bind(id)
        .execute(scope.connection())
        .await?
        .rows_affected();
    if affected == 0 {
        Err(StoreError::NotFound)
    } else {
        Ok(())
    }
}

/// A rule draft awaiting a decision.
pub struct DraftRecord {
    pub id: Uuid,
    pub source_kind: String,
    pub target_type: String,
    pub target_id: Option<Uuid>,
    pub draft: serde_json::Value,
    pub diff: Option<serde_json::Value>,
    pub status: String,
}

/// Pending drafts, for the approval queue.
pub async fn list_pending_drafts(
    scope: &mut TenantScope<'_>,
) -> Result<Vec<DraftRecord>, StoreError> {
    let rows = sqlx::query(
        "SELECT id, source_kind, target_type, target_id, draft, diff, status
           FROM rule_drafts
          WHERE environment_id = $1 AND status = 'pending'
          ORDER BY created_at DESC",
    )
    .bind(scope.environment_id())
    .fetch_all(scope.connection())
    .await?;

    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        out.push(DraftRecord {
            id: row.try_get("id")?,
            source_kind: row.try_get("source_kind")?,
            target_type: row.try_get("target_type")?,
            target_id: row.try_get("target_id")?,
            draft: row.try_get("draft")?,
            diff: row.try_get("diff")?,
            status: row.try_get("status")?,
        });
    }
    Ok(out)
}

/// Create a draft from an event.
pub async fn insert_draft(
    scope: &mut TenantScope<'_>,
    source_event_id: Option<Uuid>,
    source_kind: &str,
    target_type: &str,
    target_id: Option<Uuid>,
    draft: &serde_json::Value,
    diff: Option<&serde_json::Value>,
    created_by: Option<Uuid>,
) -> Result<Uuid, StoreError> {
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO rule_drafts
            (environment_id, source_event_id, source_kind, target_type, target_id,
             draft, diff, created_by)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
         RETURNING id",
    )
    .bind(scope.environment_id())
    .bind(source_event_id)
    .bind(source_kind)
    .bind(target_type)
    .bind(target_id)
    .bind(draft)
    .bind(diff)
    .bind(created_by)
    .fetch_one(scope.connection())
    .await?;
    Ok(id)
}

/// Decide a draft. Returns the draft only if it was still pending, which is what
/// makes a double approval a conflict rather than a duplicate rule.
pub async fn decide_draft(
    scope: &mut TenantScope<'_>,
    id: Uuid,
    approve: bool,
    reason: Option<&str>,
    decided_by: Option<Uuid>,
) -> Result<DraftRecord, StoreError> {
    let status = if approve { "approved" } else { "rejected" };
    let row = sqlx::query(
        "UPDATE rule_drafts
            SET status = $3, reason = $4, decided_by = $5, decided_at = now()
          WHERE environment_id = $1 AND id = $2 AND status = 'pending'
          RETURNING id, source_kind, target_type, target_id, draft, diff, status",
    )
    .bind(scope.environment_id())
    .bind(id)
    .bind(status)
    .bind(reason)
    .bind(decided_by)
    .fetch_optional(scope.connection())
    .await?;

    let Some(row) = row else {
        // Either it does not exist or it was already decided; the caller shows a
        // different message for each, so the distinction is preserved.
        let exists: Option<Uuid> = sqlx::query_scalar(
            "SELECT id FROM rule_drafts WHERE environment_id = $1 AND id = $2",
        )
        .bind(scope.environment_id())
        .bind(id)
        .fetch_optional(scope.connection())
        .await?;
        return if exists.is_some() {
            Err(StoreError::VersionConflict)
        } else {
            Err(StoreError::NotFound)
        };
    };

    Ok(DraftRecord {
        id: row.try_get("id")?,
        source_kind: row.try_get("source_kind")?,
        target_type: row.try_get("target_type")?,
        target_id: row.try_get("target_id")?,
        draft: row.try_get("draft")?,
        diff: row.try_get("diff")?,
        status: row.try_get("status")?,
    })
}

/// Open a temporary traffic window.
pub async fn open_window(
    scope: &mut TenantScope<'_>,
    sandbox_id: Option<Uuid>,
    scope_kind: &str,
    reason: &str,
    minutes: i32,
    created_by: Option<Uuid>,
) -> Result<(Uuid, chrono::DateTime<chrono::Utc>), StoreError> {
    let row = sqlx::query(
        "INSERT INTO traffic_windows
            (environment_id, sandbox_id, scope, reason, expires_at, created_by)
         VALUES ($1, $2, $3, $4, now() + make_interval(mins => $5), $6)
         RETURNING id, expires_at",
    )
    .bind(scope.environment_id())
    .bind(sandbox_id)
    .bind(scope_kind)
    .bind(reason)
    .bind(minutes)
    .bind(created_by)
    .fetch_one(scope.connection())
    .await?;
    Ok((row.try_get("id")?, row.try_get("expires_at")?))
}

/// Whether an unexpired window currently applies to a sandbox.
pub async fn window_active(
    scope: &mut TenantScope<'_>,
    sandbox_id: Uuid,
) -> Result<bool, StoreError> {
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM traffic_windows
          WHERE environment_id = $1
            AND revoked_at IS NULL
            AND expires_at > now()
            AND (sandbox_id = $2 OR sandbox_id IS NULL)",
    )
    .bind(scope.environment_id())
    .bind(sandbox_id)
    .fetch_one(scope.connection())
    .await?;
    Ok(count > 0)
}

/// Revoke a window early.
pub async fn revoke_window(scope: &mut TenantScope<'_>, id: Uuid) -> Result<(), StoreError> {
    let affected = sqlx::query(
        "UPDATE traffic_windows SET revoked_at = now()
          WHERE environment_id = $1 AND id = $2 AND revoked_at IS NULL",
    )
    .bind(scope.environment_id())
    .bind(id)
    .execute(scope.connection())
    .await?
    .rows_affected();
    if affected == 0 {
        Err(StoreError::NotFound)
    } else {
        Ok(())
    }
}

/// Expire windows that have passed their deadline. Called by the schedule.
///
/// Uses the admin path because it sweeps every environment.
pub async fn expire_windows(pool: &sqlx::PgPool) -> Result<u64, StoreError> {
    let affected = sqlx::query(
        "UPDATE traffic_windows SET revoked_at = now()
          WHERE revoked_at IS NULL AND expires_at <= now()",
    )
    .execute(pool)
    .await?
    .rows_affected();
    Ok(affected)
}
