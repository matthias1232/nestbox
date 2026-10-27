//! Persistent roots, their mounts, and the agents inside a sandbox.

use sqlx::Row;
use uuid::Uuid;

use super::super::TenantScope;
use crate::domain::builder::{AgentSpec, HomeMode};
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
          WHERE m.sandbox_id = $1 AND m.status = 'mounted' AND r.environment_id = $2
          ORDER BY m.mount_path"
    );
    let rows = sqlx::query(&sql)
        .bind(sandbox_id)
        .bind(scope.environment_id())
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
          WHERE m.root_id = $1 AND m.status = 'mounted' AND s.environment_id = $2
          ORDER BY s.name",
    )
    .bind(root_id)
    .bind(scope.environment_id())
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
        "UPDATE mounts SET status = 'dismounted'
          WHERE root_id = $1 AND sandbox_id = $2
            AND EXISTS (SELECT 1 FROM sandboxes s
                         WHERE s.id = mounts.sandbox_id AND s.environment_id = $3)",
    )
    .bind(root_id)
    .bind(sandbox_id)
    .bind(scope.environment_id())
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

/// Add an agent to a sandbox, reviving an instance that was removed.
///
/// Returns `None` when the instance is already in use in this sandbox, which
/// the service reports as `agent.exists`. A removed row keeps its place as a
/// tombstone — it is the only trace of an agent whose data was dropped — so
/// re-adding the same instance updates that row instead of inserting a second
/// one. That is also what makes `add_agents_from_spec` idempotent.
pub async fn insert_agent(
    scope: &mut TenantScope<'_>,
    sandbox_id: Uuid,
    pack_key: &str,
    instance: &str,
    home_mode: &str,
    persistence: &str,
    root_id: Option<Uuid>,
    port: Option<i32>,
) -> Result<Option<SandboxAgentRow>, StoreError> {
    let sql = format!(
        "INSERT INTO sandbox_agents
            (sandbox_id, pack_key, instance, home_mode, persistence, root_id, port)
         VALUES ($1, $2, $3, $4, $5, $6, $7)
         ON CONFLICT (sandbox_id, instance) DO UPDATE
             SET pack_key = EXCLUDED.pack_key,
                 home_mode = EXCLUDED.home_mode,
                 persistence = EXCLUDED.persistence,
                 root_id = EXCLUDED.root_id,
                 port = EXCLUDED.port,
                 status = 'configured'
           WHERE sandbox_agents.status = 'removed'
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
        .fetch_optional(scope.connection())
        .await?;
    row.as_ref().map(row_to_agent).transpose()
}

/// Mark an agent removed.
///
/// The row stays as a tombstone so the instance name, and the root it pointed
/// at, remain explainable after the fact. The root itself is a separate
/// decision the service makes.
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

/// Point an agent row at its persistent root.
pub async fn set_agent_root(
    scope: &mut TenantScope<'_>,
    sandbox_id: Uuid,
    instance: &str,
    root_id: Uuid,
) -> Result<(), StoreError> {
    let result = sqlx::query(
        "UPDATE sandbox_agents SET root_id = $3
          WHERE sandbox_id = $1 AND instance = $2 AND status <> 'removed'",
    )
    .bind(sandbox_id)
    .bind(instance)
    .bind(root_id)
    .execute(scope.connection())
    .await?;
    if result.rows_affected() == 0 {
        return Err(StoreError::NotFound);
    }
    Ok(())
}

/// The baseline key of the recipe a sandbox was instantiated from.
///
/// `None` means the sandbox was not built from a recipe, so no baseline
/// constrains which packs may join it. The join is used instead of a column on
/// `sandboxes` because a recipe is one thing and its baseline another; copying
/// the key onto the sandbox would let the two drift.
pub async fn recipe_baseline_for_sandbox(
    scope: &mut TenantScope<'_>,
    sandbox_id: Uuid,
) -> Result<Option<String>, StoreError> {
    let row = sqlx::query(
        "SELECT r.baseline_key
           FROM sandboxes s JOIN recipes r ON r.id = s.recipe_id
          WHERE s.id = $1 AND s.environment_id = $2",
    )
    .bind(sandbox_id)
    .bind(scope.environment_id())
    .fetch_optional(scope.connection())
    .await?;
    match row {
        Some(row) => Ok(Some(row.try_get("baseline_key")?)),
        None => Ok(None),
    }
}

/// Where persistent roots live on a node.
///
/// An environment setting rather than a column, because it describes the host
/// filesystem, which belongs to the deployment and not to a tenant record.
fn persistent_base() -> String {
    std::env::var("NESTBOX_PERSISTENT_BASE")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "/var/lib/nestbox/persistent".to_string())
}

