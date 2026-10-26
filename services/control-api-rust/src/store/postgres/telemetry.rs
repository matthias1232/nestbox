//! Telemetry event writes and reads, plus partition maintenance.
//!
//! Writes go through the admin pool because ingestion is one of the few paths
//! that is legitimately cross-tenant: a single gateway reports events for many
//! environments, and the writer trusts the event's own `environment_id`.
//! Reads are always tenant-scoped.

use chrono::{DateTime, Utc};
use sqlx::Row;
use uuid::Uuid;

use super::super::TenantScope;
use crate::domain::telemetry::{Decision, EventKind};
use crate::error::StoreError;
use crate::security::redaction;

/// A telemetry row to insert.
pub struct NewEvent<'a> {
    pub environment_id: Uuid,
    pub sandbox_id: Option<Uuid>,
    pub gateway_id: Option<Uuid>,
    pub kind: EventKind,
    pub source: Option<&'a str>,
    pub ts: DateTime<Utc>,
    pub detail: serde_json::Value,
}

/// Insert one event.
///
/// The detail is redacted before it is written, so a secret that slipped into an
/// upstream field does not become permanent history.
pub async fn insert(pool: &sqlx::PgPool, event: NewEvent<'_>) -> Result<Uuid, StoreError> {
    let redacted = redaction::redact_json(&event.detail);
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO telemetry_events
            (environment_id, sandbox_id, gateway_id, kind, source, ts, detail)
         VALUES ($1, $2, $3, $4, $5, $6, $7)
         RETURNING id",
    )
    .bind(event.environment_id)
    .bind(event.sandbox_id)
    .bind(event.gateway_id)
    .bind(event.kind.as_str())
    .bind(event.source)
    .bind(event.ts)
    .bind(redacted)
    .fetch_one(pool)
    .await?;
    Ok(id)
}

/// A stored event, as returned by a query.
#[derive(Debug, Clone, serde::Serialize)]
pub struct StoredEvent {
    pub id: Uuid,
    pub kind: String,
    pub sandbox_id: Option<Uuid>,
    pub ts: DateTime<Utc>,
    pub detail: serde_json::Value,
}

/// Read events for the console's history view.
///
/// The time range is required rather than optional: a query without one scans
/// every partition, which is the opposite of what partitions are for.
pub async fn list(
    scope: &mut TenantScope<'_>,
    kind: Option<EventKind>,
    since: DateTime<Utc>,
    limit: i64,
) -> Result<Vec<StoredEvent>, StoreError> {
    let rows = sqlx::query(
        "SELECT id, kind, sandbox_id, ts, detail
           FROM telemetry_events
          WHERE environment_id = $1
            AND ts >= $2
            AND ($3::text IS NULL OR kind = $3)
          ORDER BY ts DESC, id DESC
          LIMIT $4",
    )
    .bind(scope.environment_id())
    .bind(since)
    .bind(kind.map(|k| k.as_str()))
    .bind(limit)
    .fetch_all(scope.connection())
    .await?;

    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        out.push(StoredEvent {
            id: row.try_get("id")?,
            kind: row.try_get("kind")?,
            sandbox_id: row.try_get("sandbox_id")?,
            ts: row.try_get("ts")?,
            detail: row.try_get("detail")?,
        });
    }
    Ok(out)
}

/// Facet counts by kind, for the filter bar.
pub async fn facets(
    scope: &mut TenantScope<'_>,
    since: DateTime<Utc>,
) -> Result<std::collections::BTreeMap<String, i64>, StoreError> {
    let rows = sqlx::query(
        "SELECT kind, COUNT(*) AS n
           FROM telemetry_events
          WHERE environment_id = $1 AND ts >= $2
          GROUP BY kind",
    )
    .bind(scope.environment_id())
    .bind(since)
    .fetch_all(scope.connection())
    .await?;

    let mut out = std::collections::BTreeMap::new();
    for row in rows {
        out.insert(row.try_get("kind")?, row.try_get("n")?);
    }
    Ok(out)
}

/// Record an inference request in its own table, so the console can sort on
/// typed columns without parsing JSON on a hot path.
pub async fn insert_inference(
    pool: &sqlx::PgPool,
    event: &crate::domain::telemetry::InferenceEvent,
) -> Result<(), StoreError> {
    sqlx::query(
        "INSERT INTO inference_requests
            (id, environment_id, sandbox_id, provider_key, endpoint, model_key,
             method, path, status_code, latency_ms, tokens_in, tokens_out,
             bytes_in, bytes_out, decision, correlation_id, ts)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17)",
    )
    .bind(event.id)
    .bind(event.environment_id)
    .bind(event.sandbox_id)
    .bind(event.provider_key.as_deref())
    .bind(event.endpoint.as_deref())
    .bind(event.model_key.as_deref())
    .bind(event.method.as_deref())
    .bind(event.path.as_deref())
    .bind(event.status_code)
    .bind(event.latency_ms)
    .bind(event.tokens_in)
    .bind(event.tokens_out)
    .bind(event.bytes_in)
    .bind(event.bytes_out)
    .bind(match event.decision {
        Decision::Allow => "allow",
        Decision::Deny => "deny",
        Decision::Audit => "audit",
    })
    .bind(event.correlation_id.as_deref())
    .bind(event.ts)
    .execute(pool)
    .await?;
    Ok(())
}

