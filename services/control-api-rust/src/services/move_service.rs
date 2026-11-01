//! Moving a sandbox, with its persistent data, to another node and back (ADR 0049).
//!
//! The move is a sequence of small, named steps, each written to `sandbox_moves.log`, so the
//! console can show where it is and an operator can see where a failed one stopped:
//!
//! 1. **check** - the target is another node this data center may use, and it answers;
//!    the sandbox's roots belong to this sandbox alone;
//! 2. **image** - the target holds the sandbox's image (pulled from the registry if not);
//! 3. **snapshot** - every root is snapshotted into the data center's restic repository while
//!    the sandbox still runs, so the stop that follows has little left to send;
//! 4. **stop** - the sandbox stops on the source, through the normal path, so the reconcile loop
//!    does not start it again;
//! 5. **delta** - a second snapshot of each root, now at rest;
//! 6. **restore** - the roots are put at the same paths on the target (what was there is set
//!    aside, never deleted);
//! 7. **cutover** - roots and sandbox now belong to the target, the source container goes;
//! 8. **recreate** - the sandbox is created on the target with the same identity, policies and
//!    mounts, and started if it was running;
//! 9. **verify** - the target reports it running.
//!
//! The source's files stay where they are as a cold copy. A failure before the cutover puts the
//! sandbox back on the source; a failure after it leaves the sandbox on the target and says so.
//! Because a root is owned by exactly one node row, there is never a moment in which two nodes
//! both claim to run it.

use chrono::{DateTime, Utc};
use serde::Serialize;
use utoipa::ToSchema;
use uuid::Uuid;

use super::sandbox_service::{self, LifecycleAction};
use super::{require, secret_service, ServiceContext};
use crate::config::Config;
use crate::domain::auth::{Permission, Principal};
use crate::domain::sandbox::SandboxStatus;
use crate::error::{ApiError, ErrorCode};
use crate::store::TenantScope;

