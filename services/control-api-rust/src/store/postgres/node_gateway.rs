//! The gateway side of a node: its tunnel state and its egress lock (ADR 0057).

use sqlx::Row;
use uuid::Uuid;

use super::super::TenantScope;
use crate::domain::node_tunnel::TunnelStatus;
use crate::error::StoreError;

/// The tunnel of every node in this environment.
pub async fn tunnels(scope: &mut TenantScope<'_>) -> Result<Vec<TunnelStatus>, StoreError> {
    let rows = sqlx::query(
        "SELECT id, name, tunnel_state, tunnel_handshake_at, tunnel_rx_bytes, tunnel_tx_bytes,
                public_endpoint
           FROM nodes
          WHERE environment_id = $1
          ORDER BY is_local DESC, name",
    )
    .bind(scope.environment_id())
    .fetch_all(scope.connection())
    .await?;
    let now = chrono::Utc::now();
    let mut out = Vec::new();
    for row in rows {
        let handshake: Option<chrono::DateTime<chrono::Utc>> = row.try_get("tunnel_handshake_at")?;
        out.push(TunnelStatus {
            node_id: row.try_get("id")?,
            node_name: row.try_get("name")?,
            state: row.try_get("tunnel_state")?,
            handshake_age_seconds: handshake.map(|at| (now - at).num_seconds().max(0)),
            rx_bytes: row.try_get("tunnel_rx_bytes")?,
            tx_bytes: row.try_get("tunnel_tx_bytes")?,
            endpoint: row.try_get("public_endpoint")?,
        });
    }
    Ok(out)
}

/// Choose how a node reaches the internet.
pub async fn set_egress_lock(
    scope: &mut TenantScope<'_>,
    node_id: Uuid,
    lock: &str,
) -> Result<(), StoreError> {
    let affected = sqlx::query(
        "UPDATE nodes SET egress_lock = $3, updated_at = now()
          WHERE environment_id = $1 AND id = $2",
    )
    .bind(scope.environment_id())
    .bind(node_id)
    .bind(lock)
    .execute(scope.connection())
    .await?
    .rows_affected();
    if affected == 0 {
        Err(StoreError::NotFound)
    } else {
        Ok(())
    }
}

/// The lock a node should apply, read by the node itself.
pub async fn egress_lock(
    scope: &mut TenantScope<'_>,
    node_id: Uuid,
) -> Result<Option<String>, StoreError> {
    let lock: Option<String> =
        sqlx::query_scalar("SELECT egress_lock FROM nodes WHERE environment_id = $1 AND id = $2")
            .bind(scope.environment_id())
            .bind(node_id)
            .fetch_optional(scope.connection())
            .await?;
    Ok(lock)
}

/// Store what a node's tunnel looks like from the hub.
pub async fn record_tunnel(
    scope: &mut TenantScope<'_>,
    node_id: Uuid,
    state: &str,
    handshake_at: Option<chrono::DateTime<chrono::Utc>>,
    rx_bytes: i64,
    tx_bytes: i64,
) -> Result<(), StoreError> {
    sqlx::query(
        "UPDATE nodes
            SET tunnel_state = $3, tunnel_handshake_at = $4,
                tunnel_rx_bytes = $5, tunnel_tx_bytes = $6, updated_at = now()
          WHERE environment_id = $1 AND id = $2",
    )
    .bind(scope.environment_id())
    .bind(node_id)
    .bind(state)
    .bind(handshake_at)
    .bind(rx_bytes)
    .bind(tx_bytes)
    .execute(scope.connection())
    .await?;
    Ok(())
}
