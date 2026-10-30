//! Privacy settings and incidents (`0056_privacy_layer.sql`).
//!
//! Two aggregates, one file because they are one subject: the policy row a
//! sandbox inherits and the evidence rows of what that policy did. Both are
//! tenant-scoped and both are read through [`TenantScope`], so the database's
//! row-level security and the `environment_id` bind agree on every statement.
//!
//! Nothing here ever writes a matched value. `insert_incident` takes a masked
//! excerpt and a SHA-256 fingerprint, and the schema has no column that could
//! hold anything else — the rule survives a caller that forgets it.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use sqlx::Row;
use uuid::Uuid;

use super::super::TenantScope;
use crate::domain::privacy::{
    Action, Detector, IncidentAction, PrivacyIncident, PrivacySettings, PrivacyMode,
};
use crate::error::StoreError;

/// The settings columns, named so a schema change is a mapping error rather than
/// a silently swapped field.
const SETTINGS_COLUMNS: &str = "sandbox_id, mode, detectors, actions, \
     block_cloud_egress, retention_days, updated_by, updated_at";

/// The incident columns, named for the same reason.
const INCIDENT_COLUMNS: &str = "id, environment_id, sandbox_id, detector, action, severity, \
     masked_value, value_fingerprint, destination, cloud_delivery, created_at";

/// One incident to write: a masked excerpt, a fingerprint, and what happened.
///
/// No field carries the value, which is the point: the type cannot express a
/// write of the content even if a caller wanted one.
pub struct NewIncident<'a> {
    /// The sandbox the content belongs to; the incident row requires one.
    pub sandbox_id: Uuid,
    /// What fired.
    pub detector: Detector,
    /// What the policy did.
    pub action: IncidentAction,
    /// `info`, `warning` or `critical`; derived from the action by the service.
    pub severity: &'a str,
    /// First and last characters of the value, stars between.
    pub masked_value: &'a str,
    /// SHA-256 of the value, lowercase hex.
    pub value_fingerprint: &'a str,
    /// Where the content was going, when the caller named a destination.
    pub destination: Option<&'a str>,
    /// Whether the content counted as a delivery to a cloud destination.
    pub cloud_delivery: bool,
}

/// Whether a sandbox exists in this environment.
///
/// It lives here rather than in `sandboxes.rs` for the same reason the ingest
/// node lookup lives in `telemetry.rs`: it exists only for this write path,
/// which must know that the sandbox it is about to write an incident for is real
/// before it writes one. Reading the whole sandbox row (its joins, its spec)
/// would make a per-message inspection pay for a screen it never shows.
pub async fn sandbox_exists(
    scope: &mut TenantScope<'_>,
    sandbox_id: Uuid,
) -> Result<bool, StoreError> {
    let found: Option<Uuid> = sqlx::query_scalar(
        "SELECT id FROM sandboxes WHERE environment_id = $1 AND id = $2",
    )
    .bind(scope.environment_id())
    .bind(sandbox_id)
    .fetch_optional(scope.connection())
    .await?;
    Ok(found.is_some())
}

/// The settings row for one scope, or `None` when nothing was saved.
///
/// `sandbox_id: None` reads the environment default row. The comparison is
/// `IS NOT DISTINCT FROM` because the default row's key *is* NULL, and `= NULL`
/// matches nothing — the classic reason a "default" configuration silently never
/// applies.
pub async fn settings_for(
    scope: &mut TenantScope<'_>,
    sandbox_id: Option<Uuid>,
) -> Result<Option<PrivacySettings>, StoreError> {
    let statement = format!(
        "SELECT {SETTINGS_COLUMNS} FROM privacy_settings \
          WHERE environment_id = $1 AND sandbox_id IS NOT DISTINCT FROM $2"
    );
    let row = sqlx::query(&statement)
        .bind(scope.environment_id())
        .bind(sandbox_id)
        .fetch_optional(scope.connection())
        .await?;
    let Some(row) = row else {
        return Ok(None);
    };

    let mode: String = row.try_get("mode")?;
    let mode = PrivacyMode::parse(&mode)
        .ok_or_else(|| StoreError::UnexpectedData(format!("unknown privacy mode: {mode}")))?;

    let names: Vec<String> = row.try_get("detectors")?;
    let mut detectors = Vec::with_capacity(names.len());
    for name in names {
        let detector = Detector::parse(&name)
            .ok_or_else(|| StoreError::UnexpectedData(format!("unknown detector: {name}")))?;
        detectors.push(detector);
    }

    let actions: serde_json::Value = row.try_get("actions")?;
    let actions: BTreeMap<Detector, Action> = serde_json::from_value(actions)
        .map_err(|error| StoreError::UnexpectedData(format!("privacy actions: {error}")))?;

    let updated_at: DateTime<Utc> = row.try_get("updated_at")?;

    Ok(Some(PrivacySettings {
        sandbox_id: row.try_get("sandbox_id")?,
        mode,
        detectors,
        actions,
        block_cloud_egress: row.try_get("block_cloud_egress")?,
        retention_days: row.try_get("retention_days")?,
        updated_by: row.try_get("updated_by")?,
        updated_at: Some(updated_at),
    }))
}

