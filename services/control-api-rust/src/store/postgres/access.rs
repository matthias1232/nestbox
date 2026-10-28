//! How an operator reaches a sandbox: SSH keys, published ports, sessions.
//!
//! Three tables (`0021_sandbox_access.sql`, `0024_agent_ports.sql`):
//!
//! * `sandbox_ssh_keys` — the public half, a `file://` locator for the private
//!   half, and the fingerprint. The private key is a 0600 file on the node and
//!   never a column (ADR 0005);
//! * `forwards` + `node_port_pool` — a published port is a `forwards` row, and
//!   the claim on the node is a `node_port_pool` row whose primary key
//!   `(node_id, port)` is what makes two sandboxes unable to share a port;
//! * `sandbox_sessions` — who opened a terminal or an SSH session, when, and for
//!   how long. Never the content.
//!
//! Port claiming is a single `INSERT ... ON CONFLICT DO NOTHING RETURNING`: the
//! unique constraint decides, not a preceding `SELECT`. A `SELECT` of the
//! occupied ports only shortens the search; it is never trusted.

use sqlx::Row;
use uuid::Uuid;

use super::super::TenantScope;
use crate::domain::access::{
    ForwardPurpose, NodePortPool, PublishedPort, SandboxSession, SessionKind, SshKey,
};
use crate::error::StoreError;

const KEY_COLUMNS: &str =
    "id, sandbox_id, public_key, private_key_ref, fingerprint, downloaded_at, created_at, rotated_at";

const SESSION_COLUMNS: &str = "id, sandbox_id, user_id, kind, opened_at, closed_at, \
     close_reason, bytes_in, bytes_out";

fn row_to_key(row: &sqlx::postgres::PgRow) -> Result<SshKey, StoreError> {
    Ok(SshKey {
        id: row.try_get("id")?,
        sandbox_id: row.try_get("sandbox_id")?,
        public_key: row.try_get("public_key")?,
        private_key_ref: row.try_get("private_key_ref")?,
        fingerprint: row.try_get("fingerprint")?,
        downloaded_at: row.try_get("downloaded_at")?,
        created_at: row.try_get("created_at")?,
        rotated_at: row.try_get("rotated_at")?,
    })
}

fn row_to_session(row: &sqlx::postgres::PgRow) -> Result<SandboxSession, StoreError> {
    let kind_raw: String = row.try_get("kind")?;
    Ok(SandboxSession {
        id: row.try_get("id")?,
        sandbox_id: row.try_get("sandbox_id")?,
        user_id: row.try_get("user_id")?,
        kind: SessionKind::parse(&kind_raw).ok_or_else(|| {
            StoreError::UnexpectedData(format!("unknown session kind '{kind_raw}'"))
        })?,
        opened_at: row.try_get("opened_at")?,
        closed_at: row.try_get("closed_at")?,
        close_reason: row.try_get("close_reason")?,
        bytes_in: row.try_get("bytes_in")?,
        bytes_out: row.try_get("bytes_out")?,
    })
}

// ---------------------------------------------------------------------------
// SSH keys
// ---------------------------------------------------------------------------

/// What a first insert carries. The locator is written by the service from the
/// secret directory; it is never taken from a request.
pub struct NewSshKey<'a> {
    pub sandbox_id: Uuid,
    pub public_key: &'a str,
    pub private_key_ref: &'a str,
    pub fingerprint: &'a str,
}

/// Insert the key pair for a sandbox.
///
/// `UNIQUE (sandbox_id)` refuses a second pair, so a retried create cannot
/// leave two usable keys.
pub async fn insert_key(
    scope: &mut TenantScope<'_>,
    new: NewSshKey<'_>,
) -> Result<SshKey, StoreError> {
    let sql = format!(
        "INSERT INTO sandbox_ssh_keys
            (environment_id, sandbox_id, public_key, private_key_ref, fingerprint)
         VALUES ($1, $2, $3, $4, $5)
         RETURNING {KEY_COLUMNS}"
    );
    let row = sqlx::query(&sql)
        .bind(scope.environment_id())
        .bind(new.sandbox_id)
        .bind(new.public_key)
        .bind(new.private_key_ref)
        .bind(new.fingerprint)
        .fetch_one(scope.connection())
        .await?;
    row_to_key(&row)
}

/// The stored key pair of one sandbox, or `NotFound`.
pub async fn get_key(
    scope: &mut TenantScope<'_>,
    sandbox_id: Uuid,
) -> Result<SshKey, StoreError> {
    let sql = format!(
        "SELECT {KEY_COLUMNS} FROM sandbox_ssh_keys
          WHERE environment_id = $1 AND sandbox_id = $2"
    );
    let row = sqlx::query(&sql)
        .bind(scope.environment_id())
        .bind(sandbox_id)
        .fetch_optional(scope.connection())
        .await?
        .ok_or(StoreError::NotFound)?;
    row_to_key(&row)
}

