//! Backup and restore of persistent roots.
//!
//! A backup is a gzip tar of one root, written by the node's runner, with its SHA-256
//! recorded here. The rules that keep a restore from becoming the incident:
//!
//! * a restore checks the checksum on the node before it writes anything;
//! * a root that a *running* sandbox has mounted read-write is not restored into (the
//!   files would change under it) unless the caller says `force`;
//! * `replace` moves the current content aside instead of deleting it;
//! * a backup outlives its root, so an archived root can still be restored elsewhere.

use uuid::Uuid;

use super::{require, ServiceContext};
use crate::domain::auth::{Permission, Principal};
use crate::error::{ApiError, ErrorCode, StoreError};
use crate::store::TenantScope;
use crate::store::postgres::backups as store;
use crate::store::postgres::backups::Backup;
use crate::store::postgres::persistent_roots as roots;

fn runtime_error(error: crate::runtime::RuntimeError) -> ApiError {
    ApiError::conflict(ErrorCode::SANDBOX_INVALID_STATE, error.to_string())
}

fn not_found() -> ApiError {
    ApiError::not_found(ErrorCode::ROOT_NOT_FOUND, "No such backup in this environment.")
}

async fn audit(
    scope: &mut TenantScope<'_>,
    principal: &Principal,
    kind: &str,
    subject: Uuid,
    detail: serde_json::Value,
) -> Result<(), ApiError> {
    crate::store::postgres::audit::record(
        scope,
        crate::store::postgres::audit::NewAuditEvent {
            actor_id: Some(principal.user_id),
            actor_label: &principal.display_name,
            kind,
            subject_type: Some("root_backup"),
            subject_id: Some(subject),
            detail: &detail,
            request_id: None,
        },
    )
    .await
    .map_err(ApiError::from)
}

pub async fn list(ctx: &ServiceContext, principal: &Principal, root_id: Option<Uuid>) -> Result<Vec<Backup>, ApiError> {
    require(principal, Permission::SandboxRead)?;
    let mut scope = TenantScope::begin(&ctx.pool, principal.environment_id).await.map_err(ApiError::from)?;
    let items = store::list(&mut scope, root_id).await.map_err(ApiError::from)?;
    scope.commit().await.map_err(ApiError::from)?;
    Ok(items)
}

/// Take a backup of one root.
pub async fn create(
    ctx: &ServiceContext,
    principal: &Principal,
    root_id: Uuid,
    note: Option<&str>,
) -> Result<Backup, ApiError> {
    require(principal, Permission::PersistWrite)?;
    let note = note.map(str::trim).filter(|text| !text.is_empty());
    if note.is_some_and(|text| text.len() > 200) {
        return Err(ApiError::validation("A backup note is at most 200 characters."));
    }
    let mut scope = TenantScope::begin(&ctx.pool, principal.environment_id).await.map_err(ApiError::from)?;
    let root = roots::get_root(&mut scope, root_id).await.map_err(|error| match error {
        StoreError::NotFound => ApiError::not_found(ErrorCode::ROOT_NOT_FOUND, "No persistent root with that id."),
        other => ApiError::from(other),
    })?;
    scope.commit().await.map_err(ApiError::from)?;

    let id = Uuid::new_v4();
    let (bytes, sha256) = ctx
        .runtime
        .backup_create(&root.host_path, &id.to_string())
        .await
        .map_err(runtime_error)?;

    let mut scope = TenantScope::begin(&ctx.pool, principal.environment_id).await.map_err(ApiError::from)?;
    let backup = store::insert(
        &mut scope,
        &store::NewBackup {
            id,
            node_id: root.node_id,
            root_id: root.id,
            root_slug: &root.slug,
            root_kind: root.kind.as_str(),
            note,
            bytes,
            sha256: &sha256,
            created_by: principal.user_id,
        },
    )
    .await
    .map_err(ApiError::from)?;
    audit(&mut scope, principal, "root.backup", id, serde_json::json!({ "root": root.slug, "bytes": bytes })).await?;
    scope.commit().await.map_err(ApiError::from)?;
    Ok(backup)
}

/// Restore a backup into a root (by default the one it was taken from).
pub async fn restore(
    ctx: &ServiceContext,
    principal: &Principal,
    backup_id: Uuid,
    target_root: Option<Uuid>,
    replace: bool,
    force: bool,
) -> Result<serde_json::Value, ApiError> {
    require(principal, Permission::PersistWrite)?;
    let mut scope = TenantScope::begin(&ctx.pool, principal.environment_id).await.map_err(ApiError::from)?;
    let backup = store::get(&mut scope, backup_id).await.map_err(|error| match error {
        StoreError::NotFound => not_found(),
        other => ApiError::from(other),
    })?;
    let Some(target_id) = target_root.or(backup.root_id) else {
        return Err(ApiError::validation("The root this backup came from is gone: name a root to restore into."));
    };
    let root = roots::get_root(&mut scope, target_id).await.map_err(|error| match error {
        StoreError::NotFound => ApiError::not_found(ErrorCode::ROOT_NOT_FOUND, "No persistent root with that id."),
        other => ApiError::from(other),
    })?;
    if root.node_id != backup.node_id {
        return Err(ApiError::validation("A backup can only be restored on the node that holds its archive."));
    }
    let mounted = roots::mounts_for_root(&mut scope, root.id).await.map_err(ApiError::from)?;
    scope.commit().await.map_err(ApiError::from)?;

    if !force {
        let running = ctx.runtime.list().await.unwrap_or_default();
        for mount in mounted.iter().filter(|mount| !mount.read_only) {
            let up = running.iter().any(|fact| fact.name == mount.sandbox_name && fact.status == "running");
            if up {
                return Err(ApiError::conflict(
                    ErrorCode::MOUNT_CONFLICT,
                    format!(
                        "Sandbox {} is running with this root mounted read-write. Stop it first, or restore with force.",
                        mount.sandbox_name
                    ),
                ));
            }
        }
    }

    let answer = ctx
        .runtime
        .backup_restore(&backup.id.to_string(), &root.host_path, &backup.sha256, replace)
        .await
        .map_err(runtime_error)?;

    let mut scope = TenantScope::begin(&ctx.pool, principal.environment_id).await.map_err(ApiError::from)?;
    audit(
        &mut scope,
        principal,
        "root.restore",
        backup.id,
        serde_json::json!({ "into": root.slug, "replace": replace, "force": force }),
    )
    .await?;
    scope.commit().await.map_err(ApiError::from)?;
    Ok(answer)
}

/// Delete a backup and its archive.
pub async fn delete(ctx: &ServiceContext, principal: &Principal, backup_id: Uuid) -> Result<(), ApiError> {
    require(principal, Permission::PersistWrite)?;
    let mut scope = TenantScope::begin(&ctx.pool, principal.environment_id).await.map_err(ApiError::from)?;
    let backup = store::get(&mut scope, backup_id).await.map_err(|error| match error {
        StoreError::NotFound => not_found(),
        other => ApiError::from(other),
    })?;
    scope.commit().await.map_err(ApiError::from)?;
    // The archive first: a row without a file is harmless, a file without a row is a leak.
    ctx.runtime.backup_delete(&backup.id.to_string()).await.map_err(runtime_error)?;
    let mut scope = TenantScope::begin(&ctx.pool, principal.environment_id).await.map_err(ApiError::from)?;
    store::delete(&mut scope, backup_id).await.map_err(ApiError::from)?;
    audit(&mut scope, principal, "root.backup.delete", backup_id, serde_json::json!({ "root": backup.root_slug })).await?;
    scope.commit().await.map_err(ApiError::from)?;
    Ok(())
}
