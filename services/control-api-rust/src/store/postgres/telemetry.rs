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
///
/// The insert runs inside a tenant scope because `telemetry_events` carries a
/// row-level policy: the API connects as `nestbox_app`, a non-owner role, so an
/// insert without `app.environment_id` set is refused by the database. The scope
/// is the same control the batch path uses; leaving it out here would make
/// single-event ingest fail quietly on the appliance and succeed in a test that
/// connects as the owner.
pub async fn insert(pool: &sqlx::PgPool, event: NewEvent<'_>) -> Result<Uuid, StoreError> {
    let redacted = redaction::redact_json(&event.detail);
    let mut scope = TenantScope::begin(pool, event.environment_id).await?;
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO telemetry_events
            (environment_id, sandbox_id, gateway_id, kind, source, ts, detail)
         VALUES ($1, $2, $3, $4, $5, $6, $7)
         RETURNING id",
    )
    .bind(scope.environment_id())
    .bind(event.sandbox_id)
    .bind(event.gateway_id)
    .bind(event.kind.as_str())
    .bind(event.source)
    .bind(event.ts)
    .bind(redacted)
    .fetch_one(scope.connection())
    .await?;
    scope.commit().await?;
    Ok(id)
}

/// One event in a proxy batch, with the id the ingest path assigned.
///
/// The id is assigned before the insert, not by the database, because the same
/// id keys the body row and the MCP call: an event and its body must not drift
/// apart, and a retried batch must be recognisable.
pub struct BatchEvent<'a> {
    pub id: Uuid,
    pub environment_id: Uuid,
    pub sandbox_id: Option<Uuid>,
    pub gateway_id: Option<Uuid>,
    pub kind: EventKind,
    pub source: Option<&'a str>,
    pub ts: DateTime<Utc>,
    pub detail: serde_json::Value,
}

/// Insert one batch of events in one transaction.
///
/// One transaction for two reasons: a batch is the unit the proxy retries, so a
/// half-stored batch is worse than a rejected one; and the tenant scope is set
/// once, which is what lets the insert satisfy row-level security on
/// `telemetry_events` when the API connects as the application role.
///
/// A retried batch collides on `(id, ts)` and is skipped rather than duplicated.
pub async fn insert_batch(
    pool: &sqlx::PgPool,
    environment_id: Uuid,
    events: &[BatchEvent<'_>],
) -> Result<u64, StoreError> {
    if events.is_empty() {
        return Ok(0);
    }

    let mut scope = TenantScope::begin(pool, environment_id).await?;
    let mut written = 0u64;

    for event in events {
        let redacted = redaction::redact_json(&event.detail);
        let result = sqlx::query(
            "INSERT INTO telemetry_events
                (id, environment_id, sandbox_id, gateway_id, kind, source, ts, detail)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
             ON CONFLICT (id, ts) DO NOTHING",
        )
        .bind(event.id)
        .bind(scope.environment_id())
        .bind(event.sandbox_id)
        .bind(event.gateway_id)
        .bind(event.kind.as_str())
        .bind(event.source)
        .bind(event.ts)
        .bind(redacted)
        .execute(scope.connection())
        .await?;
        written += result.rows_affected();
    }

    scope.commit().await?;
    Ok(written)
}

/// The node a token belongs to.
///
/// `token_ref` is a locator (`file://…`), never a value: the token itself lives
/// in a 0600 file on both sides (ADR 0005).
pub struct NodeIdentity {
    pub id: Uuid,
    pub name: String,
    pub token_ref: Option<String>,
}

/// Look a node up by name or id, for the ingest path.
///
/// `nodes` are global rows without an environment, so this query is not
/// tenant-scoped. It runs on the admin pool for the same reason ingest does: a
/// node reports for the environments whose sandboxes it runs.
pub async fn node_identity(
    pool: &sqlx::PgPool,
    reference: &str,
) -> Result<Option<NodeIdentity>, StoreError> {
    let row = sqlx::query(
        "SELECT id, name, node_token_ref
           FROM nodes
          WHERE name = $1 OR id::text = $1
          LIMIT 1",
    )
    .bind(reference)
    .fetch_optional(pool)
    .await?;

    let Some(row) = row else {
        return Ok(None);
    };
    Ok(Some(NodeIdentity {
        id: row.try_get("id")?,
        name: row.try_get("name")?,
        token_ref: row.try_get("node_token_ref")?,
    }))
}

/// Whether an environment exists.
///
/// Checked before a batch is stored so a node cannot attach events to a tenant
/// that does not exist; `environments` carries no row-level policy (the login
/// path needs it before a scope exists), so this is a plain read.
pub async fn environment_exists(
    pool: &sqlx::PgPool,
    environment_id: Uuid,
) -> Result<bool, StoreError> {
    let found: Option<Uuid> = sqlx::query_scalar("SELECT id FROM environments WHERE id = $1")
        .bind(environment_id)
        .fetch_optional(pool)
        .await?;
    Ok(found.is_some())
}

/// Which of these sandboxes belong to the environment.
///
/// The caller compares the answer with what the batch claimed. A node that names
/// a sandbox in another tenant is not a parsing mistake to be tolerated: it is
/// the one cross-tenant write this endpoint could perform.
pub async fn sandboxes_in_environment(
    scope: &mut TenantScope<'_>,
    sandbox_ids: &[Uuid],
) -> Result<Vec<Uuid>, StoreError> {
    if sandbox_ids.is_empty() {
        return Ok(Vec::new());
    }
    let rows = sqlx::query(
        "SELECT id
           FROM sandboxes
          WHERE environment_id = $1 AND id = ANY($2)",
    )
    .bind(scope.environment_id())
    .bind(sandbox_ids)
    .fetch_all(scope.connection())
    .await?;

    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        out.push(row.try_get("id")?);
    }
    Ok(out)
}

