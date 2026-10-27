//! Persistent roots and mounts: what survives when the container goes.
//!
//! A root is a directory on a node's filesystem; a mount binds it into a
//! sandbox. Four rules, and each one is silent data loss when it is missing:
//!
//! * **The host path is derived, never supplied.** It is
//!   `<base>/<owner>/<kind>--<slug>` from a validated slug, the caller's own
//!   user id, and the base the runner also uses. A request that could name a
//!   path could name another user's home.
//! * **A root mounts only into a sandbox on its own node.** The bind source is
//!   a path on that node; a sandbox elsewhere would look at a path that means
//!   something else or nothing.
//! * **One read-write mount per root.** Two containers writing the same files
//!   lose data without an error, so the second request is refused and names the
//!   sandbox that holds the root. Read-only sharing stays unlimited.
//! * **Archive, do not delete.** Deleting a record is not deleting the data:
//!   the directory lives on the node, which the control plane cannot reach. The
//!   response says which of the two happened.

use chrono::{DateTime, Utc};
use uuid::Uuid;

use super::{require, ServiceContext};
use crate::domain::auth::{Permission, Principal};
use crate::domain::node::Node;
use crate::domain::persist::{
    self, Mount, MountRef, MountSpec, PersistentRoot, RootKind, RootSlug,
};
use crate::domain::sandbox::Sandbox;
use crate::error::{ApiError, ErrorCode, StoreError};
use crate::store::TenantScope;
use crate::store::postgres::nodes as node_store;
use crate::store::postgres::persistent_roots as store;
use crate::store::postgres::sandboxes as sandbox_store;

/// The directory under which every root lives when the environment names none.
///
/// The same default the runner uses (`NESTBOX_PERSISTENT_BASE`), so the path the
/// control plane derives and the prefix the runner enforces cannot disagree.
const DEFAULT_BASE: &str = "/var/lib/nestbox/persistent";

/// The base directory for persistent roots on a node.
///
/// Read at call time rather than cached, so a deployment changes it in one place
/// (the compose file) and a test can set it. An unset, empty, or relative value
/// falls back to the default: a relative path would make the derived path
/// meaningless and the runner would refuse it.
pub fn persistent_base() -> String {
    base_from(std::env::var("NESTBOX_PERSISTENT_BASE").ok().as_deref())
}

/// The base from one environment value, with the fallback applied.
fn base_from(value: Option<&str>) -> String {
    let Some(value) = value else {
        return DEFAULT_BASE.to_string();
    };
    let trimmed = value.trim().trim_end_matches('/');
    if trimmed.is_empty() || !trimmed.starts_with('/') {
        return DEFAULT_BASE.to_string();
    }
    trimmed.to_string()
}

/// What a create request carries.
///
/// There is no host path field, deliberately: the path is computed from these
/// parts so a client cannot choose a directory.
pub struct NewRoot {
    /// `agent_home` or `project`.
    pub kind: String,
    /// Node by name; empty or absent means the local node.
    pub node: Option<String>,
    pub slug: String,
}

/// What a mount request carries.
pub struct MountRequest {
    pub sandbox_id: Uuid,
    pub mount_path: String,
    pub read_only: bool,
}

/// The outcome of archiving or purging a root.
///
/// Carries the host path so the client can show where the data still is: a
/// purged root leaves the directory on the node behind.
#[derive(Debug, Clone, serde::Serialize, utoipa::ToSchema)]
pub struct RootRemoval {
    pub id: Uuid,
    /// True when the row was archived; false when it was deleted.
    pub archived: bool,
    pub host_path: String,
    pub note: String,
}

/// The measured disk usage of a root, or an honest "not measured".
#[derive(Debug, Clone, serde::Serialize, utoipa::ToSchema)]
pub struct RootUsage {
    pub root_id: Uuid,
    /// Whether a measurement exists at all.
    pub measured: bool,
    /// `None` when the root has never been measured; never a fabricated zero.
    pub bytes_used: Option<i64>,
    pub measured_at: Option<DateTime<Utc>>,
    pub note: String,
}

