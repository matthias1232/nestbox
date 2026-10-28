//! Gateways and supervisors.

use sqlx::Row;
use uuid::Uuid;

use super::super::TenantScope;
use crate::domain::gateway::{
    Gateway, GatewayStatus, Supervisor, SupervisorStatus,
};
use crate::error::StoreError;

const GATEWAY_COLUMNS: &str = "g.id, g.cluster_id, g.node_id, g.name, g.endpoint, g.status, \
     g.version, g.settings, g.settings_version, g.settings_applied_version, \
     g.capacity, g.used, g.last_seen, g.created_at, g.updated_at";

fn row_to_gateway(row: &sqlx::postgres::PgRow) -> Result<Gateway, StoreError> {
    let status_raw: String = row.try_get("status")?;
    Ok(Gateway {
        id: row.try_get("id")?,
        cluster_id: row.try_get("cluster_id")?,
        node_id: row.try_get("node_id")?,
        name: row.try_get("name")?,
        endpoint: row.try_get("endpoint")?,
        status: GatewayStatus::parse(&status_raw).unwrap_or(GatewayStatus::Unknown),
        version: row.try_get("version")?,
        settings: row.try_get("settings")?,
        settings_version: row.try_get("settings_version")?,
        settings_applied_version: row.try_get("settings_applied_version")?,
        capacity: row.try_get("capacity")?,
        used: row.try_get("used")?,
        last_seen: row.try_get("last_seen")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

/// All gateways in this environment, via their cluster.
pub async fn list(scope: &mut TenantScope<'_>) -> Result<Vec<Gateway>, StoreError> {
    let sql = format!(
        "SELECT {GATEWAY_COLUMNS}
           FROM gateways g
           JOIN clusters c ON c.id = g.cluster_id
          WHERE c.environment_id = $1
          ORDER BY g.name"
    );
    let rows = sqlx::query(&sql)
        .bind(scope.environment_id())
        .fetch_all(scope.connection())
        .await?;
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        out.push(row_to_gateway(&row)?);
    }
    Ok(out)
}

/// One gateway.
pub async fn get(scope: &mut TenantScope<'_>, id: Uuid) -> Result<Gateway, StoreError> {
    let sql = format!(
        "SELECT {GATEWAY_COLUMNS}
           FROM gateways g
           JOIN clusters c ON c.id = g.cluster_id
          WHERE c.environment_id = $1 AND g.id = $2"
    );
    let row = sqlx::query(&sql)
        .bind(scope.environment_id())
        .bind(id)
        .fetch_optional(scope.connection())
        .await?
        .ok_or(StoreError::NotFound)?;
    row_to_gateway(&row)
}

/// Replace a gateway's operator configuration, bumping the desired version.
///
/// The applied version is untouched, which is what creates drift and makes the
/// sync button meaningful.
pub async fn update_settings(
    scope: &mut TenantScope<'_>,
    id: Uuid,
    expected_version: i64,
    settings: &serde_json::Value,
) -> Result<Gateway, StoreError> {
    let sql = format!(
        "WITH updated AS (
            UPDATE gateways g
               SET settings = $3,
                   settings_version = g.settings_version + 1,
                   updated_at = now()
              FROM clusters c
             WHERE c.id = g.cluster_id
               AND c.environment_id = $1
               AND g.id = $2
               AND g.settings_version = $4
         RETURNING {GATEWAY_COLUMNS}
         )
         SELECT {GATEWAY_COLUMNS} FROM updated g"
    );
    let row = sqlx::query(&sql)
        .bind(scope.environment_id())
        .bind(id)
        .bind(settings)
        .bind(expected_version)
        .fetch_optional(scope.connection())
        .await?;

    match row {
        Some(row) => row_to_gateway(&row),
        None => Err(StoreError::VersionConflict),
    }
}

/// Mark a gateway as having applied a version.
pub async fn mark_applied(
    scope: &mut TenantScope<'_>,
    id: Uuid,
    applied_version: i64,
) -> Result<(), StoreError> {
    let affected = sqlx::query(
        "UPDATE gateways g
            SET settings_applied_version = GREATEST(g.settings_applied_version, $3),
                last_seen = now(), updated_at = now()
           FROM clusters c
          WHERE c.id = g.cluster_id AND c.environment_id = $1 AND g.id = $2",
    )
    .bind(scope.environment_id())
    .bind(id)
    .bind(applied_version)
    .execute(scope.connection())
    .await?
    .rows_affected();
    if affected == 0 {
        Err(StoreError::NotFound)
    } else {
        Ok(())
    }
}

/// Gateways that are drifted, for the sync job.
pub async fn drifted(scope: &mut TenantScope<'_>) -> Result<Vec<Gateway>, StoreError> {
    let sql = format!(
        "SELECT {GATEWAY_COLUMNS}
           FROM gateways g
           JOIN clusters c ON c.id = g.cluster_id
          WHERE c.environment_id = $1
            AND g.settings_applied_version < g.settings_version
          ORDER BY g.name"
    );
    let rows = sqlx::query(&sql)
        .bind(scope.environment_id())
        .fetch_all(scope.connection())
        .await?;
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        out.push(row_to_gateway(&row)?);
    }
    Ok(out)
}