/// Replace the pair of a sandbox and clear the download mark.
///
/// `rotated_at` and the cleared `downloaded_at` are written in the same
/// statement, so there is no moment in which the row claims a key has never
/// been downloaded while still pointing at the old fingerprint.
pub async fn replace_key(
    scope: &mut TenantScope<'_>,
    sandbox_id: Uuid,
    public_key: &str,
    private_key_ref: &str,
    fingerprint: &str,
) -> Result<SshKey, StoreError> {
    let sql = format!(
        "UPDATE sandbox_ssh_keys
            SET public_key = $3, private_key_ref = $4, fingerprint = $5,
                downloaded_at = NULL, downloaded_by = NULL, rotated_at = now()
          WHERE environment_id = $1 AND sandbox_id = $2
         RETURNING {KEY_COLUMNS}"
    );
    let row = sqlx::query(&sql)
        .bind(scope.environment_id())
        .bind(sandbox_id)
        .bind(public_key)
        .bind(private_key_ref)
        .bind(fingerprint)
        .fetch_optional(scope.connection())
        .await?
        .ok_or(StoreError::NotFound)?;
    row_to_key(&row)
}

/// Stamp the one-time download.
///
/// The `AND downloaded_at IS NULL` in the `WHERE` clause is the whole
/// guarantee: two concurrent downloads both run this statement, exactly one
/// updates a row, and the other learns it lost. Reading the flag and then
/// deciding would let both through.
pub async fn stamp_download(
    scope: &mut TenantScope<'_>,
    sandbox_id: Uuid,
    user_id: Uuid,
) -> Result<bool, StoreError> {
    let result = sqlx::query(
        "UPDATE sandbox_ssh_keys
            SET downloaded_at = now(), downloaded_by = $3
          WHERE environment_id = $1 AND sandbox_id = $2 AND downloaded_at IS NULL",
    )
    .bind(scope.environment_id())
    .bind(sandbox_id)
    .bind(user_id)
    .execute(scope.connection())
    .await?;
    Ok(result.rows_affected() == 1)
}

/// Remove the key row, for a sandbox whose rotation could not be completed.
pub async fn delete_key(
    scope: &mut TenantScope<'_>,
    sandbox_id: Uuid,
) -> Result<(), StoreError> {
    sqlx::query("DELETE FROM sandbox_ssh_keys WHERE environment_id = $1 AND sandbox_id = $2")
        .bind(scope.environment_id())
        .bind(sandbox_id)
        .execute(scope.connection())
        .await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Port pool
// ---------------------------------------------------------------------------

/// The publication range and public address of one node.
/// The name and the two ways a client can address a node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeEndpoint {
    pub name: String,
    pub address: Option<String>,
    pub hostname: Option<String>,
}

/// What a client may dial on one node.
pub async fn node_endpoint(
    scope: &mut TenantScope<'_>,
    node_id: Uuid,
) -> Result<NodeEndpoint, StoreError> {
    let row = sqlx::query("SELECT name, public_address, public_hostname FROM nodes WHERE id = $1")
        .bind(node_id)
        .fetch_optional(scope.connection())
        .await?
        .ok_or(StoreError::NotFound)?;
    Ok(NodeEndpoint {
        name: row.try_get("name")?,
        address: row.try_get("public_address")?,
        hostname: row.try_get("public_hostname")?,
    })
}

/// Set the address and the hostname of a node. `None` clears one.
pub async fn set_node_endpoint(
    scope: &mut TenantScope<'_>,
    node_id: Uuid,
    address: Option<&str>,
    hostname: Option<&str>,
) -> Result<(), StoreError> {
    let affected = sqlx::query(
        "UPDATE nodes SET public_address = $2, public_hostname = $3, updated_at = now()
          WHERE id = $1",
    )
    .bind(node_id)
    .bind(address)
    .bind(hostname)
    .execute(scope.connection())
    .await?
    .rows_affected();
    if affected == 0 {
        Err(StoreError::NotFound)
    } else {
        Ok(())
    }
}

pub async fn port_pool(
    scope: &mut TenantScope<'_>,
    node_id: Uuid,
) -> Result<NodePortPool, StoreError> {
    let row = sqlx::query(
        "SELECT id, public_address, port_pool_start, port_pool_end FROM nodes WHERE id = $1",
    )
    .bind(node_id)
    .fetch_optional(scope.connection())
    .await?
    .ok_or(StoreError::NotFound)?;
    Ok(NodePortPool {
        node_id: row.try_get("id")?,
        public_address: row.try_get("public_address")?,
        start: row.try_get("port_pool_start")?,
        end: row.try_get("port_pool_end")?,
    })
}

