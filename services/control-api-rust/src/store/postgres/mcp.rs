//! MCP servers, a sandbox's attachment to them, and the calls the proxy saw.
//!
//! Three tables (`0023_mcp.sql` for the first two, `0022_egress.sql` for the
//! third):
//!
//! * `mcp_registry` — the server definitions every sandbox can be given, with
//!   the transport that decides whether the proxy ever sees a call. Global
//!   catalog data, like `agent_packs`; there is no `environment_id` column.
//! * `sandbox_mcp_servers` — which sandbox has which server, its status, and
//!   the tool list discovery returned. Tenant-scoped.
//! * `mcp_tool_calls` — one row per JSON-RPC call on the wire, partitioned by
//!   day, 7 d retention. Written by the telemetry ingest, read by the traffic
//!   API, so the insert takes the admin pool the way `telemetry::insert` does.
//!
//! Credentials are not here and never will be: a remote server's token lives in
//! the agent's own home directory inside the container, and the registry holds
//! the *names* of the environment variables it needs.

use sqlx::Row;
use uuid::Uuid;

use super::super::TenantScope;
use crate::domain::mcp::{
    McpAuth, McpServer, McpStatus, McpToolCall, McpTransport, SandboxMcpServer,
};
use crate::error::StoreError;
use crate::security::redaction;

/// Columns of `mcp_registry`, named so the mapping cannot drift silently.
const SERVER_COLUMNS: &str = "key, name, description, publisher, homepage, transport, endpoint, \
     command, args, auth_type, env_slots, packs, enabled";

/// Columns of `sandbox_mcp_servers`.
const ATTACHMENT_COLUMNS: &str = "id, sandbox_id, server_key, enabled, status, tools, \
     last_session_id, last_seen_at, last_error";

/// Columns of `mcp_tool_calls`.
const CALL_COLUMNS: &str = "id, sandbox_id, ts, server_host, server_key, session_id, method, \
     tool_name, arguments, result, is_error, duration_ms, request_id";

/// The largest result body kept in a row, before it is cut.
///
/// The proxy buffer is 64 KiB; a tool result can legitimately be larger, and a
/// JSONB column that grows without a limit is a way to fill a disk with one
/// loop. 16 KiB keeps a readable answer and bounds the row.
const MAX_RESULT_BYTES: usize = 16 * 1024;

/// The largest error text kept on an attachment.
const MAX_ERROR_CHARS: usize = 500;

fn row_to_server(row: &sqlx::postgres::PgRow) -> Result<McpServer, StoreError> {
    let transport_raw: String = row.try_get("transport")?;
    let auth_raw: String = row.try_get("auth_type")?;
    Ok(McpServer {
        key: row.try_get("key")?,
        name: row.try_get("name")?,
        description: row.try_get("description")?,
        publisher: row.try_get("publisher")?,
        homepage: row.try_get("homepage")?,
        // The CHECK constraint makes an unknown value unreachable. If it happens
        // anyway, `stdio` is the conservative answer: it says "not visible to
        // the proxy" rather than claiming coverage that does not exist.
        transport: McpTransport::parse(&transport_raw).unwrap_or(McpTransport::Stdio),
        endpoint: row.try_get("endpoint")?,
        command: row.try_get("command")?,
        args: row.try_get("args")?,
        auth_type: McpAuth::parse(&auth_raw).unwrap_or(McpAuth::None),
        env_slots: row.try_get("env_slots")?,
        packs: row.try_get("packs")?,
        enabled: row.try_get("enabled")?,
    })
}

fn row_to_attachment(row: &sqlx::postgres::PgRow) -> Result<SandboxMcpServer, StoreError> {
    let status_raw: String = row.try_get("status")?;
    Ok(SandboxMcpServer {
        id: row.try_get("id")?,
        sandbox_id: row.try_get("sandbox_id")?,
        server_key: row.try_get("server_key")?,
        enabled: row.try_get("enabled")?,
        status: McpStatus::parse(&status_raw).unwrap_or(McpStatus::Unknown),
        tools: row.try_get("tools")?,
        last_session_id: row.try_get("last_session_id")?,
        last_seen_at: row.try_get("last_seen_at")?,
        last_error: row.try_get("last_error")?,
    })
}