/// Create a root on a node: derive the path, then store it.
pub async fn create(
    ctx: &ServiceContext,
    principal: &Principal,
    request: NewRoot,
) -> Result<PersistentRoot, ApiError> {
    require(principal, Permission::PersistWrite)?;

    let kind = RootKind::parse(&request.kind).ok_or_else(|| {
        ApiError::validation("Unknown root kind; expected 'agent_home' or 'project'.")
    })?;
    let slug = RootSlug::new(&request.slug).map_err(ApiError::validation)?;

    // The owner segment is the user id, not the display name: a rename must not
    // move a directory that already holds data.
    let host_path = persist::host_path(
        &persistent_base(),
        &principal.user_id.to_string(),
        kind,
        &slug,
    );

    let mut scope = TenantScope::begin(&ctx.pool, principal.environment_id)
        .await
        .map_err(ApiError::from)?;
    let node = resolve_node(&mut scope, request.node.as_deref()).await?;

    let root = store::insert_root(
        &mut scope,
        store::NewRoot {
            node_id: node.id,
            kind,
            owner_user_id: Some(principal.user_id),
            slug: slug.as_str(),
            host_path: &host_path,
        },
    )
    .await
    .map_err(|error| match error {
        StoreError::UniqueViolation(_) => duplicate_root(slug.as_str(), &node.name),
        other => ApiError::from(other),
    })?;

    record_audit(
        &mut scope,
        principal,
        "root.create",
        root.id,
        serde_json::json!({
            "slug": root.slug,
            "kind": root.kind.as_str(),
            "node": node.name,
            "host_path": root.host_path,
        }),
    )
    .await?;
    scope.commit().await.map_err(ApiError::from)?;
    Ok(root)
}

/// Every root in the environment, with where each one is mounted.
///
/// The mounts are read per root. A root list is one owner's projects, so the
/// count is small and a second query per row is cheaper than a grouped join
/// whose result would have to be untangled in the mapping.
pub async fn list(
    ctx: &ServiceContext,
    principal: &Principal,
) -> Result<Vec<PersistentRoot>, ApiError> {
    require(principal, Permission::SandboxRead)?;

    let mut scope = TenantScope::begin(&ctx.pool, principal.environment_id)
        .await
        .map_err(ApiError::from)?;
    let mut roots = store::list_roots(&mut scope).await.map_err(ApiError::from)?;
    for root in &mut roots {
        root.mounted_in = store::mounts_for_root(&mut scope, root.id)
            .await
            .map_err(ApiError::from)?;
    }
    scope.commit().await.map_err(ApiError::from)?;
    Ok(roots)
}

/// One root, with where it is mounted.
pub async fn get(
    ctx: &ServiceContext,
    principal: &Principal,
    id: Uuid,
) -> Result<PersistentRoot, ApiError> {
    require(principal, Permission::SandboxRead)?;

    let mut scope = TenantScope::begin(&ctx.pool, principal.environment_id)
        .await
        .map_err(ApiError::from)?;
    let mut root = root_or_not_found(&mut scope, id).await?;
    root.mounted_in = store::mounts_for_root(&mut scope, id)
        .await
        .map_err(ApiError::from)?;
    scope.commit().await.map_err(ApiError::from)?;
    Ok(root)
}

/// Archive a root (the default, data stays) or purge its record.
///
/// A mounted root is refused: archiving a root that a container has bound would
/// hide the directory the container is still writing to.
pub async fn delete(
    ctx: &ServiceContext,
    principal: &Principal,
    id: Uuid,
    purge: bool,
) -> Result<RootRemoval, ApiError> {
    require(principal, Permission::PersistWrite)?;

    let mut scope = TenantScope::begin(&ctx.pool, principal.environment_id)
        .await
        .map_err(ApiError::from)?;
    let root = root_or_not_found(&mut scope, id).await?;
    let mounted = store::mounts_for_root(&mut scope, id)
        .await
        .map_err(ApiError::from)?;
    if !mounted.is_empty() {
        return Err(not_dismounted(&root, &mounted));
    }

    store::remove_root(&mut scope, id, !purge)
        .await
        .map_err(ApiError::from)?;
    record_audit(
        &mut scope,
        principal,
        if purge { "root.purge" } else { "root.archive" },
        id,
        serde_json::json!({ "slug": root.slug, "host_path": root.host_path }),
    )
    .await?;
    scope.commit().await.map_err(ApiError::from)?;
    Ok(removal(&root, purge))
}

