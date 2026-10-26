//! Append-only audit writes.
//!
//! There is no update and no delete function here, deliberately. The database
//! role cannot perform one either, so the append-only property holds even if a
//! future caller tries.

use uuid::Uuid;

use super::super::TenantScope;
use crate::error::StoreError;
use crate::security::redaction;

/// One audit entry.
pub struct NewAuditEvent<'a> {
    pub actor_id: Option<Uuid>,
    pub actor_label: &'a str,
    pub kind: &'a str,
    pub subject_type: Option<&'a str>,
    pub subject_id: Option<Uuid>,
    pub detail: &'a serde_json::Value,
    pub request_id: Option<&'a str>,
}

/// Record an audit event.
///
/// The detail is redacted here rather than at the call site, so there is exactly
/// one place that can forget — and it does not.
pub async fn record(
    scope: &mut TenantScope<'_>,
    event: NewAuditEvent<'_>,
) -> Result<(), StoreError> {
    let redacted = redaction::redact_json(event.detail);
    sqlx::query(
        "INSERT INTO audit_events
            (environment_id, actor_id, actor_label, kind, subject_type, subject_id,
             detail, request_id)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
    )
    .bind(scope.environment_id())
    .bind(event.actor_id)
    .bind(event.actor_label)
    .bind(event.kind)
    .bind(event.subject_type)
    .bind(event.subject_id)
    .bind(redacted)
    .bind(event.request_id)
    .execute(scope.connection())
    .await?;
    Ok(())
}

/// A page of audit events.
pub async fn list(
    scope: &mut TenantScope<'_>,
    limit: i64,
) -> Result<Vec<serde_json::Value>, StoreError> {
    // `query` returns rows we map by hand because the shape is intentionally
    // loose: the detail column varies by event kind.
    let rows = sqlx::query(
        "SELECT id, actor_label, kind, subject_type, subject_id, detail,
                request_id, created_at
           FROM audit_events
          WHERE environment_id = $1
          ORDER BY created_at DESC, id DESC
          LIMIT $2",
    )
    .bind(scope.environment_id())
    .bind(limit)
    .fetch_all(scope.connection())
    .await?;

    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        use sqlx::Row;
        out.push(serde_json::json!({
            "id": row.try_get::<Uuid, _>("id")?,
            "actor_label": row.try_get::<String, _>("actor_label")?,
            "kind": row.try_get::<String, _>("kind")?,
            "subject_type": row.try_get::<Option<String>, _>("subject_type")?,
            "subject_id": row.try_get::<Option<Uuid>, _>("subject_id")?,
            "detail": row.try_get::<serde_json::Value, _>("detail")?,
            "request_id": row.try_get::<Option<String>, _>("request_id")?,
            "created_at": row.try_get::<chrono::DateTime<chrono::Utc>, _>("created_at")?,
        }));
    }
    Ok(out)
}
