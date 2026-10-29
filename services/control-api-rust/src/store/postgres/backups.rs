//! The catalogue of root backups. The archives are files on the node; this layer holds
//! only what was backed up, when, and its checksum.

use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::Row;
use utoipa::ToSchema;
use uuid::Uuid;

use super::super::TenantScope;
use crate::error::StoreError;

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct Backup {
    pub id: Uuid,
    pub node_id: Uuid,
    /// The root it was taken from; empty once that root is purged.
    pub root_id: Option<Uuid>,
    pub root_slug: String,
    pub root_kind: String,
    pub note: Option<String>,
    pub bytes: i64,
    pub sha256: String,
    pub created_at: DateTime<Utc>,
}

const COLUMNS: &str = "id, node_id, root_id, root_slug, root_kind, note, bytes, sha256, created_at";

fn row_to_backup(row: &sqlx::postgres::PgRow) -> Result<Backup, StoreError> {
    Ok(Backup {
        id: row.try_get("id")?,
        node_id: row.try_get("node_id")?,
        root_id: row.try_get("root_id")?,
        root_slug: row.try_get("root_slug")?,
        root_kind: row.try_get("root_kind")?,
        note: row.try_get("note")?,
        bytes: row.try_get("bytes")?,
        sha256: row.try_get("sha256")?,
        created_at: row.try_get("created_at")?,
    })
}

pub struct NewBackup<'a> {
    pub id: Uuid,
    pub node_id: Uuid,
    pub root_id: Uuid,
    pub root_slug: &'a str,
    pub root_kind: &'a str,
    pub note: Option<&'a str>,
    pub bytes: i64,
    pub sha256: &'a str,
    pub created_by: Uuid,
}

pub async fn insert(scope: &mut TenantScope<'_>, new: &NewBackup<'_>) -> Result<Backup, StoreError> {
    let environment_id = scope.environment_id();
    let row = sqlx::query(&format!(
        "INSERT INTO root_backups (id, environment_id, node_id, root_id, root_slug, root_kind, note, bytes, sha256, created_by)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10) RETURNING {COLUMNS}"
    ))
    .bind(new.id)
    .bind(environment_id)
    .bind(new.node_id)
    .bind(new.root_id)
    .bind(new.root_slug)
    .bind(new.root_kind)
    .bind(new.note)
    .bind(new.bytes)
    .bind(new.sha256)
    .bind(new.created_by)
    .fetch_one(scope.connection())
    .await?;
    row_to_backup(&row)
}

pub async fn list(scope: &mut TenantScope<'_>, root_id: Option<Uuid>) -> Result<Vec<Backup>, StoreError> {
    let rows = sqlx::query(&format!(
        "SELECT {COLUMNS} FROM root_backups WHERE ($1::uuid IS NULL OR root_id = $1) ORDER BY created_at DESC LIMIT 500"
    ))
    .bind(root_id)
    .fetch_all(scope.connection())
    .await?;
    rows.iter().map(row_to_backup).collect()
}

pub async fn get(scope: &mut TenantScope<'_>, id: Uuid) -> Result<Backup, StoreError> {
    let row = sqlx::query(&format!("SELECT {COLUMNS} FROM root_backups WHERE id = $1"))
        .bind(id)
        .fetch_optional(scope.connection())
        .await?
        .ok_or(StoreError::NotFound)?;
    row_to_backup(&row)
}

pub async fn delete(scope: &mut TenantScope<'_>, id: Uuid) -> Result<(), StoreError> {
    let affected = sqlx::query("DELETE FROM root_backups WHERE id = $1")
        .bind(id)
        .execute(scope.connection())
        .await?
        .rows_affected();
    if affected == 0 { Err(StoreError::NotFound) } else { Ok(()) }
}