/// Mount a root into a sandbox.
///
/// The refusal for a second read-write mount is a 409 that names the sandbox
/// holding it; a root and a sandbox on different nodes is a 422, because the
/// request is well formed and simply cannot be satisfied.
pub async fn mount(
    ctx: &ServiceContext,
    principal: &Principal,
    id: Uuid,
    request: MountRequest,
) -> Result<Mount, ApiError> {
    require(principal, Permission::PersistWrite)?;

    let mount_path = clean_mount_path(&request.mount_path)?;

    let mut scope = TenantScope::begin(&ctx.pool, principal.environment_id)
        .await
        .map_err(ApiError::from)?;
    let root = root_or_not_found(&mut scope, id).await?;
    if root.archived_at.is_some() {
        return Err(ApiError::not_found(
            ErrorCode::ROOT_NOT_FOUND,
            "That root is archived and cannot take a new mount.",
        ));
    }
    let sandbox = sandbox_or_not_found(&mut scope, request.sandbox_id).await?;

    if !same_node(root.node_id, sandbox.node_id) {
        return Err(cross_node_refusal(&mut scope, &root, &sandbox).await);
    }

    if !request.read_only {
        let mounted = store::mounts_for_root(&mut scope, root.id)
            .await
            .map_err(ApiError::from)?;
        if let Some(held) = read_write_holder(&mounted, sandbox.id) {
            return Err(read_write_conflict(held));
        }
    }

    let mount = store::insert_mount(
        &mut scope,
        root.id,
        sandbox.id,
        &mount_path,
        request.read_only,
    )
    .await
    .map_err(|error| match error {
        // The partial unique index, not the check above, is what makes the rule
        // hold: two requests can pass the check at the same time.
        StoreError::UniqueViolation(_) => ApiError::conflict(
            ErrorCode::MOUNT_CONFLICT,
            "The root was taken read-write by another sandbox a moment ago. Reload and try again.",
        ),
        other => ApiError::from(other),
    })?;

    record_audit(
        &mut scope,
        principal,
        "root.mount",
        root.id,
        serde_json::json!({
            "sandbox_id": sandbox.id,
            "sandbox": sandbox.name,
            "mount_path": mount.mount_path,
            "read_only": mount.read_only,
        }),
    )
    .await?;
    scope.commit().await.map_err(ApiError::from)?;
    Ok(mount)
}

/// Mark a mount of this root in this sandbox as dismounted.
///
/// The container is not changed here: a bind mount is fixed at container
/// creation. The row records that the next container for this sandbox must not
/// receive the bind.
pub async fn dismount(
    ctx: &ServiceContext,
    principal: &Principal,
    id: Uuid,
    sandbox_id: Uuid,
) -> Result<(), ApiError> {
    require(principal, Permission::PersistWrite)?;

    let mut scope = TenantScope::begin(&ctx.pool, principal.environment_id)
        .await
        .map_err(ApiError::from)?;
    let root = root_or_not_found(&mut scope, id).await?;
    store::dismount(&mut scope, root.id, sandbox_id)
        .await
        .map_err(|error| match error {
            StoreError::NotFound => ApiError::not_found(
                ErrorCode::MOUNT_NOT_FOUND,
                "That root is not mounted in that sandbox.",
            ),
            other => ApiError::from(other),
        })?;
    record_audit(
        &mut scope,
        principal,
        "root.dismount",
        root.id,
        serde_json::json!({ "sandbox_id": sandbox_id }),
    )
    .await?;
    scope.commit().await.map_err(ApiError::from)?;
    Ok(())
}

/// The stored measurement of a root, or an honest "never measured".
///
/// `bytes_used` defaults to zero in the table, which is indistinguishable from a
/// measured empty directory. Without `measured_at` there is no measurement, so
/// no number is reported.
pub async fn usage(
    ctx: &ServiceContext,
    principal: &Principal,
    id: Uuid,
) -> Result<RootUsage, ApiError> {
    require(principal, Permission::SandboxRead)?;

    let mut scope = TenantScope::begin(&ctx.pool, principal.environment_id)
        .await
        .map_err(ApiError::from)?;
    let root = root_or_not_found(&mut scope, id).await?;
    scope.commit().await.map_err(ApiError::from)?;
    Ok(usage_of(&root))
}