/// Record a supervisor for a sandbox on a gateway.
///
/// A supervisor is the runtime's name for "this gateway supervises that
/// sandbox". The row is written by the control plane when an operator registers
/// one; the runtime later reports its state.
pub async fn insert_supervisor(
    scope: &mut TenantScope<'_>,
    gateway_id: Uuid,
    sandbox_id: Option<Uuid>,
    name: &str,
    status: &str,
) -> Result<Supervisor, StoreError> {
    let row = sqlx::query(
        "INSERT INTO supervisors (gateway_id, sandbox_id, name, status)
         VALUES ($1, $2, $3, $4)
         RETURNING id, gateway_id, sandbox_id, name, status, version,
                   config_dirtiness, last_policy_applied_at, last_seen",
    )
    .bind(gateway_id)
    .bind(sandbox_id)
    .bind(name)
    .bind(status)
    .fetch_one(scope.connection())
    .await?;
    let status_raw: String = row.try_get("status")?;
    Ok(Supervisor {
        id: row.try_get("id")?,
        gateway_id: row.try_get("gateway_id")?,
        sandbox_id: row.try_get("sandbox_id")?,
        name: row.try_get("name")?,
        status: SupervisorStatus::parse(&status_raw).unwrap_or(SupervisorStatus::Unknown),
        version: row.try_get("version")?,
        config_dirtiness: row.try_get("config_dirtiness")?,
        last_policy_applied_at: row.try_get("last_policy_applied_at")?,
        last_seen: row.try_get("last_seen")?,
        reported: row.try_get("reported").unwrap_or(None),
    })
}

/// Insert a supervisor with the hash of the token it will present.
pub async fn insert_supervisor_with_token(
    scope: &mut TenantScope<'_>,
    gateway_id: Uuid,
    sandbox_id: Uuid,
    name: &str,
    token_hash: &str,
) -> Result<Supervisor, StoreError> {
    let row = sqlx::query(
        "INSERT INTO supervisors (gateway_id, sandbox_id, name, status, token_hash, last_seen)
         VALUES ($1, $2, $3, 'starting', $4, now())
         RETURNING id, gateway_id, sandbox_id, name, status, version,
                   config_dirtiness, last_policy_applied_at, last_seen",
    )
    .bind(gateway_id)
    .bind(sandbox_id)
    .bind(name)
    .bind(token_hash)
    .fetch_one(scope.connection())
    .await?;
    let status_raw: String = row.try_get("status")?;
    Ok(Supervisor {
        id: row.try_get("id")?,
        gateway_id: row.try_get("gateway_id")?,
        sandbox_id: row.try_get("sandbox_id")?,
        name: row.try_get("name")?,
        status: SupervisorStatus::parse(&status_raw).unwrap_or(SupervisorStatus::Unknown),
        version: row.try_get("version")?,
        config_dirtiness: row.try_get("config_dirtiness")?,
        last_policy_applied_at: row.try_get("last_policy_applied_at")?,
        last_seen: row.try_get("last_seen")?,
        reported: row.try_get("reported").unwrap_or(None),
    })
}