/// The largest MCP result stored. Arguments are bounded by the request body cap
/// (64 KiB); a result is what a server chose to send, so it needs its own cut.
pub const MAX_RESULT_BYTES: usize = 16_384;

/// One MCP call as the proxy saw it, ready to store.
///
/// The text fields are owned here because the ingest path builds these from a
/// request that has already been decoded into memory; borrowing would tie the
/// batch's lifetime to a struct that outlives it for no gain.
#[derive(Clone)]
pub struct NewToolCall {
    pub id: Uuid,
    pub sandbox_id: Option<Uuid>,
    pub ts: DateTime<Utc>,
    pub server_host: String,
    pub server_key: Option<String>,
    pub session_id: Option<String>,
    pub method: String,
    pub tool_name: Option<String>,
    pub arguments: Option<serde_json::Value>,
    pub result: Option<serde_json::Value>,
    pub is_error: bool,
    pub duration_ms: Option<i32>,
    pub request_id: Option<String>,
    pub event_id: Option<Uuid>,
}

/// Insert the MCP calls of one batch.
///
/// This lives here rather than in `mcp.rs` because it is the ingest path's
/// write, on the same event id and tenant as the traffic row it came from; the
/// registry aggregate owns the server catalogue, not the wire record. Redaction
/// and the result cap are applied here, so an MCP payload cannot reach the
/// database unredacted.
pub async fn insert_tool_calls(
    pool: &sqlx::PgPool,
    environment_id: Uuid,
    calls: &[NewToolCall],
) -> Result<u64, StoreError> {
    if calls.is_empty() {
        return Ok(0);
    }

    let mut scope = TenantScope::begin(pool, environment_id).await?;
    let mut written = 0u64;

    for call in calls {
        let arguments = call
            .arguments
            .as_ref()
            .map(redaction::redact_json);
        let result = call.result.as_ref().map(|value| {
            let redacted = redaction::redact_json(value);
            cap_result(redacted, MAX_RESULT_BYTES)
        });
        let inserted = sqlx::query(
            "INSERT INTO mcp_tool_calls
                (id, environment_id, sandbox_id, ts, server_host, server_key,
                 session_id, method, tool_name, arguments, result, is_error,
                 duration_ms, request_id, event_id)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15)
             ON CONFLICT (id, ts) DO NOTHING",
        )
        .bind(call.id)
        .bind(scope.environment_id())
        .bind(call.sandbox_id)
        .bind(call.ts)
        .bind(&call.server_host)
        .bind(call.server_key.as_deref())
        .bind(call.session_id.as_deref())
        .bind(&call.method)
        .bind(call.tool_name.as_deref())
        .bind(arguments)
        .bind(result)
        .bind(call.is_error)
        .bind(call.duration_ms)
        .bind(call.request_id.as_deref())
        .bind(call.event_id)
        .execute(scope.connection())
        .await?;
        written += inserted.rows_affected();
    }

    scope.commit().await?;
    Ok(written)
}