/// The bind mounts a sandbox's container must be created with.
///
/// Takes the caller's transaction because the sandbox row is normally inserted
/// in that same transaction moments before the container is created; a separate
/// scope would not see it. The source is the root's stored host path, so a
/// request cannot supply one.
pub async fn mount_specs_for_sandbox(
    scope: &mut TenantScope<'_>,
    sandbox_id: Uuid,
) -> Result<Vec<MountSpec>, ApiError> {
    let rows = store::mounts_for_sandbox(scope, sandbox_id)
        .await
        .map_err(ApiError::from)?;
    Ok(mount_specs(&rows))
}

/// Map stored mounts into the binds the runtime receives.
fn mount_specs(rows: &[(Mount, String)]) -> Vec<MountSpec> {
    rows.iter()
        .map(|(mount, host_path)| MountSpec {
            source: host_path.clone(),
            target: mount.mount_path.clone(),
            read_only: mount.read_only,
        })
        .collect()
}

/// The node a root lives on, resolved by name; the local node when none is
/// named. A typo is a 404 rather than a silent fallback to `local`.
async fn resolve_node(scope: &mut TenantScope<'_>, name: Option<&str>) -> Result<Node, ApiError> {
    match name.map(str::trim).filter(|value| !value.is_empty()) {
        Some(name) => node_store::get_by_name(scope, name)
            .await
            .map_err(|error| match error {
                StoreError::NotFound => ApiError::not_found(
                    ErrorCode::NODE_NOT_FOUND,
                    format!("No node named '{name}'."),
                ),
                other => ApiError::from(other),
            }),
        None => node_store::local(scope).await.map_err(ApiError::from),
    }
}

/// Read a root, mapping a missing row to `root.not_found`.
async fn root_or_not_found(
    scope: &mut TenantScope<'_>,
    id: Uuid,
) -> Result<PersistentRoot, ApiError> {
    store::get_root(scope, id).await.map_err(|error| match error {
        StoreError::NotFound => ApiError::not_found(
            ErrorCode::ROOT_NOT_FOUND,
            "No persistent root with that id.",
        ),
        other => ApiError::from(other),
    })
}

/// Read a sandbox, mapping a missing row to `sandbox.not_found`.
async fn sandbox_or_not_found(
    scope: &mut TenantScope<'_>,
    id: Uuid,
) -> Result<Sandbox, ApiError> {
    sandbox_store::get(scope, id)
        .await
        .map_err(|error| match error {
            StoreError::NotFound => ApiError::not_found(
                ErrorCode::SANDBOX_NOT_FOUND,
                "No sandbox with that id.",
            ),
            other => ApiError::from(other),
        })
}

/// Whether a root and a sandbox are on the same node.
///
/// Pure and named, so the rule is one place to read and one line to test: a
/// bind source is a path on one machine.
fn same_node(root_node_id: Uuid, sandbox_node_id: Uuid) -> bool {
    root_node_id == sandbox_node_id
}

/// The refusal for a mount across nodes, with both node names.
///
/// Names rather than ids: the operator picked the node by name, and an id in an
/// error message is a support ticket.
async fn cross_node_refusal(
    scope: &mut TenantScope<'_>,
    root: &PersistentRoot,
    sandbox: &Sandbox,
) -> ApiError {
    let root_node = node_label(scope, root.node_id, None).await;
    let sandbox_node = node_label(scope, sandbox.node_id, sandbox.node_name.clone()).await;
    ApiError::unprocessable(
        ErrorCode::MOUNT_CONFLICT,
        format!(
            "A root mounts only into a sandbox on the same node: the root is on '{root_node}', the sandbox '{name}' is on '{sandbox_node}'.",
            name = sandbox.name
        ),
    )
}

/// A node's name, falling back to its id when the row cannot be read.
async fn node_label(
    scope: &mut TenantScope<'_>,
    node_id: Uuid,
    known: Option<String>,
) -> String {
    if let Some(known) = known.filter(|value| !value.is_empty()) {
        return known;
    }
    match node_store::get(scope, node_id).await {
        Ok(node) => node.name,
        Err(_) => node_id.to_string(),
    }
}

/// The mount that already holds a root read-write for another sandbox.
fn read_write_holder(mounts: &[MountRef], sandbox_id: Uuid) -> Option<&MountRef> {
    mounts
        .iter()
        .find(|mount| !mount.read_only && mount.sandbox_id != sandbox_id)
}

