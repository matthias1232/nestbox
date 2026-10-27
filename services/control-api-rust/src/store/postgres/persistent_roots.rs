//! Persistent roots, their mounts, and the agents inside a sandbox.

use sqlx::Row;
use uuid::Uuid;

use super::super::TenantScope;
use crate::domain::persist::{Mount, MountRef, MountStatus, PersistentRoot, RootKind};
use crate::error::StoreError;

const ROOT_COLUMNS: &str = "id, environment_id, node_id, kind, owner_user_id, slug, host_path, \
     bytes_used, measured_at, archived_at, created_at";

fn row_to_root(row: &sqlx::postgres::PgRow) -> Result<PersistentRoot, StoreError> {
    let kind_raw: String = row.try_get("kind")?;
    Ok(PersistentRoot {
        id: row.try_get("id")?,
        environment_id: row.try_get("environment_id")?,
        node_id: row.try_get("node_id")?,
        kind: RootKind::parse(&kind_raw).unwrap_or(RootKind::Project),
        owner_user_id: row.try_get("owner_user_id")?,
        slug: row.try_get("slug")?,
        host_path: row.try_get("host_path")?,
        bytes_used: row.try_get("bytes_used")?,
        measured_at: row.try_get("measured_at")?,
        archived_at: row.try_get("archived_at")?,
        created_at: row.try_get("created_at")?,
        mounted_in: Vec::new(),
    })
}

/// Every root in this environment that is not archived.
pub async fn list_roots(scope: &mut TenantScope<'_>) -> Result<Vec<PersistentRoot>, StoreError> {
    let sql = format!(
        "SELECT {ROOT_COLUMNS} FROM persistent_roots
          WHERE environment_id = $1 AND archived_at IS NULL
          ORDER BY kind, slug"
    );
    let rows = sqlx::query(&sql)
        .bind(scope.environment_id())
        .fetch_all(scope.connection())
        .await?;
    rows.iter().map(row_to_root).collect()
}

/// One root by id.
pub async fn get_root(scope: &mut TenantScope<'_>, id: Uuid) -> Result<PersistentRoot, StoreError> {
    let sql = format!(
        "SELECT {ROOT_COLUMNS} FROM persistent_roots WHERE environment_id = $1 AND id = $2"
    );
    let row = sqlx::query(&sql)
        .bind(scope.environment_id())
        .bind(id)
        .fetch_optional(scope.connection())
        .await?
        .ok_or(StoreError::NotFound)?;
    row_to_root(&row)
}

/// What an insert carries. The host path is computed by the service from
/// validated parts; it is never a request field.
pub struct NewRoot<'a> {
    pub node_id: Uuid,
    pub kind: RootKind,
    pub owner_user_id: Option<Uuid>,
    pub slug: &'a str,
    pub host_path: &'a str,
}

/// Create a root.
pub async fn insert_root(
    scope: &mut TenantScope<'_>,
    new: NewRoot<'_>,
) -> Result<PersistentRoot, StoreError> {
    let sql = format!(
        "INSERT INTO persistent_roots (environment_id, node_id, kind, owner_user_id, slug, host_path)
         VALUES ($1, $2, $3, $4, $5, $6)
         RETURNING {ROOT_COLUMNS}"
    );
    let row = sqlx::query(&sql)
        .bind(scope.environment_id())
        .bind(new.node_id)
        .bind(new.kind.as_str())
        .bind(new.owner_user_id)
        .bind(new.slug)
        .bind(new.host_path)
        .fetch_one(scope.connection())
        .await?;
    row_to_root(&row)
}

/// Record a usage measurement.
pub async fn set_usage(
    scope: &mut TenantScope<'_>,
    id: Uuid,
    bytes_used: i64,
) -> Result<(), StoreError> {
    sqlx::query(
        "UPDATE persistent_roots SET bytes_used = $3, measured_at = now(), updated_at = now()
          WHERE environment_id = $1 AND id = $2",
    )
    .bind(scope.environment_id())
    .bind(id)
    .bind(bytes_used)
    .execute(scope.connection())
    .await?;
    Ok(())
}