/// Find the supervisor a token belongs to.
///
/// The internal report endpoint has no session, so this walks the environments
/// like every other unscoped maintenance read: the token decides which row, and
/// the row decides the scope. Only the hash is compared.
pub async fn find_by_token_hash(
    pool: &sqlx::PgPool,
    token_hash: &str,
) -> Result<Option<(Uuid, Uuid, Option<Uuid>)>, StoreError> {
    let environments: Vec<Uuid> =
        sqlx::query_scalar("SELECT id FROM environments ORDER BY created_at, id")
            .fetch_all(pool)
            .await?;

    for environment_id in environments {
        let mut tx = pool.begin().await?;
        sqlx::query("SELECT set_config('app.environment_id', $1, true)")
            .bind(environment_id.to_string())
            .execute(&mut *tx)
            .await?;
        let row = sqlx::query(
            "SELECT s.id, s.sandbox_id
               FROM supervisors s
               JOIN gateways g ON g.id = s.gateway_id
               JOIN clusters c ON c.id = g.cluster_id
              WHERE c.environment_id = $1 AND s.token_hash = $2",
        )
        .bind(environment_id)
        .bind(token_hash)
        .fetch_optional(&mut *tx)
        .await?;

        if let Some(row) = row {
            let id: Uuid = row.try_get("id")?;
            let sandbox_id: Option<Uuid> = row.try_get("sandbox_id")?;
            tx.commit().await?;
            return Ok(Some((id, environment_id, sandbox_id)));
        }
        tx.rollback().await?;
    }
    Ok(None)
}

/// Point a supervisor at a sandbox.
///
/// A supervisor whose sandbox was recreated keeps its identity and loses the
/// link (the foreign key clears it); the operator re-binds it here rather than
/// deleting and creating a row with the same name.
pub async fn set_supervisor_sandbox(
    scope: &mut TenantScope<'_>,
    id: Uuid,
    sandbox_id: Uuid,
) -> Result<Supervisor, StoreError> {
    let row = sqlx::query(
        "UPDATE supervisors SET sandbox_id = $2, last_seen = now()
          WHERE id = $1
      RETURNING id, gateway_id, sandbox_id, name, status, version,
                config_dirtiness, last_policy_applied_at, last_seen, reported",
    )
    .bind(id)
    .bind(sandbox_id)
    .fetch_optional(scope.connection())
    .await?
    .ok_or(StoreError::NotFound)?;
    let status_raw: String = row.try_get("status")?;
    Ok(Supervisor {
        id: row.try_get("id")?,
        gateway_id: row.try_get("gateway_id")?,
        sandbox_id: row.try_get("sandbox_id")?,
        name: row.try_get("name")?,
        status: SupervisorStatus::parse(&status_raw).unwrap_or(SupervisorStatus::Unknown),
        version: row.try_get("version")?,
        config_dirtiness: row.try_get("config_dirtiness")?,
        last_policy_applied_at: row.try_get("last_policy_applied_at")?,
        last_seen: row.try_get("last_seen")?,
        reported: row.try_get("reported").unwrap_or(None),
    })
}

