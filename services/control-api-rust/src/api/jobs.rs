//! Job and schedule endpoints.

use axum::Json;
use axum::extract::{Path, Query, State};
use serde::Deserialize;
use uuid::Uuid;

use crate::api::principal::Auth;
use crate::error::ApiError;
use crate::services::job_service;
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct ListQuery {
    #[serde(default = "default_limit")]
    pub limit: i64,
}

fn default_limit() -> i64 {
    50
}

/// List recent jobs.
pub async fn list(
    State(state): State<AppState>,
    auth: Auth,
    Query(query): Query<ListQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let jobs = job_service::list(&state.services, &auth.principal, query.limit).await?;
    Ok(Json(serde_json::json!({ "items": jobs })))
}

/// One job.
pub async fn get(
    State(state): State<AppState>,
    auth: Auth,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let job = job_service::get(&state.services, &auth.principal, id).await?;
    Ok(Json(serde_json::json!({
        "job": job,
        "percent": job.percent(),
        "retryable": job.is_retryable(),
    })))
}

/// A job's log.
pub async fn logs(
    State(state): State<AppState>,
    auth: Auth,
    Path(id): Path<Uuid>,
    Query(query): Query<ListQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let items = job_service::logs(&state.services, &auth.principal, id, query.limit).await?;
    Ok(Json(serde_json::json!({ "items": items })))
}

/// All schedules.
pub async fn schedules(
    State(state): State<AppState>,
    auth: Auth,
) -> Result<Json<serde_json::Value>, ApiError> {
    let schedules = job_service::schedules(&state.services, &auth.principal).await?;
    Ok(Json(serde_json::json!({ "items": schedules })))
}

/// Run a schedule now.
pub async fn run_schedule(
    State(state): State<AppState>,
    auth: Auth,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
    Ok(Json(
        job_service::run_schedule(&state.services, &auth.principal, id).await?,
    ))
}
