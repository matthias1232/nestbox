//! Stored secrets: ciphertext in, metadata out. The value never leaves this layer
//! in the clear; decryption belongs to the service that holds the master key.

use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::Row;
use utoipa::ToSchema;
use uuid::Uuid;

use super::super::TenantScope;
use crate::error::StoreError;

/// What the API may show about a stored secret: never the value.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct SecretMeta {
    pub id: Uuid,
    pub name: String,
    pub kind: String,
    /// The last four characters of the value.
    pub last4: String,
    /// The locator a provider uses to point at it.
    pub locator: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub last_used_at: Option<DateTime<Utc>>,
    /// How many providers use it.
    pub used_by: i64,
}

fn meta(row: &sqlx::postgres::PgRow) -> Result<SecretMeta, StoreError> {
    let name: String = row.try_get("name")?;
    Ok(SecretMeta {
        id: row.try_get("id")?,
        locator: format!("db://{name}"),
        name,
        kind: row.try_get("kind")?,
        last4: row.try_get("last4")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
        last_used_at: row.try_get("last_used_at")?,
        used_by: row.try_get("used_by")?,
    })
}

const COLUMNS: &str = "s.id, s.name, s.kind, s.last4, s.created_at, s.updated_at, s.last_used_at,
    (SELECT count(*) FROM provider_profiles p WHERE p.secret_ref = 'db://' || s.name) AS used_by";

pub async fn list(scope: &mut TenantScope<'_>) -> Result<Vec<SecretMeta>, StoreError> {
    let rows = sqlx::query(&format!("SELECT {COLUMNS} FROM stored_secrets s ORDER BY s.name"))
        .fetch_all(scope.connection())
        .await?;
    rows.iter().map(meta).collect()
}

/// Insert, or replace the value of an existing name (rotation keeps the name, so the
/// providers that point at it keep working).
pub async fn upsert(
    scope: &mut TenantScope<'_>,
    name: &str,
    kind: &str,
    ciphertext: &[u8],
    nonce: &[u8],
    last4: &str,
    created_by: Uuid,
) -> Result<SecretMeta, StoreError> {
    let environment_id = scope.environment_id();
    let row = sqlx::query(&format!(
        "WITH written AS (
            INSERT INTO stored_secrets (environment_id, name, kind, ciphertext, nonce, last4, created_by)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            ON CONFLICT (environment_id, name) DO UPDATE
               SET kind = EXCLUDED.kind, ciphertext = EXCLUDED.ciphertext, nonce = EXCLUDED.nonce,
                   last4 = EXCLUDED.last4, updated_at = now()
            RETURNING *
         ) SELECT {COLUMNS} FROM written s"
    ))
    .bind(environment_id)
    .bind(name)
    .bind(kind)
    .bind(ciphertext)
    .bind(nonce)
    .bind(last4)
    .bind(created_by)
    .fetch_one(scope.connection())
    .await?;
    meta(&row)
}

pub async fn delete(scope: &mut TenantScope<'_>, id: Uuid) -> Result<String, StoreError> {
    let name: Option<String> = sqlx::query_scalar("DELETE FROM stored_secrets WHERE id = $1 RETURNING name")
        .bind(id)
        .fetch_optional(scope.connection())
        .await?;
    name.ok_or(StoreError::NotFound)
}

/// The sealed value of one secret, by name, and the stamp that it was used.
pub async fn sealed(scope: &mut TenantScope<'_>, name: &str) -> Result<(Vec<u8>, Vec<u8>), StoreError> {
    let row = sqlx::query(
        "UPDATE stored_secrets SET last_used_at = now() WHERE name = $1 RETURNING ciphertext, nonce",
    )
    .bind(name)
    .fetch_optional(scope.connection())
    .await?
    .ok_or(StoreError::NotFound)?;
    Ok((row.try_get("ciphertext")?, row.try_get("nonce")?))
}

pub async fn providers_using(scope: &mut TenantScope<'_>, name: &str) -> Result<Vec<String>, StoreError> {
    let locator = format!("db://{name}");
    Ok(sqlx::query_scalar("SELECT name FROM provider_profiles WHERE secret_ref = $1 ORDER BY name")
        .bind(locator)
        .fetch_all(scope.connection())
        .await?)
}