/// The slug of one agent's home root: instance, sandbox name, stable suffix.
///
/// The suffix is the first eight characters of the sandbox id, so two
/// environments that happen to use the same sandbox name on the same node get
/// two directories instead of silently sharing one. The instance is part of the
/// slug because `persistent_roots` is unique on `(node, owner_user_id, slug)`
/// and the owner column holds a user, while the path's owner segment holds the
/// agent instance. Names are validated as slugs before they reach this point and
/// the suffix is hex, so the result always matches the slug constraint.
fn agent_home_slug(instance: &str, sandbox_name: &str, sandbox_id: Uuid) -> String {
    let short: String = sandbox_id.simple().to_string().chars().take(8).collect();
    let reserved = instance.len() + 1 + short.len() + 1;
    let room = 63usize.saturating_sub(reserved);
    let stem: String = sandbox_name.chars().take(room).collect();
    let stem = stem.trim_end_matches('-');
    if stem.is_empty() {
        format!("{instance}-{short}")
    } else {
        format!("{instance}-{stem}-{short}")
    }
}

/// Ensure the persistent root that holds one agent's home.
///
/// The host path is derived from validated parts and never supplied by a
/// request (ADR 0021): `<base>/<instance>/agent--<sandbox>-<short id>`. The
/// lookup is by path, so re-adding an agent whose data was kept reuses the
/// directory that already exists instead of creating a second row for it.
pub async fn ensure_agent_home_root(
    scope: &mut TenantScope<'_>,
    sandbox_id: Uuid,
    instance: &str,
    owner_user_id: Option<Uuid>,
) -> Result<PersistentRoot, StoreError> {
    let sandbox = sqlx::query("SELECT node_id, name FROM sandboxes WHERE id = $1 AND environment_id = $2")
        .bind(sandbox_id)
        .bind(scope.environment_id())
        .fetch_optional(scope.connection())
        .await?
        .ok_or(StoreError::NotFound)?;
    let node_id: Uuid = sandbox.try_get("node_id")?;
    let sandbox_name: String = sandbox.try_get("name")?;

    let slug_value = agent_home_slug(instance, &sandbox_name, sandbox_id);
    let slug = crate::domain::persist::RootSlug::new(&slug_value)
        .map_err(StoreError::UniqueViolation)?;
    let path = crate::domain::persist::host_path(
        &persistent_base(),
        instance,
        RootKind::AgentHome,
        &slug,
    );

    let existing = sqlx::query(&format!(
        "SELECT {ROOT_COLUMNS} FROM persistent_roots
          WHERE environment_id = $1 AND node_id = $2 AND kind = 'agent_home'
            AND host_path = $3 AND archived_at IS NULL"
    ))
    .bind(scope.environment_id())
    .bind(node_id)
    .bind(&path)
    .fetch_optional(scope.connection())
    .await?;
    if let Some(row) = existing {
        return row_to_root(&row);
    }

    insert_root(
        scope,
        NewRoot {
            node_id,
            kind: RootKind::AgentHome,
            owner_user_id,
            slug: &slug_value,
            host_path: &path,
        },
    )
    .await
}

/// Give an agent its persistent home: the root, the link, and the mount.
///
/// All three in one place so the API path and the recipe path cannot disagree
/// about what a persistent home is. A shared home is one directory for every
/// agent, so a shared agent adopts the root another shared agent in the same
/// sandbox already has instead of claiming a second directory at the same path.
/// The mount is read-write because an agent writes its own home; the partial
/// unique index on `mounts` therefore refuses a second sandbox mounting the same
/// home, which the service reports as `mount.conflict`.
pub async fn attach_agent_home(
    scope: &mut TenantScope<'_>,
    sandbox_id: Uuid,
    instance: &str,
    home_mode: HomeMode,
    owner_user_id: Option<Uuid>,
) -> Result<PersistentRoot, StoreError> {
    let root = match home_mode {
        HomeMode::Shared => match shared_home_root(scope, sandbox_id).await? {
            Some(root_id) => get_root(scope, root_id).await?,
            None => ensure_agent_home_root(scope, sandbox_id, instance, owner_user_id).await?,
        },
        HomeMode::Isolated => {
            ensure_agent_home_root(scope, sandbox_id, instance, owner_user_id).await?
        }
    };
    set_agent_root(scope, sandbox_id, instance, root.id).await?;
    insert_mount(
        scope,
        root.id,
        sandbox_id,
        &home_mode.home_path(instance),
        false,
    )
    .await?;
    Ok(root)
}