fn db(error: sqlx::Error) -> ApiError {
    ApiError::from(crate::store::map_sqlx_error(error))
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct MoveView {
    pub id: Uuid,
    pub sandbox_id: Option<Uuid>,
    pub sandbox_name: String,
    pub from_node: String,
    pub to_node: String,
    /// `running`, `done` or `failed`.
    pub state: String,
    pub step: String,
    pub log: serde_json::Value,
    pub bytes_moved: i64,
    pub error: Option<String>,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
}

/// The restic repository of a data center, as the nodes reach it over the overlay.
pub fn repository_url(config: &Config, environment_id: Uuid) -> String {
    format!("rest:http://{}:8500/{}", config.overlay.hub_ip, environment_id)
}

/// A root can only move with a sandbox that is its sole user.
pub fn movable(sharing: &[(Uuid, i64)]) -> Result<(), String> {
    for (root, others) in sharing {
        if *others > 0 {
            return Err(format!(
                "The folder {root} is also mounted in another sandbox. Move them together or unmount it first."
            ));
        }
    }
    Ok(())
}

async fn note(ctx: &ServiceContext, move_id: Uuid, step: &str, detail: &str) {
    let entry = serde_json::json!([{ "step": step, "at": Utc::now(), "detail": detail }]);
    let _ = sqlx::query("UPDATE sandbox_moves SET step = $2, log = log || $3::jsonb WHERE id = $1")
        .bind(move_id)
        .bind(step)
        .bind(entry)
        .execute(&ctx.pool)
        .await;
}

async fn fail(ctx: &ServiceContext, move_id: Uuid, step: &str, error: &ApiError) {
    let _ = sqlx::query("UPDATE sandbox_moves SET state = 'failed', error = $2, finished_at = now() WHERE id = $1")
        .bind(move_id)
        .bind(format!("{step}: {}", error.message))
        .execute(&ctx.pool)
        .await;
    note(ctx, move_id, "failed", &error.message).await;
}

/// The password of the data center's repository, made on first use and kept in the vault.
pub(crate) async fn repository_password(ctx: &ServiceContext, principal: &Principal) -> Result<String, ApiError> {
    let name = "restic-repository";
    if let Ok(existing) = secret_service::reveal(ctx, principal.environment_id, name).await {
        return Ok(existing);
    }
    let fresh = crate::security::session::random_token();
    secret_service::put_system(ctx, principal.environment_id, principal.user_id, name, "password", &fresh).await?;
    Ok(fresh)
}

/// Start moving a sandbox. Returns at once; the move runs in the background and is followed
/// through `get` and `list`.
pub async fn start(
    ctx: &ServiceContext,
    config: &Config,
    principal: &Principal,
    sandbox_id: Uuid,
    target: &str,
) -> Result<MoveView, ApiError> {
    begin(ctx, config, principal, sandbox_id, target, false).await
}

/// Bring a sandbox back on another node after the node it ran on is gone (ADR 0049).
///
/// Only for a source node that is not online: the folders are restored from their newest scheduled
/// snapshot, so the work since that snapshot (at most the snapshot interval) is lost, and the log
/// says which snapshot was used. If the source node is online, use a normal move: it loses nothing.
pub async fn recover(
    ctx: &ServiceContext,
    config: &Config,
    principal: &Principal,
    sandbox_id: Uuid,
    target: &str,
) -> Result<MoveView, ApiError> {
    begin(ctx, config, principal, sandbox_id, target, true).await
}

async fn begin(
    ctx: &ServiceContext,
    config: &Config,
    principal: &Principal,
    sandbox_id: Uuid,
    target: &str,
    recovering: bool,
) -> Result<MoveView, ApiError> {
    require(principal, Permission::SandboxWrite)?;
    let Some(router) = ctx.router.clone() else {
        return Err(ApiError::unavailable(ErrorCode::NODE_RUNTIME_UNAVAILABLE, "This installation has no node dispatcher."));
    };
    let mut scope = TenantScope::begin_for(&ctx.pool, principal).await.map_err(ApiError::from)?;
    let sandbox = crate::store::postgres::sandboxes::get(&mut scope, sandbox_id).await.map_err(ApiError::from)?;
    let to = crate::store::postgres::nodes::get_by_name(&mut scope, target.trim()).await.map_err(|e| match e {
        crate::error::StoreError::NotFound => ApiError::not_found(ErrorCode::NODE_NOT_FOUND, format!("No node named '{target}'.")),
        other => ApiError::from(other),
    })?;
    if to.id == sandbox.node_id {
        return Err(ApiError::validation("The sandbox is already on that node."));
    }
    if to.status != crate::domain::node::NodeStatus::Online {
        return Err(ApiError::conflict(ErrorCode::NODE_RUNTIME_UNAVAILABLE, format!("Node '{}' is not online.", to.name)));
    }
    let from = crate::store::postgres::nodes::get(&mut scope, sandbox.node_id).await.map_err(ApiError::from)?;
    // The roots of this sandbox, and whether anyone else mounts them.
    let roots: Vec<(Uuid, String)> = sqlx::query_as(
        "SELECT r.id, r.host_path FROM persistent_roots r JOIN mounts m ON m.root_id = r.id
          WHERE m.sandbox_id = $1 AND m.status = 'mounted' AND r.archived_at IS NULL ORDER BY r.host_path",
    )
    .bind(sandbox.id)
    .fetch_all(scope.connection())
    .await
    .map_err(db)?;
    let mut sharing = Vec::new();
    for (root, _) in &roots {
        let others: i64 = sqlx::query_scalar("SELECT count(*) FROM mounts WHERE root_id = $1 AND sandbox_id <> $2 AND status = 'mounted'")
            .bind(root)
            .bind(sandbox.id)
            .fetch_one(scope.connection())
            .await
            .map_err(db)?;
        sharing.push((*root, others));
    }
    movable(&sharing).map_err(ApiError::validation)?;
    let mut latest: Vec<(String, String)> = Vec::new();
    if recovering {
        if from.status == crate::domain::node::NodeStatus::Online {
            return Err(ApiError::conflict(
                ErrorCode::SANDBOX_INVALID_STATE,
                format!("Node '{}' is online. Move the sandbox instead: a move loses nothing, a recovery loses the work since the last snapshot.", from.name),
            ));
        }
        for (root, path) in &roots {
            let newest: Option<String> = sqlx::query_scalar("SELECT snapshot FROM root_snapshots WHERE root_id = $1 ORDER BY taken_at DESC LIMIT 1")
                .bind(root)
                .fetch_optional(scope.connection())
                .await
                .map_err(db)?;
            let Some(snapshot) = newest else {
                return Err(ApiError::conflict(ErrorCode::SANDBOX_INVALID_STATE, format!("The folder {path} was never snapshotted, so there is nothing to recover from.")));
            };
            latest.push((path.clone(), snapshot));
        }
    } else if ![SandboxStatus::Running, SandboxStatus::Stopped].contains(&sandbox.status) {
        return Err(ApiError::conflict(ErrorCode::SANDBOX_INVALID_STATE, "Only a running or stopped sandbox can be moved."));
    }
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO sandbox_moves (environment_id, sandbox_id, sandbox_name, from_node_id, to_node_id, from_node_name, to_node_name, started_by)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8) RETURNING id",
    )
    .bind(scope.environment_id())
    .bind(sandbox.id)
    .bind(&sandbox.name)
    .bind(from.id)
    .bind(to.id)
    .bind(&from.name)
    .bind(&to.name)
    .bind(principal.user_id)
    .fetch_one(scope.connection())
    .await
    .map_err(|e| match crate::store::map_sqlx_error(e) {
        crate::error::StoreError::UniqueViolation(_) => ApiError::conflict(ErrorCode::SANDBOX_INVALID_STATE, "This sandbox is already being moved."),
        other => ApiError::from(other),
    })?;
    scope.commit().await.map_err(ApiError::from)?;

    let job = Job {
        id,
        sandbox_id: sandbox.id,
        name: sandbox.name.clone(),
        image: sandbox.image.clone(),
        was_running: recovering || sandbox.status == SandboxStatus::Running,
        recover: recovering,
        latest,
        from: from.id,
        to: to.id,
        roots,
        repo: repository_url(config, principal.environment_id),
    };
    let background = ctx.clone();
    let acting = principal.clone();
    tokio::spawn(async move {
        run(&background, &router, &acting, job).await;
    });
    get(ctx, principal, id).await
}