/// Archive (keep data, hide) or delete a root row.
pub async fn remove_root(
    scope: &mut TenantScope<'_>,
    id: Uuid,
    archive: bool,
) -> Result<(), StoreError> {
    let result = if archive {
        sqlx::query(
            "UPDATE persistent_roots SET archived_at = now(), updated_at = now()
              WHERE environment_id = $1 AND id = $2 AND archived_at IS NULL",
        )
        .bind(scope.environment_id())
        .bind(id)
        .execute(scope.connection())
        .await?
    } else {
        sqlx::query("DELETE FROM persistent_roots WHERE environment_id = $1 AND id = $2")
            .bind(scope.environment_id())
            .bind(id)
            .execute(scope.connection())
            .await?
    };
    if result.rows_affected() == 0 {
        return Err(StoreError::NotFound);
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Mounts
// ---------------------------------------------------------------------------

const MOUNT_COLUMNS: &str = "m.id, m.root_id, m.sandbox_id, m.mount_path, m.read_only, m.status, m.created_at";

fn row_to_mount(row: &sqlx::postgres::PgRow) -> Result<Mount, StoreError> {
    let status_raw: String = row.try_get("status")?;
    Ok(Mount {
        id: row.try_get("id")?,
        root_id: row.try_get("root_id")?,
        sandbox_id: row.try_get("sandbox_id")?,
        mount_path: row.try_get("mount_path")?,
        read_only: row.try_get("read_only")?,
        status: MountStatus::parse(&status_raw).unwrap_or(MountStatus::Missing),
        created_at: row.try_get("created_at")?,
    })
}

/// Mounts of one sandbox, with the root's host path for the runtime spec.
pub async fn mounts_for_sandbox(
    scope: &mut TenantScope<'_>,
    sandbox_id: Uuid,
) -> Result<Vec<(Mount, String)>, StoreError> {
    let sql = format!(
        "SELECT {MOUNT_COLUMNS}, r.host_path
           FROM mounts m
           JOIN persistent_roots r ON r.id = m.root_id
          WHERE m.sandbox_id = $1 AND m.status = 'mounted'
          ORDER BY m.mount_path"
    );
    let rows = sqlx::query(&sql)
        .bind(sandbox_id)
        .fetch_all(scope.connection())
        .await?;
    rows.iter()
        .map(|row| Ok((row_to_mount(row)?, row.try_get::<String, _>("host_path")?)))
        .collect()
}

/// Where a root is mounted, for the list view.
pub async fn mounts_for_root(
    scope: &mut TenantScope<'_>,
    root_id: Uuid,
) -> Result<Vec<MountRef>, StoreError> {
    let rows = sqlx::query(
        "SELECT m.sandbox_id, s.name AS sandbox_name, m.mount_path, m.read_only
           FROM mounts m JOIN sandboxes s ON s.id = m.sandbox_id
          WHERE m.root_id = $1 AND m.status = 'mounted'
          ORDER BY s.name",
    )
    .bind(root_id)
    .fetch_all(scope.connection())
    .await?;
    rows.iter()
        .map(|row| {
            Ok(MountRef {
                sandbox_id: row.try_get("sandbox_id")?,
                sandbox_name: row.try_get("sandbox_name")?,
                mount_path: row.try_get("mount_path")?,
                read_only: row.try_get("read_only")?,
            })
        })
        .collect()
}

/// Attach a root to a sandbox. The partial unique index refuses a second
/// read-write mount; the caller maps that to `mount.conflict`.
pub async fn insert_mount(
    scope: &mut TenantScope<'_>,
    root_id: Uuid,
    sandbox_id: Uuid,
    mount_path: &str,
    read_only: bool,
) -> Result<Mount, StoreError> {
    let sql = format!(
        "WITH inserted AS (
            INSERT INTO mounts (root_id, sandbox_id, mount_path, read_only, status)
            VALUES ($1, $2, $3, $4, 'mounted')
            ON CONFLICT (root_id, sandbox_id) DO UPDATE
                SET mount_path = EXCLUDED.mount_path, read_only = EXCLUDED.read_only,
                    status = 'mounted'
            RETURNING *
         )
         SELECT {MOUNT_COLUMNS} FROM inserted m"
    );
    let row = sqlx::query(&sql)
        .bind(root_id)
        .bind(sandbox_id)
        .bind(mount_path)
        .bind(read_only)
        .fetch_one(scope.connection())
        .await?;
    row_to_mount(&row)
}

/// Mark a mount dismounted.
pub async fn dismount(
    scope: &mut TenantScope<'_>,
    root_id: Uuid,
    sandbox_id: Uuid,
) -> Result<(), StoreError> {
    let result = sqlx::query(
        "UPDATE mounts SET status = 'dismounted' WHERE root_id = $1 AND sandbox_id = $2",
    )
    .bind(root_id)
    .bind(sandbox_id)
    .execute(scope.connection())
    .await?;
    if result.rows_affected() == 0 {
        return Err(StoreError::NotFound);
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Sandbox agents
// ---------------------------------------------------------------------------

/// An agent row inside a sandbox.
#[derive(Debug, Clone, serde::Serialize, utoipa::ToSchema)]
pub struct SandboxAgentRow {
    pub id: Uuid,
    pub sandbox_id: Uuid,
    pub pack_key: String,
    pub instance: String,
    pub home_mode: String,
    pub persistence: String,
    pub root_id: Option<Uuid>,
    pub port: Option<i32>,
    pub status: String,
}

const AGENT_COLUMNS: &str =
    "id, sandbox_id, pack_key, instance, home_mode, persistence, root_id, port, status";

fn row_to_agent(row: &sqlx::postgres::PgRow) -> Result<SandboxAgentRow, StoreError> {
    Ok(SandboxAgentRow {
        id: row.try_get("id")?,
        sandbox_id: row.try_get("sandbox_id")?,
        pack_key: row.try_get("pack_key")?,
        instance: row.try_get("instance")?,
        home_mode: row.try_get("home_mode")?,
        persistence: row.try_get("persistence")?,
        root_id: row.try_get("root_id")?,
        port: row.try_get("port")?,
        status: row.try_get("status")?,
    })
}

/// Agents of one sandbox.
pub async fn agents_for_sandbox(
    scope: &mut TenantScope<'_>,
    sandbox_id: Uuid,
) -> Result<Vec<SandboxAgentRow>, StoreError> {
    let sql = format!(
        "SELECT {AGENT_COLUMNS} FROM sandbox_agents WHERE sandbox_id = $1 AND status <> 'removed'
          ORDER BY instance"
    );
    let rows = sqlx::query(&sql)
        .bind(sandbox_id)
        .fetch_all(scope.connection())
        .await?;
    rows.iter().map(row_to_agent).collect()
}

/// Add an agent to a sandbox.
pub async fn insert_agent(
    scope: &mut TenantScope<'_>,
    sandbox_id: Uuid,
    pack_key: &str,
    instance: &str,
    home_mode: &str,
    persistence: &str,
    root_id: Option<Uuid>,
    port: Option<i32>,
) -> Result<SandboxAgentRow, StoreError> {
    let sql = format!(
        "INSERT INTO sandbox_agents
            (sandbox_id, pack_key, instance, home_mode, persistence, root_id, port)
         VALUES ($1, $2, $3, $4, $5, $6, $7)
         RETURNING {AGENT_COLUMNS}"
    );
    let row = sqlx::query(&sql)
        .bind(sandbox_id)
        .bind(pack_key)
        .bind(instance)
        .bind(home_mode)
        .bind(persistence)
        .bind(root_id)
        .bind(port)
        .fetch_one(scope.connection())
        .await?;
    row_to_agent(&row)
}

/// Mark an agent removed.
pub async fn remove_agent(
    scope: &mut TenantScope<'_>,
    sandbox_id: Uuid,
    instance: &str,
) -> Result<SandboxAgentRow, StoreError> {
    let sql = format!(
        "UPDATE sandbox_agents SET status = 'removed'
          WHERE sandbox_id = $1 AND instance = $2 AND status <> 'removed'
      RETURNING {AGENT_COLUMNS}"
    );
    let row = sqlx::query(&sql)
        .bind(sandbox_id)
        .bind(instance)
        .fetch_optional(scope.connection())
        .await?
        .ok_or(StoreError::NotFound)?;
    row_to_agent(&row)
}