/// The ports this node has already handed out.
///
/// A hint for the claim loop, never a decision: between this read and the
/// insert another sandbox may claim the same port, and the constraint is what
/// settles that.
pub async fn claimed_ports(
    scope: &mut TenantScope<'_>,
    node_id: Uuid,
) -> Result<Vec<i32>, StoreError> {
    let rows = sqlx::query("SELECT port FROM node_port_pool WHERE node_id = $1 ORDER BY port")
        .bind(node_id)
        .fetch_all(scope.connection())
        .await?;
    rows.iter().map(|row| row.try_get("port").map_err(StoreError::from)).collect()
}

/// Claim one port on a node, or report that someone else has it.
///
/// Returns `true` when this call inserted the row. `DO NOTHING` plus the
/// primary key makes the database the arbiter, so two control planes sharing a
/// node cannot both win.
pub async fn claim_port(
    scope: &mut TenantScope<'_>,
    node_id: Uuid,
    port: i32,
    sandbox_id: Uuid,
    purpose: ForwardPurpose,
    target_port: i32,
) -> Result<bool, StoreError> {
    let row = sqlx::query(
        "INSERT INTO node_port_pool (node_id, port, sandbox_id, purpose, target_port)
         VALUES ($1, $2, $3, $4, $5)
         ON CONFLICT (node_id, port) DO NOTHING
         RETURNING port",
    )
    .bind(node_id)
    .bind(port)
    .bind(sandbox_id)
    .bind(purpose.as_str())
    .bind(target_port)
    .fetch_optional(scope.connection())
    .await?;
    Ok(row.is_some())
}

/// What an insert carries: the `forwards` row that makes a claim visible.
pub struct NewForward<'a> {
    pub sandbox_id: Uuid,
    pub local_port: i32,
    pub bind_address: &'a str,
    pub target_port: i32,
    pub purpose: ForwardPurpose,
    pub label: Option<&'a str>,
}

/// Record a forward. It starts `pending`: claimed and known, not yet published
/// by the node's runtime.
pub async fn insert_forward(
    scope: &mut TenantScope<'_>,
    new: NewForward<'_>,
) -> Result<Uuid, StoreError> {
    let row = sqlx::query(
        "INSERT INTO forwards
            (environment_id, sandbox_id, local_port, bind_address, target_port, state, purpose, label)
         VALUES ($1, $2, $3, $4, $5, 'pending', $6, $7)
         RETURNING id",
    )
    .bind(scope.environment_id())
    .bind(new.sandbox_id)
    .bind(new.local_port)
    .bind(new.bind_address)
    .bind(new.target_port)
    .bind(new.purpose.as_str())
    .bind(new.label)
    .fetch_one(scope.connection())
    .await?;
    row.try_get("id").map_err(StoreError::from)
}

/// Point a claimed pool row at its forward.
pub async fn link_forward(
    scope: &mut TenantScope<'_>,
    node_id: Uuid,
    port: i32,
    sandbox_id: Uuid,
    forward_id: Uuid,
) -> Result<(), StoreError> {
    sqlx::query(
        "UPDATE node_port_pool SET forward_id = $4
          WHERE node_id = $1 AND port = $2 AND sandbox_id = $3",
    )
    .bind(node_id)
    .bind(port)
    .bind(sandbox_id)
    .bind(forward_id)
    .execute(scope.connection())
    .await?;
    Ok(())
}

/// Every port published for one sandbox, as the Access table shows it.
pub async fn list_forwards(
    scope: &mut TenantScope<'_>,
    sandbox_id: Uuid,
) -> Result<Vec<PublishedPort>, StoreError> {
    let rows = sqlx::query(
        "SELECT f.id, f.sandbox_id, s.node_id, n.public_address, f.local_port,
                f.target_port, f.purpose, f.label, f.state
           FROM forwards f
           JOIN sandboxes s ON s.id = f.sandbox_id
           LEFT JOIN nodes n ON n.id = s.node_id
          WHERE f.environment_id = $1 AND f.sandbox_id = $2
          ORDER BY f.local_port",
    )
    .bind(scope.environment_id())
    .bind(sandbox_id)
    .fetch_all(scope.connection())
    .await?;

    let mut ports = Vec::with_capacity(rows.len());
    for row in &rows {
        let purpose_raw: String = row.try_get("purpose")?;
        ports.push(PublishedPort {
            forward_id: row.try_get("id")?,
            sandbox_id: row.try_get("sandbox_id")?,
            node_id: row.try_get("node_id")?,
            public_address: row.try_get("public_address")?,
            public_port: row.try_get("local_port")?,
            target_port: row.try_get("target_port")?,
            purpose: ForwardPurpose::parse(&purpose_raw).unwrap_or(ForwardPurpose::Custom),
            label: row.try_get("label")?,
            state: row.try_get("state")?,
        });
    }
    Ok(ports)
}