/// Everything the background task needs, decided before it starts.
struct Job {
    id: Uuid,
    sandbox_id: Uuid,
    name: String,
    image: String,
    was_running: bool,
    /// Restoring from the newest scheduled snapshots because the source is gone.
    recover: bool,
    latest: Vec<(String, String)>,
    from: Uuid,
    to: Uuid,
    roots: Vec<(Uuid, String)>,
    repo: String,
}

/// Run the steps in order; any error stops the move, records where, and undoes what can be undone.
async fn run(ctx: &ServiceContext, router: &crate::runtime::router::RuntimeRouter, principal: &Principal, job: Job) {
    let source = router.for_node(Some(job.from));
    let target = router.for_node(Some(job.to));
    let mut cut_over = false;
    let mut stopped = false;
    let outcome: Result<(), (String, ApiError)> = async {
        let step = |name: &str| name.to_string();
        let password = repository_password(ctx, principal).await.map_err(|e| (step("check"), e))?;
        note(ctx, job.id, "check", &format!("{} folder(s) travel with the sandbox", job.roots.len())).await;

        // The image: the target must hold the same bytes. A local target has no registry to pull
        // from and already has what the appliance built.
        note(ctx, job.id, "image", &job.image).await;
        if router.is_remote(job.to) {
            let held = target.images().await.map(|list| list.iter().any(|i| i.image == job.image)).unwrap_or(false);
            if !held {
                target.pull_image(&job.image).await.map_err(|e| (step("image"), ApiError::unavailable(ErrorCode::NODE_RUNTIME_UNAVAILABLE, format!("The target could not get the image {}: {e}", job.image))))?;
            }
        }

        let mut bytes = 0i64;
        let mut snapshots = Vec::new();
        let tags = vec![format!("sandbox:{}", job.name), format!("move:{}", job.id)];
        if job.recover {
            for (path, snapshot) in &job.latest {
                note(ctx, job.id, "recover", &format!("{path} from snapshot {snapshot}")).await;
                snapshots.push((path.clone(), snapshot.clone()));
            }
        }
        for (_, path) in job.roots.iter().filter(|_| !job.recover) {
            note(ctx, job.id, "snapshot", path).await;
            let fact = source.snapshot_backup(&job.repo, &password, path, &tags).await.map_err(|e| (step("snapshot"), ApiError::unavailable(ErrorCode::NODE_RUNTIME_UNAVAILABLE, e.to_string())))?;
            bytes += fact.bytes_added;
        }

        if job.was_running && !job.recover {
            note(ctx, job.id, "stop", "stopping on the source").await;
            sandbox_service::act(ctx, principal, job.sandbox_id, LifecycleAction::Stop).await.map_err(|e| (step("stop"), e))?;
            stopped = true;
        }
        for (_, path) in job.roots.iter().filter(|_| !job.recover) {
            note(ctx, job.id, "delta", path).await;
            let fact = source.snapshot_backup(&job.repo, &password, path, &tags).await.map_err(|e| (step("delta"), ApiError::unavailable(ErrorCode::NODE_RUNTIME_UNAVAILABLE, e.to_string())))?;
            bytes += fact.bytes_added;
            snapshots.push((path.clone(), fact.snapshot));
        }
        let _ = sqlx::query("UPDATE sandbox_moves SET bytes_moved = $2, snapshots = $3 WHERE id = $1")
            .bind(job.id)
            .bind(bytes)
            .bind(serde_json::json!(snapshots))
            .execute(&ctx.pool)
            .await;

        for (path, snapshot) in &snapshots {
            note(ctx, job.id, "restore", path).await;
            target.snapshot_restore(&job.repo, &password, snapshot, path, path).await.map_err(|e| (step("restore"), ApiError::unavailable(ErrorCode::NODE_RUNTIME_UNAVAILABLE, e.to_string())))?;
        }

        note(ctx, job.id, "cutover", "the folders and the sandbox now belong to the target").await;
        let _ = source.remove(&job.name).await;
        {
            let mut scope = TenantScope::begin_for(&ctx.pool, principal).await.map_err(|e| (step("cutover"), ApiError::from(e)))?;
            let ids: Vec<Uuid> = job.roots.iter().map(|r| r.0).collect();
            sqlx::query("UPDATE persistent_roots SET node_id = $2, updated_at = now() WHERE id = ANY($1)")
                .bind(&ids)
                .bind(job.to)
                .execute(scope.connection())
                .await
                .map_err(|e| (step("cutover"), db(e)))?;
            sqlx::query("UPDATE sandboxes SET node_id = $2, updated_at = now() WHERE id = $1")
                .bind(job.sandbox_id)
                .bind(job.to)
                .execute(scope.connection())
                .await
                .map_err(|e| (step("cutover"), db(e)))?;
            scope.commit().await.map_err(|e| (step("cutover"), ApiError::from(e)))?;
        }
        router.place(&job.name, Some(job.to));
        cut_over = true;

        note(ctx, job.id, "recreate", "creating the sandbox on the target").await;
        sandbox_service::recreate(ctx, principal, job.sandbox_id).await.map_err(|e| (step("recreate"), e))?;
        if !job.was_running {
            let _ = sandbox_service::act(ctx, principal, job.sandbox_id, LifecycleAction::Stop).await;
        }

        if job.was_running {
            note(ctx, job.id, "verify", "waiting for the target to report it running").await;
            let mut running = false;
            for _ in 0..30 {
                if let Ok(facts) = target.inspect(&job.name).await {
                    if facts.status == "running" {
                        running = true;
                        break;
                    }
                }
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            }
            if !running {
                return Err((step("verify"), ApiError::conflict(ErrorCode::SANDBOX_INVALID_STATE, "The target did not report the sandbox running within a minute.")));
            }
        }
        Ok(())
    }
    .await;

    match outcome {
        Ok(()) => {
            let _ = sqlx::query("UPDATE sandbox_moves SET state = 'done', step = 'done', finished_at = now() WHERE id = $1")
                .bind(job.id)
                .execute(&ctx.pool)
                .await;
            note(ctx, job.id, "done", "the source keeps its files as a cold copy").await;
        }
        Err((step, error)) => {
            tracing::warn!(move_id = %job.id, step = %step, code = error.code.0, "a sandbox move failed");
            fail(ctx, job.id, &step, &error).await;
            // Before the cutover the source is still the sandbox's home: start it again there.
            if !cut_over && stopped {
                let _ = sandbox_service::act(ctx, principal, job.sandbox_id, LifecycleAction::Start).await;
                note(ctx, job.id, "rollback", "started again on the source").await;
            }
        }
    }
}