/// Replace a result that is too large with a labelled preview.
///
/// Cutting the JSON text would produce a document that no longer parses, so the
/// value is replaced by an object that says what happened and carries a readable
/// prefix. `truncated` is the field the console shows; a silent cut would look
/// like a server that returned less.
fn cap_result(value: serde_json::Value, max: usize) -> serde_json::Value {
    let text = match serde_json::to_string(&value) {
        Ok(text) => text,
        // A value that cannot be serialised cannot be stored as JSON either.
        Err(_) => return serde_json::json!({ "truncated": true, "bytes": 0 }),
    };
    if text.len() <= max {
        return value;
    }
    serde_json::json!({
        "truncated": true,
        "bytes": text.len(),
        "preview": preview(&text, max),
    })
}

/// The longest prefix of `text` that ends on a character boundary below `max`.
fn preview(text: &str, max: usize) -> &str {
    if text.len() <= max {
        return text;
    }
    let mut end = 0usize;
    for (index, _) in text.char_indices() {
        if index > max {
            break;
        }
        end = index;
    }
    &text[..end]
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

/// The DNS names one sandbox asked for in the last `since_minutes`, grouped by
/// name and decision.
///
/// A refused lookup produces no network event: the connection was never made, so
/// the traffic rows cannot show it. This query is therefore the only evidence
/// that the stack needed a name and could not resolve it. The grouping happens
/// in SQL so a hostname asked for a thousand times is one row with a count, not
/// a thousand rows the caller has to fold.
pub async fn recent_dns(
    scope: &mut TenantScope<'_>,
    sandbox_id: Uuid,
    since_minutes: i64,
) -> Result<Vec<(String, String, i64)>, StoreError> {
    let since = Utc::now() - chrono::Duration::minutes(since_minutes.clamp(1, 24 * 60));
    let rows = sqlx::query(
        "SELECT COALESCE(NULLIF(detail->>'hostname', ''), NULLIF(detail->>'host', '')) AS host,
                COALESCE(detail->>'decision', '') AS decision,
                COUNT(*) AS n
           FROM telemetry_events
          WHERE environment_id = $1
            AND sandbox_id = $2
            AND kind = 'dns'
            AND ts >= $3
            AND COALESCE(NULLIF(detail->>'hostname', ''), NULLIF(detail->>'host', '')) IS NOT NULL
            AND COALESCE(detail->>'decision', '') <> ''
          GROUP BY host, decision
          ORDER BY n DESC, host ASC
          LIMIT 50",
    )
    .bind(scope.environment_id())
    .bind(sandbox_id)
    .bind(since)
    .fetch_all(scope.connection())
    .await?;

    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        out.push((
            row.try_get::<String, _>("host")?,
            row.try_get::<String, _>("decision")?,
            row.try_get::<i64, _>("n")?,
        ));
    }
    Ok(out)
}

/// Record an inference request in its own table, so the console can sort on
/// typed columns without parsing JSON on a hot path.
///
/// Scoped like `insert`, for the same reason: `inference_requests` has a
/// row-level policy and the application role is not its owner.
pub async fn insert_inference(
    pool: &sqlx::PgPool,
    event: &crate::domain::telemetry::InferenceEvent,
) -> Result<(), StoreError> {
    let mut scope = TenantScope::begin(pool, event.environment_id).await?;
    sqlx::query(
        "INSERT INTO inference_requests
            (id, environment_id, sandbox_id, provider_key, endpoint, model_key,
             method, path, status_code, latency_ms, tokens_in, tokens_out,
             bytes_in, bytes_out, decision, correlation_id, ts)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17)",
    )
    .bind(event.id)
    .bind(scope.environment_id())
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
    .execute(scope.connection())
    .await?;
    scope.commit().await?;
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
