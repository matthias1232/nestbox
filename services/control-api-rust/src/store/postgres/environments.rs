//! Environments, clusters, nodes, and the overview counts.

use sqlx::Row;
use uuid::Uuid;

use super::super::TenantScope;
use crate::domain::environment::{Cluster, Environment, EnvironmentSummary, PlacementPolicy};
use crate::error::StoreError;

/// The environment a request is scoped to, by id.
pub async fn get_environment(
    pool: &sqlx::PgPool,
    environment_id: Uuid,
) -> Result<Environment, StoreError> {
    let row = sqlx::query(
        "SELECT id, organization_id, name, slug, timezone, retention_days, created_at
           FROM environments WHERE id = $1",
    )
    .bind(environment_id)
    .fetch_optional(pool)
    .await?
    .ok_or(StoreError::NotFound)?;

    Ok(Environment {
        id: row.try_get("id")?,
        organization_id: row.try_get("organization_id")?,
        name: row.try_get("name")?,
        slug: row.try_get("slug")?,
        timezone: row.try_get("timezone")?,
        retention_days: row.try_get("retention_days")?,
        created_at: row.try_get("created_at")?,
    })
}

/// Every environment, for the admin view. Deliberately unscoped: only an admin
/// path calls it, and it returns environments rather than their contents.
pub async fn list_environments(
    pool: &sqlx::PgPool,
    organization_id: Uuid,
) -> Result<Vec<Environment>, StoreError> {
    let rows = sqlx::query(
        "SELECT id, organization_id, name, slug, timezone, retention_days, created_at
           FROM environments WHERE organization_id = $1 ORDER BY name",
    )
    .bind(organization_id)
    .fetch_all(pool)
    .await?;

    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        out.push(Environment {
            id: row.try_get("id")?,
            organization_id: row.try_get("organization_id")?,
            name: row.try_get("name")?,
            slug: row.try_get("slug")?,
            timezone: row.try_get("timezone")?,
            retention_days: row.try_get("retention_days")?,
            created_at: row.try_get("created_at")?,
        });
    }
    Ok(out)
}

/// The clusters in this environment.
pub async fn list_clusters(scope: &mut TenantScope<'_>) -> Result<Vec<Cluster>, StoreError> {
    let rows = sqlx::query(
        "SELECT id, environment_id, name, slug, placement_policy, created_at
           FROM clusters WHERE environment_id = $1 ORDER BY name",
    )
    .bind(scope.environment_id())
    .fetch_all(scope.connection())
    .await?;

    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        let policy_raw: String = row.try_get("placement_policy")?;
        out.push(Cluster {
            id: row.try_get("id")?,
            environment_id: row.try_get("environment_id")?,
            name: row.try_get("name")?,
            slug: row.try_get("slug")?,
            placement_policy: PlacementPolicy::parse(&policy_raw)
                .unwrap_or(PlacementPolicy::LeastLoaded),
            created_at: row.try_get("created_at")?,
        });
    }
    Ok(out)
}

/// Counts for the environment switcher and the overview page.
pub async fn summary(scope: &mut TenantScope<'_>) -> Result<EnvironmentSummary, StoreError> {
    let environment_id = scope.environment_id();
    let env = sqlx::query("SELECT id, name, slug FROM environments WHERE id = $1")
        .bind(environment_id)
        .fetch_one(scope.connection())
        .await?;

    let sandbox_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM sandboxes WHERE environment_id = $1")
            .bind(environment_id)
            .fetch_one(scope.connection())
            .await?;
    let gateway_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM gateways g JOIN clusters c ON c.id = g.cluster_id
          WHERE c.environment_id = $1",
    )
    .bind(environment_id)
    .fetch_one(scope.connection())
    .await?;
    let running_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sandboxes WHERE environment_id = $1 AND status = 'running'",
    )
    .bind(environment_id)
    .fetch_one(scope.connection())
    .await?;

    Ok(EnvironmentSummary {
        id: env.try_get("id")?,
        name: env.try_get("name")?,
        slug: env.try_get("slug")?,
        sandbox_count,
        gateway_count,
        running_count,
    })
}

/// The default cluster, used when a create request names none.
pub async fn default_cluster(scope: &mut TenantScope<'_>) -> Result<Uuid, StoreError> {
    sqlx::query_scalar(
        "SELECT id FROM clusters WHERE environment_id = $1 ORDER BY created_at LIMIT 1",
    )
    .bind(scope.environment_id())
    .fetch_optional(scope.connection())
    .await?
    .ok_or(StoreError::NotFound)
}