/// One move, with node names.
pub async fn get(ctx: &ServiceContext, principal: &Principal, id: Uuid) -> Result<MoveView, ApiError> {
    require(principal, Permission::SandboxRead)?;
    list_where(ctx, principal, Some(id)).await?.into_iter().next().ok_or_else(|| ApiError::not_found(ErrorCode::SANDBOX_NOT_FOUND, "move"))
}

/// The recent moves of this data center that the caller may see.
pub async fn list(ctx: &ServiceContext, principal: &Principal) -> Result<Vec<MoveView>, ApiError> {
    require(principal, Permission::SandboxRead)?;
    list_where(ctx, principal, None).await
}

async fn list_where(ctx: &ServiceContext, principal: &Principal, only: Option<Uuid>) -> Result<Vec<MoveView>, ApiError> {
    let mut scope = TenantScope::begin_for(&ctx.pool, principal).await.map_err(ApiError::from)?;
    // A member sees the moves of sandboxes they can see: the join to `sandboxes` runs under their
    // row-level security. A move whose sandbox is gone is for administrators.
    let rows = sqlx::query_as::<_, (Uuid, Option<Uuid>, String, String, String, String, String, serde_json::Value, i64, Option<String>, DateTime<Utc>, Option<DateTime<Utc>>)>(
        "SELECT m.id, m.sandbox_id, m.sandbox_name, coalesce(f.name, m.from_node_name, '?'), coalesce(t.name, m.to_node_name, '?'), m.state, m.step, m.log, m.bytes_moved, m.error, m.started_at, m.finished_at
           FROM sandbox_moves m
           LEFT JOIN nodes f ON f.id = m.from_node_id LEFT JOIN nodes t ON t.id = m.to_node_id
          WHERE m.environment_id = $1 AND ($2::uuid IS NULL OR m.id = $2)
            AND ($3 OR EXISTS (SELECT 1 FROM sandboxes s WHERE s.id = m.sandbox_id))
          ORDER BY m.started_at DESC LIMIT 100",
    )
    .bind(scope.environment_id())
    .bind(only)
    .bind(principal.role == crate::domain::auth::Role::Admin)
    .fetch_all(scope.connection())
    .await
    .map_err(db)?;
    scope.commit().await.map_err(ApiError::from)?;
    Ok(rows
        .into_iter()
        .map(|(id, sandbox_id, sandbox_name, from_node, to_node, state, step, log, bytes_moved, error, started_at, finished_at)| MoveView {
            id, sandbox_id, sandbox_name, from_node, to_node, state, step, log, bytes_moved, error, started_at, finished_at,
        })
        .collect())
}

