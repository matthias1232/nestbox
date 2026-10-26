//! Provider profiles and secret references.

use sqlx::Row;
use uuid::Uuid;

use super::super::TenantScope;
use crate::domain::provider::{ProviderKind, ProviderProfile, SecretKind, SecretReference};
use crate::error::StoreError;

const PROFILE_COLUMNS: &str = "id, environment_id, name, kind, endpoint, model, secret_ref, \
     credential_expiry, labels, version, created_at, updated_at";

fn row_to_profile(row: &sqlx::postgres::PgRow) -> Result<ProviderProfile, StoreError> {
    let kind_raw: String = row.try_get("kind")?;
    let secret_ref: Option<String> = row.try_get("secret_ref")?;
    Ok(ProviderProfile {
        id: row.try_get("id")?,
        environment_id: row.try_get("environment_id")?,
        name: row.try_get("name")?,
        kind: ProviderKind::parse(&kind_raw).unwrap_or(ProviderKind::OpenaiCompatible),
        endpoint: row.try_get("endpoint")?,
        model: row.try_get("model")?,
        // The response reports whether a credential is configured, never a
        // value. A locator present means configured.
        credential_configured: secret_ref.is_some(),
        secret_ref,
        credential_expiry: row.try_get("credential_expiry")?,
        labels: row.try_get("labels")?,
        version: row.try_get("version")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

/// All provider profiles in this environment.
pub async fn list(scope: &mut TenantScope<'_>) -> Result<Vec<ProviderProfile>, StoreError> {
    let sql = format!(
        "SELECT {PROFILE_COLUMNS} FROM provider_profiles
          WHERE environment_id = $1 ORDER BY name"
    );
    let rows = sqlx::query(&sql)
        .bind(scope.environment_id())
        .fetch_all(scope.connection())
        .await?;
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        out.push(row_to_profile(&row)?);
    }
    Ok(out)
}

/// One provider profile.
pub async fn get(scope: &mut TenantScope<'_>, id: Uuid) -> Result<ProviderProfile, StoreError> {
    let sql = format!(
        "SELECT {PROFILE_COLUMNS} FROM provider_profiles
          WHERE environment_id = $1 AND id = $2"
    );
    let row = sqlx::query(&sql)
        .bind(scope.environment_id())
        .bind(id)
        .fetch_optional(scope.connection())
        .await?
        .ok_or(StoreError::NotFound)?;
    row_to_profile(&row)
}

/// Insert a provider profile.
pub struct NewProviderProfile<'a> {
    pub name: &'a str,
    pub kind: ProviderKind,
    pub endpoint: &'a str,
    pub model: Option<&'a str>,
    pub secret_ref: Option<&'a str>,
    pub labels: &'a serde_json::Value,
}

pub async fn insert(
    scope: &mut TenantScope<'_>,
    new: NewProviderProfile<'_>,
) -> Result<ProviderProfile, StoreError> {
    let sql = format!(
        "WITH inserted AS (
            INSERT INTO provider_profiles
                (environment_id, name, kind, endpoint, model, secret_ref, labels)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
         RETURNING {PROFILE_COLUMNS}
         )
         SELECT {PROFILE_COLUMNS} FROM inserted"
    );
    let row = sqlx::query(&sql)
        .bind(scope.environment_id())
        .bind(new.name)
        .bind(new.kind.as_str())
        .bind(new.endpoint)
        .bind(new.model)
        .bind(new.secret_ref)
        .bind(new.labels)
        .fetch_one(scope.connection())
        .await?;
    row_to_profile(&row)
}

/// Delete a provider profile.
pub async fn delete(scope: &mut TenantScope<'_>, id: Uuid) -> Result<(), StoreError> {
    let affected = sqlx::query(
        "DELETE FROM provider_profiles WHERE environment_id = $1 AND id = $2",
    )
    .bind(scope.environment_id())
    .bind(id)
    .execute(scope.connection())
    .await?
    .rows_affected();
    if affected == 0 {
        Err(StoreError::NotFound)
    } else {
        Ok(())
    }
}

/// Record that a credential was rotated now.
pub async fn mark_rotated(
    scope: &mut TenantScope<'_>,
    name: &str,
    expires_at: Option<chrono::DateTime<chrono::Utc>>,
) -> Result<(), StoreError> {
    sqlx::query(
        "UPDATE provider_profiles
            SET credential_expiry = $3, version = version + 1, updated_at = now()
          WHERE environment_id = $1 AND name = $2",
    )
    .bind(scope.environment_id())
    .bind(name)
    .bind(expires_at)
    .execute(scope.connection())
    .await?;
    Ok(())
}

/// All secret references. Locators only; there is no value column to select.
pub async fn list_secrets(scope: &mut TenantScope<'_>) -> Result<Vec<SecretReference>, StoreError> {
    let rows = sqlx::query(
        "SELECT id, environment_id, name, locator, kind, rotated_at
           FROM secret_references WHERE environment_id = $1 ORDER BY name",
    )
    .bind(scope.environment_id())
    .fetch_all(scope.connection())
    .await?;

    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        let kind_raw: String = row.try_get("kind")?;
        out.push(SecretReference {
            id: row.try_get("id")?,
            environment_id: row.try_get("environment_id")?,
            name: row.try_get("name")?,
            locator: row.try_get("locator")?,
            kind: SecretKind::parse(&kind_raw).unwrap_or(SecretKind::ApiKey),
            rotated_at: row.try_get("rotated_at")?,
        });
    }
    Ok(out)
}

/// Register a secret locator.
pub async fn insert_secret(
    scope: &mut TenantScope<'_>,
    name: &str,
    locator: &str,
    kind: SecretKind,
) -> Result<Uuid, StoreError> {
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO secret_references (environment_id, name, locator, kind)
         VALUES ($1, $2, $3, $4) RETURNING id",
    )
    .bind(scope.environment_id())
    .bind(name)
    .bind(locator)
    .bind(kind.as_str())
    .fetch_one(scope.connection())
    .await?;
    Ok(id)
}