/// The 409 for a second read-write mount, naming the sandbox that holds it.
fn read_write_conflict(held: &MountRef) -> ApiError {
    ApiError::conflict(
        ErrorCode::MOUNT_CONFLICT,
        format!(
            "The root is already mounted read-write in sandbox '{}'. Dismount it there first, or mount it read-only.",
            held.sandbox_name
        ),
    )
    .with_details(serde_json::json!({
        "sandbox_id": held.sandbox_id,
        "sandbox": held.sandbox_name,
        "mount_path": held.mount_path,
    }))
}

/// The 409 for archiving or purging a root that a container still holds.
fn not_dismounted(root: &PersistentRoot, mounted: &[MountRef]) -> ApiError {
    let names: Vec<&str> = mounted
        .iter()
        .map(|mount| mount.sandbox_name.as_str())
        .collect();
    ApiError::conflict(
        ErrorCode::ROOT_IN_USE,
        format!(
            "The root '{}' is mounted in {}. Dismount it first; the data stays on the node either way.",
            root.slug,
            names.join(", ")
        ),
    )
    .with_details(serde_json::json!({ "sandboxes": names }))
}

/// The 409 for a second root with the same slug on the same node and owner.
fn duplicate_root(slug: &str, node: &str) -> ApiError {
    ApiError::conflict(
        ErrorCode::CONFLICT,
        format!("A root with slug '{slug}' already exists for this user on node '{node}'."),
    )
}

/// A mount target: absolute, no `..`, not `/`, without a trailing slash.
fn clean_mount_path(value: &str) -> Result<String, ApiError> {
    persist::validate_mount_path(value).map_err(ApiError::validation)
}

/// The answer to a delete, derived from what happened to the data.
fn removal(root: &PersistentRoot, purge: bool) -> RootRemoval {
    RootRemoval {
        id: root.id,
        archived: !purge,
        host_path: root.host_path.clone(),
        note: if purge {
            "The record was deleted. The directory on the node was not touched: the control plane has no access to the node's filesystem."
        } else {
            "The root is archived and hidden from the list. Its directory and data are unchanged."
        }
        .to_string(),
    }
}

/// The usage answer for a stored root, never inventing a number.
fn usage_of(root: &PersistentRoot) -> RootUsage {
    match root.measured_at {
        Some(at) => RootUsage {
            root_id: root.id,
            measured: true,
            bytes_used: Some(root.bytes_used),
            measured_at: Some(at),
            note: "Measured on the node; bytes_used is the size of the root's directory tree."
                .to_string(),
        },
        None => RootUsage {
            root_id: root.id,
            measured: false,
            bytes_used: None,
            measured_at: None,
            note: "This root has never been measured, so there is no number to report."
                .to_string(),
        },
    }
}