/// A move that was running when the process died will never finish: say so. The sandbox is where
/// the last completed step left it, which the log shows.
pub async fn settle_interrupted(ctx: &ServiceContext) -> Result<u64, ApiError> {
    let done = sqlx::query(
        "UPDATE sandbox_moves SET state = 'failed', finished_at = now(),
                error = 'The control plane restarted while this move ran. Check where the sandbox is and move it again.'
          WHERE state = 'running'",
    )
    .execute(&ctx.pool)
    .await
    .map_err(db)?;
    Ok(done.rows_affected())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_folder_shared_with_another_sandbox_does_not_move() {
        assert!(movable(&[(Uuid::nil(), 0)]).is_ok());
        assert!(movable(&[(Uuid::nil(), 0), (Uuid::nil(), 2)]).is_err());
        assert!(movable(&[]).is_ok());
    }

    #[test]
    fn the_repository_of_a_data_center_is_its_own_path_on_the_hub() {
        let mut config = crate::config::Config {
            database_url: String::new(),
            listen_addr: String::new(),
            secret_dir: Default::default(),
            session_cookie_secure: false,
            session_idle_seconds: 1,
            session_max_seconds: 1,
            allowed_origins: Vec::new(),
            runner_url: String::new(),
            runtime_network: String::new(),
            bootstrap_admin: false,
            body_limit_bytes: 1,
            rate_limit_per_minute: 1,
            terminal_idle_seconds: 1,
            deploy_mode: crate::config::DeployMode::SelfHosted,
            public_url: String::new(),
            oidc: Vec::new(),
            overlay: crate::config::OverlayConfig {
                dir: Default::default(),
                endpoint: String::new(),
                hub_ip: "10.77.0.1".into(),
                registry: String::new(),
                control_url: String::new(),
            },
        };
        config.overlay.hub_ip = "10.77.0.1".into();
        let env = Uuid::nil();
        assert_eq!(repository_url(&config, env), format!("rest:http://10.77.0.1:8500/{env}"));
    }
}