/// Write one report from a supervisor and update its row.
pub async fn record_report(
    scope: &mut TenantScope<'_>,
    supervisor_id: Uuid,
    report: &crate::domain::gateway::SupervisorReport,
) -> Result<Supervisor, StoreError> {
    let row = sqlx::query(
        "UPDATE supervisors
            SET status = $2, version = COALESCE($3, version),
                config_dirtiness = $4,
                last_policy_applied_at = COALESCE($5, last_policy_applied_at),
                last_seen = now(), reported = $6
          WHERE id = $1
      RETURNING id, gateway_id, sandbox_id, name, status, version,
                config_dirtiness, last_policy_applied_at, last_seen, reported",
    )
    .bind(supervisor_id)
    .bind(report.status.as_str())
    .bind(report.version.as_deref())
    .bind(report.config_dirtiness)
    .bind(report.last_policy_applied_at)
    .bind(serde_json::json!({ "checks": report.checks, "facts": report.facts }))
    .fetch_optional(scope.connection())
    .await?
    .ok_or(StoreError::NotFound)?;

    let status_raw: String = row.try_get("status")?;
    let supervisor = Supervisor {
        id: row.try_get("id")?,
        gateway_id: row.try_get("gateway_id")?,
        sandbox_id: row.try_get("sandbox_id")?,
        name: row.try_get("name")?,
        status: SupervisorStatus::parse(&status_raw).unwrap_or(SupervisorStatus::Unknown),
        version: row.try_get("version")?,
        config_dirtiness: row.try_get("config_dirtiness")?,
        last_policy_applied_at: row.try_get("last_policy_applied_at")?,
        last_seen: row.try_get("last_seen")?,
        reported: row.try_get("reported").unwrap_or(None),
    };

    sqlx::query(
        "INSERT INTO supervisor_reports
            (environment_id, supervisor_id, sandbox_id, status, version,
             config_dirtiness, last_policy_applied_at, checks, facts)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
    )
    .bind(scope.environment_id())
    .bind(supervisor.id)
    .bind(supervisor.sandbox_id)
    .bind(report.status.as_str())
    .bind(report.version.as_deref())
    .bind(report.config_dirtiness)
    .bind(report.last_policy_applied_at)
    .bind(serde_json::json!(report.checks))
    .bind(&report.facts)
    .execute(scope.connection())
    .await?;

    Ok(supervisor)
}

/// All supervisors in this environment.
pub async fn list_supervisors(scope: &mut TenantScope<'_>) -> Result<Vec<Supervisor>, StoreError> {
    let rows = sqlx::query(
        "SELECT s.id, s.gateway_id, s.sandbox_id, s.name, s.status, s.version,
                s.config_dirtiness, s.last_policy_applied_at, s.last_seen, s.reported
           FROM supervisors s
           JOIN gateways g ON g.id = s.gateway_id
           JOIN clusters c ON c.id = g.cluster_id
          WHERE c.environment_id = $1
          ORDER BY s.name",
    )
    .bind(scope.environment_id())
    .fetch_all(scope.connection())
    .await?;

    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        let status_raw: String = row.try_get("status")?;
        out.push(Supervisor {
            id: row.try_get("id")?,
            gateway_id: row.try_get("gateway_id")?,
            sandbox_id: row.try_get("sandbox_id")?,
            name: row.try_get("name")?,
            status: SupervisorStatus::parse(&status_raw).unwrap_or(SupervisorStatus::Unknown),
            version: row.try_get("version")?,
            config_dirtiness: row.try_get("config_dirtiness")?,
            last_policy_applied_at: row.try_get("last_policy_applied_at")?,
            last_seen: row.try_get("last_seen")?,
            reported: row.try_get("reported").unwrap_or(None),
        });
    }
    Ok(out)
}

/// Topology rows: gateways with their sandbox counts, for the cluster graph.
pub async fn topology(scope: &mut TenantScope<'_>) -> Result<serde_json::Value, StoreError> {
    let gateways = list(scope).await?;
    let mut nodes = Vec::new();
    for gateway in &gateways {
        let running: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM sandboxes
              WHERE environment_id = $1 AND gateway_id = $2 AND status = 'running'",
        )
        .bind(scope.environment_id())
        .bind(gateway.id)
        .fetch_one(scope.connection())
        .await?;

        nodes.push(serde_json::json!({
            "id": gateway.id,
            "kind": "gateway",
            "name": gateway.name,
            "status": gateway.status.as_str(),
            "drifted": gateway.is_drifted(),
            "capacity": gateway.capacity,
            "used": gateway.used,
            "running_sandboxes": running,
        }));
    }

    Ok(serde_json::json!({ "nodes": nodes }))
}

const SUPERVISOR_COLUMNS: &str = "s.id, s.gateway_id, s.sandbox_id, s.name, s.status, s.version, \
     s.config_dirtiness, s.last_policy_applied_at, s.last_seen, s.reported";

