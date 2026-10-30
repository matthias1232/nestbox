//! Samples of what a served model reports about itself.
//!
//! One row per scrape. The series is what makes a spike visible after the fact;
//! it holds numbers only (token counts, request counts, cache use), never a
//! prompt or a completion.

use chrono::{DateTime, Utc};
use serde_json::Value;
use uuid::Uuid;

use super::super::TenantScope;
use crate::error::StoreError;

/// Append one sample.
pub async fn insert(
    scope: &mut TenantScope<'_>,
    model_key: &str,
    node_id: Uuid,
    metrics: &Value,
) -> Result<(), StoreError> {
    sqlx::query(
        "INSERT INTO model_metric_samples (environment_id, model_key, node_id, metrics)
         VALUES ($1, $2, $3, $4)",
    )
    .bind(scope.environment_id())
    .bind(model_key)
    .bind(node_id)
    .bind(metrics)
    .execute(scope.connection())
    .await?;
    Ok(())
}

/// The newest `limit` samples of one model, oldest first so a chart can draw
/// left to right without reversing.
pub async fn list(
    scope: &mut TenantScope<'_>,
    model_key: &str,
    limit: i64,
) -> Result<Vec<(DateTime<Utc>, Value)>, StoreError> {
    let rows: Vec<(DateTime<Utc>, Value)> = sqlx::query_as(
        "SELECT captured_at, metrics FROM (
             SELECT captured_at, metrics
               FROM model_metric_samples
              WHERE environment_id = $1 AND model_key = $2
              ORDER BY captured_at DESC
              LIMIT $3
         ) recent
         ORDER BY captured_at",
    )
    .bind(scope.environment_id())
    .bind(model_key)
    .bind(limit)
    .fetch_all(scope.connection())
    .await?;
    Ok(rows)
}