/// Write the settings row for its scope, replacing what was there.
///
/// The conflict target is the named constraint because the default row's key
/// contains a NULL, and PostgreSQL's inference for a nullable key is only
/// reliable through the constraint name. Two concurrent saves therefore collide
/// on the constraint instead of producing two "defaults".
pub async fn save_settings(
    scope: &mut TenantScope<'_>,
    settings: &PrivacySettings,
) -> Result<(), StoreError> {
    let detectors: Vec<String> = settings
        .detectors
        .iter()
        .map(|detector| detector.as_str().to_string())
        .collect();
    let actions = serde_json::to_value(&settings.actions)
        .map_err(|error| StoreError::UnexpectedData(format!("privacy actions: {error}")))?;

    sqlx::query(
        "INSERT INTO privacy_settings (environment_id, sandbox_id, mode, detectors, actions, \
            block_cloud_egress, retention_days, updated_by, updated_at) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, now()) \
         ON CONFLICT ON CONSTRAINT privacy_settings_scope DO UPDATE SET \
            mode = EXCLUDED.mode, \
            detectors = EXCLUDED.detectors, \
            actions = EXCLUDED.actions, \
            block_cloud_egress = EXCLUDED.block_cloud_egress, \
            retention_days = EXCLUDED.retention_days, \
            updated_by = EXCLUDED.updated_by, \
            updated_at = now()",
    )
    .bind(scope.environment_id())
    .bind(settings.sandbox_id)
    .bind(settings.mode.as_str())
    .bind(&detectors)
    .bind(&actions)
    .bind(settings.block_cloud_egress)
    .bind(settings.retention_days)
    .bind(settings.updated_by)
    .execute(scope.connection())
    .await?;
    Ok(())
}

/// Remove a sandbox's override so it inherits the environment default again.
///
/// Returns whether a row was removed: "inherit again" on a sandbox that had no
/// override is not an error, but the caller can tell the two apart for the audit
/// trail.
pub async fn delete_override(
    scope: &mut TenantScope<'_>,
    sandbox_id: Uuid,
) -> Result<bool, StoreError> {
    let result = sqlx::query(
        "DELETE FROM privacy_settings WHERE environment_id = $1 AND sandbox_id = $2",
    )
    .bind(scope.environment_id())
    .bind(sandbox_id)
    .execute(scope.connection())
    .await?;
    Ok(result.rows_affected() > 0)
}

/// Write one incident and return its id.
///
/// The row is evidence, so it is insert-only: there is no update or delete
/// statement for an incident except the retention sweep, which removes whole
/// days rather than rewriting one record.
pub async fn insert_incident(
    scope: &mut TenantScope<'_>,
    incident: NewIncident<'_>,
) -> Result<Uuid, StoreError> {
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO privacy_incidents (environment_id, sandbox_id, detector, action, severity, \
            masked_value, value_fingerprint, destination, cloud_delivery) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9) \
         RETURNING id",
    )
    .bind(scope.environment_id())
    .bind(incident.sandbox_id)
    .bind(incident.detector.as_str())
    .bind(incident.action.as_str())
    .bind(incident.severity)
    .bind(incident.masked_value)
    .bind(incident.value_fingerprint)
    .bind(incident.destination)
    .bind(incident.cloud_delivery)
    .fetch_one(scope.connection())
    .await?;
    Ok(id)
}

/// The most recent incidents of one sandbox, newest first.
///
/// `limit` is bound, and the caller clamps it, so a list request cannot ask for
/// a full table. The index `privacy_incidents_sandbox_idx` is what keeps this
/// cheap on a sandbox with months of traffic.
pub async fn list_incidents(
    scope: &mut TenantScope<'_>,
    sandbox_id: Uuid,
    limit: i64,
) -> Result<Vec<PrivacyIncident>, StoreError> {
    let statement = format!(
        "SELECT {INCIDENT_COLUMNS} FROM privacy_incidents \
          WHERE environment_id = $1 AND sandbox_id = $2 \
          ORDER BY created_at DESC, id DESC \
          LIMIT $3"
    );
    let rows = sqlx::query(&statement)
        .bind(scope.environment_id())
        .bind(sandbox_id)
        .bind(limit)
        .fetch_all(scope.connection())
        .await?;

    let mut incidents = Vec::with_capacity(rows.len());
    for row in rows {
        let detector: String = row.try_get("detector")?;
        let detector = Detector::parse(&detector)
            .ok_or_else(|| StoreError::UnexpectedData(format!("unknown detector: {detector}")))?;
        let action: String = row.try_get("action")?;
        let action = IncidentAction::parse(&action).ok_or_else(|| {
            StoreError::UnexpectedData(format!("unknown incident action: {action}"))
        })?;
        incidents.push(PrivacyIncident {
            id: row.try_get("id")?,
            environment_id: row.try_get("environment_id")?,
            sandbox_id: row.try_get("sandbox_id")?,
            detector,
            action,
            severity: row.try_get("severity")?,
            masked_value: row.try_get("masked_value")?,
            value_fingerprint: row.try_get("value_fingerprint")?,
            destination: row.try_get("destination")?,
            cloud_delivery: row.try_get("cloud_delivery")?,
            created_at: row.try_get("created_at")?,
        });
    }
    Ok(incidents)
}

/// Delete this environment's incidents older than `cutoff` and report how many.
///
/// Retention, not a user delete: the caller passes the moment the window
/// started, derived from `retention_days`. A purge that a second request repeats
/// removes nothing the second time, so it is safe to run on a schedule.
pub async fn purge_before(
    scope: &mut TenantScope<'_>,
    cutoff: DateTime<Utc>,
) -> Result<u64, StoreError> {
    let result = sqlx::query(
        "DELETE FROM privacy_incidents WHERE environment_id = $1 AND created_at < $2",
    )
    .bind(scope.environment_id())
    .bind(cutoff)
    .execute(scope.connection())
    .await?;
    Ok(result.rows_affected())
}