fn row_to_supervisor(row: &sqlx::postgres::PgRow) -> Result<Supervisor, StoreError> {
    let status_raw: String = row.try_get("status")?;
    Ok(Supervisor {
        id: row.try_get("id")?,
        gateway_id: row.try_get("gateway_id")?,
        sandbox_id: row.try_get("sandbox_id")?,
        name: row.try_get("name")?,
        status: SupervisorStatus::parse(&status_raw).unwrap_or(SupervisorStatus::Unknown),
        version: row.try_get("version")?,
        config_dirtiness: row.try_get("config_dirtiness")?,
        last_policy_applied_at: row.try_get("last_policy_applied_at")?,
        last_seen: row.try_get("last_seen")?,
        reported: row.try_get("reported").unwrap_or(None),
    })
}

/// The supervisor that watches one sandbox, if it has one.
///
/// A sandbox is given its supervisor at creation (`create_for_sandbox`), so the
/// row is the console's way from a sandbox to the settings it may change. The
/// newest row wins when history left more than one, because that is the one the
/// running process is reporting through.
pub async fn supervisor_for_sandbox(
    scope: &mut TenantScope<'_>,
    sandbox_id: Uuid,
) -> Result<Supervisor, StoreError> {
    let sql = format!(
        "SELECT {SUPERVISOR_COLUMNS}
           FROM supervisors s
          WHERE s.sandbox_id = $1
          ORDER BY s.last_seen DESC NULLS LAST
          LIMIT 1"
    );
    let row = sqlx::query(&sql)
        .bind(sandbox_id)
        .fetch_optional(scope.connection())
        .await?
        .ok_or(StoreError::NotFound)?;
    row_to_supervisor(&row)
}

/// Store a supervisor's operator settings as the desired configuration.
///
/// The row keeps what the operator asked for; the process in the sandbox reads
/// the same document from its own disk. Writing here first means the console can
/// show the intent even when delivery failed, and a retry has something to send.
pub async fn set_supervisor_settings(
    scope: &mut TenantScope<'_>,
    supervisor_id: Uuid,
    settings: &serde_json::Value,
) -> Result<(), StoreError> {
    let affected = sqlx::query("UPDATE supervisors SET settings = $2 WHERE id = $1")
        .bind(supervisor_id)
        .bind(settings)
        .execute(scope.connection())
        .await?
        .rows_affected();
    if affected == 0 {
        return Err(StoreError::NotFound);
    }
    Ok(())
}

/// A supervisor's stored operator settings, or an empty document.
pub async fn supervisor_settings(
    scope: &mut TenantScope<'_>,
    supervisor_id: Uuid,
) -> Result<serde_json::Value, StoreError> {
    let settings: Option<serde_json::Value> =
        sqlx::query_scalar("SELECT settings FROM supervisors WHERE id = $1")
            .bind(supervisor_id)
            .fetch_optional(scope.connection())
            .await?
            .ok_or(StoreError::NotFound)?;
    Ok(settings.unwrap_or_else(|| serde_json::json!({})))
}

/// One supervisor by id, within the caller's environment.
pub async fn get_supervisor(
    scope: &mut TenantScope<'_>,
    supervisor_id: Uuid,
) -> Result<Supervisor, StoreError> {
    let sql = format!(
        "SELECT {SUPERVISOR_COLUMNS}
           FROM supervisors s
           JOIN gateways g ON g.id = s.gateway_id
           JOIN clusters c ON c.id = g.cluster_id
          WHERE c.environment_id = $1 AND s.id = $2"
    );
    let row = sqlx::query(&sql)
        .bind(scope.environment_id())
        .bind(supervisor_id)
        .fetch_optional(scope.connection())
        .await?
        .ok_or(StoreError::NotFound)?;
    row_to_supervisor(&row)
}

/// Delete the supervisors that belong to a sandbox.
///
/// The foreign key is `ON DELETE SET NULL`, which keeps a manually registered
/// supervisor's identity when its sandbox is gone. An automatically created one
/// has no meaning without its sandbox — a row that can never report again — so
/// it is removed with it.
pub async fn delete_supervisors_for_sandbox(
    scope: &mut TenantScope<'_>,
    sandbox_id: Uuid,
) -> Result<u64, StoreError> {
    let removed = sqlx::query("DELETE FROM supervisors WHERE sandbox_id = $1")
        .bind(sandbox_id)
        .execute(scope.connection())
        .await?
        .rows_affected();
    Ok(removed)
}