/// Recent inference requests for one sandbox.
pub async fn inference_for_sandbox(
    scope: &mut TenantScope<'_>,
    sandbox_id: Uuid,
    limit: i64,
) -> Result<Vec<serde_json::Value>, StoreError> {
    let rows = sqlx::query(
        "SELECT provider_key, endpoint, model_key, method, path, status_code,
                latency_ms, tokens_in, tokens_out, decision, ts
           FROM inference_requests
          WHERE environment_id = $1 AND sandbox_id = $2
          ORDER BY ts DESC
          LIMIT $3",
    )
    .bind(scope.environment_id())
    .bind(sandbox_id)
    .bind(limit)
    .fetch_all(scope.connection())
    .await?;

    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        out.push(serde_json::json!({
            "provider_key": row.try_get::<Option<String>, _>("provider_key")?,
            "endpoint": row.try_get::<Option<String>, _>("endpoint")?,
            "model_key": row.try_get::<Option<String>, _>("model_key")?,
            "method": row.try_get::<Option<String>, _>("method")?,
            "path": row.try_get::<Option<String>, _>("path")?,
            "status_code": row.try_get::<Option<i32>, _>("status_code")?,
            "latency_ms": row.try_get::<Option<i32>, _>("latency_ms")?,
            "tokens_in": row.try_get::<Option<i32>, _>("tokens_in")?,
            "tokens_out": row.try_get::<Option<i32>, _>("tokens_out")?,
            "decision": row.try_get::<String, _>("decision")?,
            "ts": row.try_get::<DateTime<Utc>, _>("ts")?,
        }));
    }
    Ok(out)
}

/// Create the partitions for the next N months.
///
/// Required, not optional: an insert for a month with no partition lands in the
/// default partition, which is a safety net rather than a design.
pub async fn ensure_partitions(pool: &sqlx::PgPool, months_ahead: i32) -> Result<Vec<String>, StoreError> {
    // Built in SQL so the date arithmetic matches the server's clock and
    // timezone, which is what the partition bound must agree with.
    let rows = sqlx::query(
        "WITH months AS (
            SELECT generate_series(0, $1) AS i
         ),
         bounds AS (
            SELECT date_trunc('month', now())::date + (i || ' month')::interval AS start_at,
                   date_trunc('month', now())::date + ((i + 1) || ' month')::interval AS end_at
              FROM months
         )
         SELECT to_char(start_at, 'YYYY_MM') AS suffix,
                start_at::date AS start_at,
                end_at::date AS end_at
           FROM bounds",
    )
    .bind(months_ahead)
    .fetch_all(pool)
    .await?;

    let mut created = Vec::new();
    for row in rows {
        let suffix: String = row.try_get("suffix")?;
        let start_at: chrono::NaiveDate = row.try_get("start_at")?;
        let end_at: chrono::NaiveDate = row.try_get("end_at")?;

        for table in ["telemetry_events", "inference_requests"] {
            // The table name comes from this closed list, never from input.
            let name = format!("{table}_{suffix}");
            let sql = format!(
                "CREATE TABLE IF NOT EXISTS {name} PARTITION OF {table}
                 FOR VALUES FROM ('{start_at}') TO ('{end_at}')"
            );
            sqlx::query(&sql).execute(pool).await?;
            created.push(name);
        }
    }
    Ok(created)
}

/// Drop partitions whose entire range is older than the retention cut-off.
pub async fn drop_expired_partitions(
    pool: &sqlx::PgPool,
    retention_days: i32,
) -> Result<Vec<String>, StoreError> {
    let cutoff = Utc::now() - chrono::Duration::days(i64::from(retention_days));
    // Find child tables of the partitioned parents whose upper bound is before
    // the cut-off.
    let rows = sqlx::query(
        "SELECT c.relname AS name
           FROM pg_class c
           JOIN pg_inherits i ON i.inhrelid = c.oid
           JOIN pg_class p ON p.oid = i.inhparent
          WHERE p.relname IN ('telemetry_events', 'inference_requests')
            AND c.relname <> p.relname || '_default'
            AND pg_get_expr(c.relpartbound, c.oid) ~ 'TO \\('
            AND substring(pg_get_expr(c.relpartbound, c.oid) FROM 'TO \\(''([^'']+)''')::timestamptz < $1",
    )
    .bind(cutoff)
    .fetch_all(pool)
    .await?;

    let mut dropped = Vec::new();
    for row in rows {
        let name: String = row.try_get("name")?;
        // The name comes from the catalog, and is quoted as an identifier.
        let sql = format!("DROP TABLE IF EXISTS \"{}\"", name.replace('"', ""));
        sqlx::query(&sql).execute(pool).await?;
        dropped.push(name);
    }
    Ok(dropped)
}