/// The root a shared-home agent in this sandbox already writes to.
///
/// One shared home means one directory, so the second agent joins the first
/// one's root. Without this lookup every shared agent would mount its own
/// directory at `/home/nestbox`, and only the last mount would be visible.
async fn shared_home_root(
    scope: &mut TenantScope<'_>,
    sandbox_id: Uuid,
) -> Result<Option<Uuid>, StoreError> {
    let root: Option<Uuid> = sqlx::query_scalar(
        "SELECT root_id FROM sandbox_agents
          WHERE sandbox_id = $1 AND home_mode = 'shared' AND root_id IS NOT NULL
            AND status <> 'removed'
          ORDER BY created_at LIMIT 1",
    )
    .bind(sandbox_id)
    .fetch_optional(scope.connection())
    .await?;
    Ok(root)
}

/// Whether another agent in this sandbox still lives in the same root.
///
/// Asked before a root is deleted: with a shared home, the agent being removed
/// is usually not the only one using the directory.
pub async fn root_used_by_other_agent(
    scope: &mut TenantScope<'_>,
    root_id: Uuid,
    sandbox_id: Uuid,
    instance: &str,
) -> Result<bool, StoreError> {
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sandbox_agents
          WHERE sandbox_id = $1 AND root_id = $2 AND instance <> $3
            AND status <> 'removed'",
    )
    .bind(sandbox_id)
    .bind(root_id)
    .bind(instance)
    .fetch_one(scope.connection())
    .await?;
    Ok(count > 0)
}

/// Whether a root is currently mounted somewhere other than this sandbox.
///
/// Asked before a root is deleted: another sandbox may still be writing to it,
/// and deleting the row would silently unmount it there.
pub async fn root_mounted_elsewhere(
    scope: &mut TenantScope<'_>,
    root_id: Uuid,
    sandbox_id: Uuid,
) -> Result<bool, StoreError> {
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM mounts
          WHERE root_id = $1 AND sandbox_id <> $2 AND status = 'mounted'",
    )
    .bind(root_id)
    .bind(sandbox_id)
    .fetch_one(scope.connection())
    .await?;
    Ok(count > 0)
}

/// Add the agents a recipe names, idempotently.
///
/// Called at instantiate time, which may run again for the same recipe — a
/// retried instantiate, a rebuild from recipe — so an instance that is already
/// configured is left alone rather than duplicated. Removed instances are
/// revived by `insert_agent`, with the same home when their data was kept.
pub async fn add_agents_from_spec(
    scope: &mut TenantScope<'_>,
    sandbox_id: Uuid,
    specs: &[AgentSpec],
) -> Result<Vec<SandboxAgentRow>, StoreError> {
    for spec in specs {
        let existing: Option<(String, Option<Uuid>)> = sqlx::query_as(
            "SELECT status, root_id FROM sandbox_agents WHERE sandbox_id = $1 AND instance = $2",
        )
        .bind(sandbox_id)
        .bind(&spec.instance)
        .fetch_optional(scope.connection())
        .await?;
        if let Some((status, _)) = existing {
            if status != "removed" {
                continue;
            }
        }

        let inserted = insert_agent(
            scope,
            sandbox_id,
            &spec.pack_key,
            &spec.instance,
            spec.home_mode.as_str(),
            spec.persistence.as_str(),
            None,
            spec.port.map(i32::from),
        )
        .await?;
        if inserted.is_none() {
            // Another writer configured this instance in the same breath; the
            // row is what this call was asked to produce.
            continue;
        }

        if spec.persistence == crate::domain::builder::Persistence::Persistent {
            attach_agent_home(scope, sandbox_id, &spec.instance, spec.home_mode, None).await?;
        }
    }

    agents_for_sandbox(scope, sandbox_id).await
}
