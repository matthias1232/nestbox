//! Nodes: the machines sandboxes and models run on.
//!
//! Nodes are global rows (a machine is shared by every environment on it), so
//! these queries do not filter by environment. What runs on a node is
//! tenant-scoped and lives in the sandbox, install, and hardware tables.

use sqlx::Row;
use uuid::Uuid;

use super::super::TenantScope;
use crate::domain::node::{Node, NodeKind, NodeStatus};
use crate::error::StoreError;

const NODE_COLUMNS: &str = "id, name, display_name, kind, status, is_local, runner_url, \
     container_gpu, zone, region, description, last_seen, created_at";

fn row_to_node(row: &sqlx::postgres::PgRow) -> Result<Node, StoreError> {
    let kind_raw: String = row.try_get("kind")?;
    let status_raw: String = row.try_get("status")?;
    Ok(Node {
        id: row.try_get("id")?,
        name: row.try_get("name")?,
        display_name: row.try_get("display_name")?,
        kind: NodeKind::parse(&kind_raw).unwrap_or(NodeKind::Local),
        status: NodeStatus::parse(&status_raw).unwrap_or(NodeStatus::Unknown),
        is_local: row.try_get("is_local")?,
        runner_url: row.try_get("runner_url")?,
        container_gpu: row.try_get("container_gpu")?,
        zone: row.try_get("zone")?,
        region: row.try_get("region")?,
        description: row.try_get("description")?,
        last_seen: row.try_get("last_seen")?,
        created_at: row.try_get("created_at")?,
    })
}

/// Every node, local first.
pub async fn list(scope: &mut TenantScope<'_>) -> Result<Vec<Node>, StoreError> {
    let sql = format!("SELECT {NODE_COLUMNS} FROM nodes ORDER BY is_local DESC, name");
    let rows = sqlx::query(&sql).fetch_all(scope.connection()).await?;
    rows.iter().map(row_to_node).collect()
}

/// One node by id.
pub async fn get(scope: &mut TenantScope<'_>, id: Uuid) -> Result<Node, StoreError> {
    let sql = format!("SELECT {NODE_COLUMNS} FROM nodes WHERE id = $1");
    let row = sqlx::query(&sql)
        .bind(id)
        .fetch_optional(scope.connection())
        .await?
        .ok_or(StoreError::NotFound)?;
    row_to_node(&row)
}

/// One node by name (`local`, `gpu-box-01`).
pub async fn get_by_name(scope: &mut TenantScope<'_>, name: &str) -> Result<Node, StoreError> {
    let sql = format!("SELECT {NODE_COLUMNS} FROM nodes WHERE name = $1");
    let row = sqlx::query(&sql)
        .bind(name)
        .fetch_optional(scope.connection())
        .await?
        .ok_or(StoreError::NotFound)?;
    row_to_node(&row)
}

/// The local node: the machine the control plane runs on.
pub async fn local(scope: &mut TenantScope<'_>) -> Result<Node, StoreError> {
    let sql = format!("SELECT {NODE_COLUMNS} FROM nodes WHERE is_local LIMIT 1");
    let row = sqlx::query(&sql)
        .fetch_optional(scope.connection())
        .await?
        .ok_or(StoreError::NotFound)?;
    row_to_node(&row)
}

/// Record what the runner reported about itself.
pub async fn mark_seen(
    scope: &mut TenantScope<'_>,
    id: Uuid,
    status: NodeStatus,
    container_gpu: bool,
) -> Result<(), StoreError> {
    sqlx::query(
        "UPDATE nodes SET status = $2, container_gpu = $3, last_seen = now(), updated_at = now()
          WHERE id = $1",
    )
    .bind(id)
    .bind(status.as_str())
    .bind(container_gpu)
    .execute(scope.connection())
    .await?;
    Ok(())
}

/// Update the display fields of a node. Nothing operational changes here.
pub async fn update_display(
    scope: &mut TenantScope<'_>,
    id: Uuid,
    display_name: &str,
    description: Option<&str>,
) -> Result<Node, StoreError> {
    let sql = format!(
        "UPDATE nodes SET display_name = $2, description = $3, updated_at = now()
          WHERE id = $1 RETURNING {NODE_COLUMNS}"
    );
    let row = sqlx::query(&sql)
        .bind(id)
        .bind(display_name)
        .bind(description)
        .fetch_optional(scope.connection())
        .await?
        .ok_or(StoreError::NotFound)?;
    row_to_node(&row)
}

/// How many sandboxes of this environment sit on one node.
///
/// The single-node form of `sandbox_counts`, for the detail view: a caller that
/// asks about one node should not summarise the whole table to get its number.
pub async fn sandbox_count(scope: &mut TenantScope<'_>, node_id: Uuid) -> Result<i64, StoreError> {
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*)::bigint FROM sandboxes
          WHERE environment_id = $1 AND node_id = $2 AND status <> 'deleted'",
    )
    .bind(scope.environment_id())
    .bind(node_id)
    .fetch_one(scope.connection())
    .await?;
    Ok(count)
}

/// How many sandboxes of this environment sit on each node.
pub async fn sandbox_counts(
    scope: &mut TenantScope<'_>,
) -> Result<std::collections::BTreeMap<Uuid, i64>, StoreError> {
    let rows = sqlx::query(
        "SELECT node_id, COUNT(*) AS n FROM sandboxes
          WHERE environment_id = $1 AND status <> 'deleted'
          GROUP BY node_id",
    )
    .bind(scope.environment_id())
    .fetch_all(scope.connection())
    .await?;
    let mut out = std::collections::BTreeMap::new();
    for row in rows {
        out.insert(row.try_get::<Uuid, _>("node_id")?, row.try_get::<i64, _>("n")?);
    }
    Ok(out)
}