/// Withdraw a forward and give its port back to the pool.
///
/// The pool row is deleted first: `node_port_pool.forward_id` is
/// `ON DELETE SET NULL`, so deleting the forward first would leave a claimed
/// port with no way to find it again. Both statements run in the caller's
/// transaction, so a failure between them rolls back.
pub async fn delete_forward(
    scope: &mut TenantScope<'_>,
    sandbox_id: Uuid,
    forward_id: Uuid,
) -> Result<(), StoreError> {
    sqlx::query(
        "DELETE FROM node_port_pool
          WHERE forward_id = $1 AND sandbox_id = $2
            AND sandbox_id IN (SELECT id FROM sandboxes WHERE environment_id = $3)",
    )
    .bind(forward_id)
    .bind(sandbox_id)
    .bind(scope.environment_id())
    .execute(scope.connection())
    .await?;

    let result = sqlx::query(
        "DELETE FROM forwards WHERE environment_id = $1 AND sandbox_id = $2 AND id = $3",
    )
    .bind(scope.environment_id())
    .bind(sandbox_id)
    .bind(forward_id)
    .execute(scope.connection())
    .await?;
    if result.rows_affected() == 0 {
        return Err(StoreError::NotFound);
    }
    Ok(())
}

/// The container ports a pack listens on.
///
/// `agent_packs` is global data with no tenant column, and the key comes from a
/// sandbox row the service already read inside its scope.
pub async fn pack_ports(
    scope: &mut TenantScope<'_>,
    pack_key: &str,
) -> Result<Vec<i32>, StoreError> {
    let ports: Option<Vec<i32>> =
        sqlx::query_scalar("SELECT ports FROM agent_packs WHERE key = $1")
            .bind(pack_key)
            .fetch_optional(scope.connection())
            .await?;
    Ok(ports.unwrap_or_default())
}

// ---------------------------------------------------------------------------
// Sessions
// ---------------------------------------------------------------------------

/// What opening a session records.
pub struct NewSession<'a> {
    pub sandbox_id: Uuid,
    pub user_id: Option<Uuid>,
    pub kind: SessionKind,
    pub close_reason: Option<&'a str>,
}

/// Open a session row. The terminal package calls this when the socket opens.
pub async fn open_session(
    scope: &mut TenantScope<'_>,
    new: NewSession<'_>,
) -> Result<Uuid, StoreError> {
    let row = sqlx::query(
        "INSERT INTO sandbox_sessions
            (environment_id, sandbox_id, user_id, kind, close_reason)
         VALUES ($1, $2, $3, $4, $5)
         RETURNING id",
    )
    .bind(scope.environment_id())
    .bind(new.sandbox_id)
    .bind(new.user_id)
    .bind(new.kind.as_str())
    .bind(new.close_reason)
    .fetch_one(scope.connection())
    .await?;
    row.try_get("id").map_err(StoreError::from)
}

/// Close a session row with its reason and byte counters.
pub async fn close_session(
    scope: &mut TenantScope<'_>,
    session_id: Uuid,
    reason: &str,
    bytes_in: i64,
    bytes_out: i64,
) -> Result<(), StoreError> {
    sqlx::query(
        "UPDATE sandbox_sessions
            SET closed_at = now(), close_reason = $3, bytes_in = $4, bytes_out = $5
          WHERE environment_id = $1 AND id = $2 AND closed_at IS NULL",
    )
    .bind(scope.environment_id())
    .bind(session_id)
    .bind(reason)
    .bind(bytes_in)
    .bind(bytes_out)
    .execute(scope.connection())
    .await?;
    Ok(())
}

/// The sessions of one sandbox, newest first. Content is not a column.
pub async fn list_sessions(
    scope: &mut TenantScope<'_>,
    sandbox_id: Uuid,
    limit: i64,
) -> Result<Vec<SandboxSession>, StoreError> {
    let sql = format!(
        "SELECT {SESSION_COLUMNS} FROM sandbox_sessions
          WHERE environment_id = $1 AND sandbox_id = $2
          ORDER BY opened_at DESC
          LIMIT $3"
    );
    let rows = sqlx::query(&sql)
        .bind(scope.environment_id())
        .bind(sandbox_id)
        .bind(limit)
        .fetch_all(scope.connection())
        .await?;
    rows.iter().map(row_to_session).collect()
}

/// How many sessions of one sandbox are still open, for the concurrency limit.
pub async fn open_session_count(
    scope: &mut TenantScope<'_>,
    sandbox_id: Uuid,
) -> Result<i64, StoreError> {
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sandbox_sessions
          WHERE environment_id = $1 AND sandbox_id = $2 AND closed_at IS NULL",
    )
    .bind(scope.environment_id())
    .bind(sandbox_id)
    .fetch_one(scope.connection())
    .await?;
    Ok(count)
}