fn row_to_call(row: &sqlx::postgres::PgRow) -> Result<McpToolCall, StoreError> {
    Ok(McpToolCall {
        id: row.try_get("id")?,
        sandbox_id: row.try_get("sandbox_id")?,
        ts: row.try_get("ts")?,
        server_host: row.try_get("server_host")?,
        server_key: row.try_get("server_key")?,
        session_id: row.try_get("session_id")?,
        method: row.try_get("method")?,
        tool_name: row.try_get("tool_name")?,
        arguments: row.try_get("arguments")?,
        result: row.try_get("result")?,
        is_error: row.try_get("is_error")?,
        duration_ms: row.try_get("duration_ms")?,
        request_id: row.try_get("request_id")?,
    })
}

// ---------------------------------------------------------------------------
// The registry
// ---------------------------------------------------------------------------

/// Every server in the registry, by name.
pub async fn list_servers(scope: &mut TenantScope<'_>) -> Result<Vec<McpServer>, StoreError> {
    let sql = format!("SELECT {SERVER_COLUMNS} FROM mcp_registry ORDER BY name");
    let rows = sqlx::query(&sql).fetch_all(scope.connection()).await?;
    rows.iter().map(row_to_server).collect()
}

/// One server by key, enabled or not.
pub async fn get_server(
    scope: &mut TenantScope<'_>,
    key: &str,
) -> Result<McpServer, StoreError> {
    let sql = format!("SELECT {SERVER_COLUMNS} FROM mcp_registry WHERE key = $1");
    let row = sqlx::query(&sql)
        .bind(key)
        .fetch_optional(scope.connection())
        .await?
        .ok_or(StoreError::NotFound)?;
    row_to_server(&row)
}

/// The fields a create or a replace writes.
///
/// Separate from [`McpServer`] because a caller supplies no timestamps and the
/// key is validated before it arrives; the domain type is what a response
/// carries.
pub struct ServerDefinition<'a> {
    pub key: &'a str,
    pub name: &'a str,
    pub description: Option<&'a str>,
    pub publisher: Option<&'a str>,
    pub homepage: Option<&'a str>,
    pub transport: McpTransport,
    pub endpoint: Option<&'a str>,
    pub command: Option<&'a str>,
    pub args: &'a [String],
    pub auth_type: McpAuth,
    pub env_slots: &'a [String],
    pub packs: &'a [String],
    pub enabled: bool,
}

/// Insert a definition.
///
/// A taken key is a [`StoreError::UniqueViolation`], which the service turns
/// into a 409 naming the key rather than a 500 from the constraint.
pub async fn insert_server(
    scope: &mut TenantScope<'_>,
    definition: &ServerDefinition<'_>,
) -> Result<McpServer, StoreError> {
    let sql = format!(
        "INSERT INTO mcp_registry
            (key, name, description, publisher, homepage, transport, endpoint, command,
             args, auth_type, env_slots, packs, enabled)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
         RETURNING {SERVER_COLUMNS}"
    );
    let row = sqlx::query(&sql)
        .bind(definition.key)
        .bind(definition.name)
        .bind(definition.description)
        .bind(definition.publisher)
        .bind(definition.homepage)
        .bind(definition.transport.as_str())
        .bind(definition.endpoint)
        .bind(definition.command)
        .bind(definition.args)
        .bind(definition.auth_type.as_str())
        .bind(definition.env_slots)
        .bind(definition.packs)
        .bind(definition.enabled)
        .fetch_one(scope.connection())
        .await?;
    row_to_server(&row)
}

/// Replace a definition in place.
///
/// `key` is not updatable: an attachment refers to it, and renaming a server by
/// rewriting the key would either orphan rows or silently point them at a
/// different server.
pub async fn replace_server(
    scope: &mut TenantScope<'_>,
    definition: &ServerDefinition<'_>,
) -> Result<McpServer, StoreError> {
    let sql = format!(
        "UPDATE mcp_registry
            SET name = $2, description = $3, publisher = $4, homepage = $5,
                transport = $6, endpoint = $7, command = $8, args = $9,
                auth_type = $10, env_slots = $11, packs = $12, enabled = $13,
                updated_at = now()
          WHERE key = $1
      RETURNING {SERVER_COLUMNS}"
    );
    let row = sqlx::query(&sql)
        .bind(definition.key)
        .bind(definition.name)
        .bind(definition.description)
        .bind(definition.publisher)
        .bind(definition.homepage)
        .bind(definition.transport.as_str())
        .bind(definition.endpoint)
        .bind(definition.command)
        .bind(definition.args)
        .bind(definition.auth_type.as_str())
        .bind(definition.env_slots)
        .bind(definition.packs)
        .bind(definition.enabled)
        .fetch_optional(scope.connection())
        .await?
        .ok_or(StoreError::NotFound)?;
    row_to_server(&row)
}