/// Record an audit event for a root change.
async fn record_audit(
    scope: &mut TenantScope<'_>,
    principal: &Principal,
    kind: &str,
    subject_id: Uuid,
    detail: serde_json::Value,
) -> Result<(), ApiError> {
    crate::store::postgres::audit::record(
        scope,
        crate::store::postgres::audit::NewAuditEvent {
            actor_id: Some(principal.user_id),
            actor_label: &principal.display_name,
            kind,
            subject_type: Some("persistent_root"),
            subject_id: Some(subject_id),
            detail: &detail,
            request_id: None,
        },
    )
    .await
    .map_err(ApiError::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root(id: Uuid, node_id: Uuid) -> PersistentRoot {
        PersistentRoot {
            id,
            environment_id: Uuid::nil(),
            node_id,
            kind: RootKind::Project,
            owner_user_id: None,
            slug: "alpha".into(),
            host_path: "/var/lib/nestbox/persistent/u/project--alpha".into(),
            bytes_used: 0,
            measured_at: None,
            archived_at: None,
            created_at: Utc::now(),
            mounted_in: Vec::new(),
        }
    }

    fn mount_ref(sandbox_id: Uuid, name: &str, read_only: bool) -> MountRef {
        MountRef {
            sandbox_id,
            sandbox_name: name.into(),
            mount_path: "/workspace".into(),
            read_only,
        }
    }

    #[test]
    fn an_unset_or_relative_base_falls_back_to_the_runner_default() {
        assert_eq!(base_from(None), DEFAULT_BASE);
        assert_eq!(base_from(Some("")), DEFAULT_BASE);
        assert_eq!(base_from(Some("var/lib/nestbox")), DEFAULT_BASE);
        assert_eq!(base_from(Some("/srv/persistent/")), "/srv/persistent");
    }

    #[test]
    fn a_mount_path_must_be_absolute_clean_and_not_the_root() {
        assert_eq!(
            clean_mount_path("/workspace/project/").ok().as_deref(),
            Some("/workspace/project")
        );
        for bad in ["workspace", "/a/../b", "/", " / "] {
            let error = clean_mount_path(bad).expect_err("must be refused");
            assert_eq!(error.code, ErrorCode::VALIDATION_FAILED);
        }
    }

    #[test]
    fn a_root_and_a_sandbox_on_different_nodes_do_not_connect() {
        let node = Uuid::new_v4();
        assert!(same_node(node, node));
        assert!(!same_node(node, Uuid::new_v4()));
    }

    #[test]
    fn a_second_read_write_mount_is_refused_with_the_sandbox_holding_it() {
        let holder = Uuid::new_v4();
        let mounts = vec![
            mount_ref(holder, "writer", false),
            mount_ref(Uuid::new_v4(), "reader", true),
        ];
        let held = read_write_holder(&mounts, Uuid::new_v4()).expect("a holder");
        assert_eq!(held.sandbox_id, holder);

        // The same sandbox may re-mount its own root at another path: that is an
        // update of its own row, not a conflict.
        assert!(read_write_holder(&mounts, holder).is_none());

        let error = read_write_conflict(held);
        assert_eq!(error.status, axum::http::StatusCode::CONFLICT);
        assert_eq!(error.code, ErrorCode::MOUNT_CONFLICT);
        assert!(error.message.contains("writer"));

        // Read-only mounts never block anything.
        let only_readers = vec![mount_ref(Uuid::new_v4(), "reader", true)];
        assert!(read_write_holder(&only_readers, Uuid::new_v4()).is_none());
    }

    #[test]
    fn a_duplicate_slug_is_a_conflict_that_names_the_slug_and_node() {
        let error = duplicate_root("alpha", "local");
        assert_eq!(error.status, axum::http::StatusCode::CONFLICT);
        assert!(error.message.contains("alpha"));
        assert!(error.message.contains("local"));
    }

    #[test]
    fn a_root_that_is_still_mounted_is_not_removed() {
        let mounted = vec![mount_ref(Uuid::new_v4(), "work", false)];
        let error = not_dismounted(&root(Uuid::nil(), Uuid::nil()), &mounted);
        assert_eq!(error.status, axum::http::StatusCode::CONFLICT);
        assert_eq!(error.code, ErrorCode::ROOT_IN_USE);
        assert!(error.message.contains("work"));
    }

    #[test]
    fn a_mount_never_reports_a_number_it_was_not_given() {
        let unmeasured = usage_of(&root(Uuid::nil(), Uuid::nil()));
        assert!(!unmeasured.measured);
        assert_eq!(unmeasured.bytes_used, None);
        assert!(unmeasured.note.contains("never been measured"));

        let mut measured = root(Uuid::nil(), Uuid::nil());
        measured.bytes_used = 4096;
        measured.measured_at = Some(Utc::now());
        let answer = usage_of(&measured);
        assert!(answer.measured);
        assert_eq!(answer.bytes_used, Some(4096));
    }

    #[test]
    fn deleting_a_record_keeps_the_data_and_says_which_happened() {
        let sample = root(Uuid::nil(), Uuid::nil());
        let archived = removal(&sample, false);
        assert!(archived.archived);
        assert_eq!(archived.host_path, sample.host_path);
        assert!(archived.note.contains("archived"));

        let purged = removal(&sample, true);
        assert!(!purged.archived);
        assert!(purged.note.contains("was not touched"));
    }

    #[test]
    fn the_runtime_receives_the_stored_source_and_target() {
        let mount = Mount {
            id: Uuid::nil(),
            root_id: Uuid::new_v4(),
            sandbox_id: Uuid::new_v4(),
            mount_path: "/workspace".into(),
            read_only: true,
            status: crate::domain::persist::MountStatus::Mounted,
            created_at: Utc::now(),
        };
        let specs = mount_specs(&[(mount, "/var/lib/nestbox/persistent/u/x".into())]);
        assert_eq!(
            specs,
            vec![MountSpec {
                source: "/var/lib/nestbox/persistent/u/x".into(),
                target: "/workspace".into(),
                read_only: true,
            }]
        );
    }
}