/// How many sandboxes use a server.
///
/// Asked before a delete so the refusal names the number instead of surfacing a
/// foreign-key violation. The constraint is still there for the race.
pub async fn count_attachments(
    scope: &mut TenantScope<'_>,
    key: &str,
) -> Result<i64, StoreError> {
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM sandbox_mcp_servers WHERE server_key = $1")
        .bind(key)
        .fetch_one(scope.connection())
        .await?;
    Ok(count)
}

/// Delete a definition. Returns `NotFound` when there was nothing to delete.
pub async fn delete_server(scope: &mut TenantScope<'_>, key: &str) -> Result<(), StoreError> {
    let result = sqlx::query("DELETE FROM mcp_registry WHERE key = $1")
        .bind(key)
        .execute(scope.connection())
        .await?;
    if result.rows_affected() == 0 {
        return Err(StoreError::NotFound);
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// A sandbox's attachments
// ---------------------------------------------------------------------------

/// Every server attached to one sandbox.
pub async fn list_attachments(
    scope: &mut TenantScope<'_>,
    sandbox_id: Uuid,
) -> Result<Vec<SandboxMcpServer>, StoreError> {
    let sql = format!(
        "SELECT {ATTACHMENT_COLUMNS} FROM sandbox_mcp_servers
          WHERE sandbox_id = $1 AND environment_id = $2
          ORDER BY server_key"
    );
    let rows = sqlx::query(&sql)
        .bind(sandbox_id)
        .bind(scope.environment_id())
        .fetch_all(scope.connection())
        .await?;
    rows.iter().map(row_to_attachment).collect()
}

/// One attachment, or `NotFound`.
pub async fn get_attachment(
    scope: &mut TenantScope<'_>,
    sandbox_id: Uuid,
    key: &str,
) -> Result<SandboxMcpServer, StoreError> {
    let sql = format!(
        "SELECT {ATTACHMENT_COLUMNS} FROM sandbox_mcp_servers
          WHERE sandbox_id = $1 AND server_key = $2 AND environment_id = $3"
    );
    let row = sqlx::query(&sql)
        .bind(sandbox_id)
        .bind(key)
        .bind(scope.environment_id())
        .fetch_optional(scope.connection())
        .await?
        .ok_or(StoreError::NotFound)?;
    row_to_attachment(&row)
}

/// Attach a registry server to a sandbox.
///
/// A second attach of the same pair is a unique violation and becomes a 409;
/// attaching is not idempotent on purpose, because the caller may have meant a
/// different server and a silent success would hide the mistake.
pub async fn insert_attachment(
    scope: &mut TenantScope<'_>,
    sandbox_id: Uuid,
    key: &str,
    enabled: bool,
) -> Result<SandboxMcpServer, StoreError> {
    let sql = format!(
        "INSERT INTO sandbox_mcp_servers (environment_id, sandbox_id, server_key, enabled)
         VALUES ($1, $2, $3, $4)
         RETURNING {ATTACHMENT_COLUMNS}"
    );
    let row = sqlx::query(&sql)
        .bind(scope.environment_id())
        .bind(sandbox_id)
        .bind(key)
        .bind(enabled)
        .fetch_one(scope.connection())
        .await?;
    row_to_attachment(&row)
}

/// Detach a server from a sandbox.
pub async fn delete_attachment(
    scope: &mut TenantScope<'_>,
    sandbox_id: Uuid,
    key: &str,
) -> Result<(), StoreError> {
    let result = sqlx::query(
        "DELETE FROM sandbox_mcp_servers
          WHERE sandbox_id = $1 AND server_key = $2 AND environment_id = $3",
    )
    .bind(sandbox_id)
    .bind(key)
    .bind(scope.environment_id())
    .execute(scope.connection())
    .await?;
    if result.rows_affected() == 0 {
        return Err(StoreError::NotFound);
    }
    Ok(())
}

/// Record a successful discovery: the tool list, the session, and the fact that
/// the server answered.
pub async fn mark_discovered(
    scope: &mut TenantScope<'_>,
    attachment_id: Uuid,
    tools: &serde_json::Value,
    session_id: Option<&str>,
) -> Result<SandboxMcpServer, StoreError> {
    let sql = format!(
        "UPDATE sandbox_mcp_servers
            SET status = 'connected', tools = $2, last_session_id = $3,
                last_seen_at = now(), last_error = NULL, updated_at = now()
          WHERE id = $1 AND environment_id = $4
      RETURNING {ATTACHMENT_COLUMNS}"
    );
    let row = sqlx::query(&sql)
        .bind(attachment_id)
        .bind(tools)
        .bind(session_id)
        .bind(scope.environment_id())
        .fetch_optional(scope.connection())
        .await?
        .ok_or(StoreError::NotFound)?;
    row_to_attachment(&row)
}

/// Record a failed discovery.
///
/// The status becomes `error` and the message is kept, because a server that
/// cannot be reached is a fact the console has to show. The reason is redacted
/// before it is stored: a proxy error can echo a header.
pub async fn mark_discovery_failed(
    scope: &mut TenantScope<'_>,
    attachment_id: Uuid,
    reason: &str,
) -> Result<SandboxMcpServer, StoreError> {
    let redacted = redaction::redact(reason);
    let cut: String = redacted.chars().take(MAX_ERROR_CHARS).collect();
    let sql = format!(
        "UPDATE sandbox_mcp_servers
            SET status = 'error', last_error = $2, last_seen_at = now(), updated_at = now()
          WHERE id = $1 AND environment_id = $3
      RETURNING {ATTACHMENT_COLUMNS}"
    );
    let row = sqlx::query(&sql)
        .bind(attachment_id)
        .bind(cut)
        .bind(scope.environment_id())
        .fetch_optional(scope.connection())
        .await?
        .ok_or(StoreError::NotFound)?;
    row_to_attachment(&row)
}

// ---------------------------------------------------------------------------
// Calls the proxy saw
// ---------------------------------------------------------------------------

/// One JSON-RPC call to insert.
///
/// `environment_id` is carried by the caller because this write is the ingest
/// path: the proxy reports for many environments, exactly like telemetry.
pub struct NewToolCall {
    pub environment_id: Uuid,
    pub sandbox_id: Option<Uuid>,
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
}

/// Insert one call.
///
/// The arguments and the result are redacted here, not at the call site: there
/// is one place that could forget, and it does not. A result over 16 KiB is
/// replaced by its size rather than truncated mid-JSON, because half a document
/// in a JSONB column reads as a corrupt answer.
pub async fn insert_call(pool: &sqlx::PgPool, call: NewToolCall) -> Result<Uuid, StoreError> {
    let arguments = call.arguments.as_ref().map(redaction::redact_json);
    let result = call.result.as_ref().map(cap_result);
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO mcp_tool_calls
            (environment_id, sandbox_id, server_host, server_key, session_id, method,
             tool_name, arguments, result, is_error, duration_ms, request_id)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
         RETURNING id",
    )
    .bind(call.environment_id)
    .bind(call.sandbox_id)
    .bind(&call.server_host)
    .bind(&call.server_key)
    .bind(&call.session_id)
    .bind(&call.method)
    .bind(&call.tool_name)
    .bind(arguments)
    .bind(result)
    .bind(call.is_error)
    .bind(call.duration_ms)
    .bind(&call.request_id)
    .fetch_one(pool)
    .await?;
    Ok(id)
}

/// Redact a result and bound its size.
fn cap_result(result: &serde_json::Value) -> serde_json::Value {
    let redacted = redaction::redact_json(result);
    let size = serde_json::to_vec(&redacted).map(|bytes| bytes.len()).unwrap_or(0);
    if size > MAX_RESULT_BYTES {
        return serde_json::json!({
            "truncated": true,
            "bytes": size,
            "reason": format!("result exceeded {MAX_RESULT_BYTES} bytes"),
        });
    }
    redacted
}

/// The most recent calls for one sandbox.
///
/// A limited, time-ordered read: `mcp_tool_calls` is partitioned by day, so a
/// query without an order would scan every partition for a page of rows.
pub async fn list_calls(
    scope: &mut TenantScope<'_>,
    sandbox_id: Uuid,
    limit: i64,
) -> Result<Vec<McpToolCall>, StoreError> {
    let sql = format!(
        "SELECT {CALL_COLUMNS} FROM mcp_tool_calls
          WHERE environment_id = $1 AND sandbox_id = $2
          ORDER BY ts DESC, id DESC
          LIMIT $3"
    );
    let rows = sqlx::query(&sql)
        .bind(scope.environment_id())
        .bind(sandbox_id)
        .bind(limit)
        .fetch_all(scope.connection())
        .await?;
    rows.iter().map(row_to_call).collect()
}
