//! Sandbox endpoints.

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use serde::Deserialize;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::api::principal::Auth;
use crate::domain::pagination::ListParams;
use crate::error::ApiError;
use crate::services::sandbox_service::{self, CreateSandbox, LifecycleAction};
use crate::state::AppState;

/// Query parameters for the sandbox list.
///
/// Filters use the `filter[field]=value` shape so a new filter does not need a
/// new query parameter.
#[derive(Debug, Deserialize)]
pub struct ListQuery {
    pub cursor: Option<String>,
    pub limit: Option<i64>,
    pub sort: Option<String>,
    pub direction: Option<String>,
    pub search: Option<String>,
    /// `filter[status]=running` and friends.
    #[serde(flatten)]
    pub filters: std::collections::BTreeMap<String, String>,
}

/// List sandboxes with cursor paging and facets.
pub async fn list(
    State(state): State<AppState>,
    auth: Auth,
    Query(query): Query<ListQuery>,
) -> Result<Json<crate::domain::pagination::Page<crate::domain::sandbox::SandboxSummary>>, ApiError> {
    // Only `filter[...]` keys become filters; anything else is a typo and is
    // ignored rather than silently broadening the query.
    let filters: std::collections::BTreeMap<String, String> = query
        .filters
        .iter()
        .filter_map(|(key, value)| {
            key.strip_prefix("filter[")
                .and_then(|rest| rest.strip_suffix(']'))
                .map(|field| (field.to_string(), value.clone()))
        })
        .collect();

    let params = ListParams::from_query(
        query.cursor,
        query.limit,
        query.sort,
        query.direction,
        query.search,
        filters,
    );

    let page = sandbox_service::list(&state.services, &auth.principal, &params).await?;
    Ok(Json(page))
}

/// Read one sandbox, merging what the runtime reports.
pub async fn get(
    State(state): State<AppState>,
    auth: Auth,
    Path(id): Path<Uuid>,
) -> Result<Json<crate::domain::sandbox::SandboxDetail>, ApiError> {
    let detail = sandbox_service::get(&state.services, &auth.principal, id).await?;
    Ok(Json(detail))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateRequest {
    pub name: String,
    pub pack_key: String,
    /// Optional: empty means the pack's default image.
    #[serde(default)]
    pub image: String,
    pub image_digest: Option<String>,
    /// Optional: empty means the pack's default command.
    #[serde(default)]
    pub command: Vec<String>,
    #[serde(default = "default_memory")]
    pub memory_limit: String,
    #[serde(default = "default_cpu")]
    pub cpu_limit: f64,
    #[serde(default = "default_pids")]
    pub pids_limit: i64,
    /// The node to place the sandbox on, by name. Omitted means the local node.
    #[serde(default)]
    pub node: Option<String>,
}

fn default_memory() -> String {
    "2Gi".to_string()
}
fn default_cpu() -> f64 {
    1.0
}
fn default_pids() -> i64 {
    512
}

/// Create a sandbox.
pub async fn create(
    State(state): State<AppState>,
    auth: Auth,
    Json(body): Json<CreateRequest>,
) -> Result<(StatusCode, Json<crate::domain::sandbox::Sandbox>), ApiError> {
    let sandbox = sandbox_service::create(
        &state.services,
        &auth.principal,
        CreateSandbox {
            name: body.name,
            pack_key: body.pack_key,
            image: body.image,
            image_digest: body.image_digest,
            command: body.command,
            memory_limit: body.memory_limit,
            cpu_limit: body.cpu_limit,
            pids_limit: body.pids_limit,
            node: body.node,
        },
    )
    .await?;
    Ok((StatusCode::CREATED, Json(sandbox)))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct ActionRequest {
    pub action: String,
}

/// Run a lifecycle action.
pub async fn action(
    State(state): State<AppState>,
    auth: Auth,
    Path(id): Path<Uuid>,
    Json(body): Json<ActionRequest>,
) -> Result<Json<crate::domain::sandbox::Sandbox>, ApiError> {
    let action = LifecycleAction::parse(&body.action).ok_or_else(|| {
        ApiError::validation("Unknown action; expected start, stop or restart.")
    })?;
    let sandbox = sandbox_service::act(&state.services, &auth.principal, id, action).await?;
    Ok(Json(sandbox))
}

/// Delete a sandbox.
pub async fn delete(
    State(state): State<AppState>,
    auth: Auth,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    sandbox_service::delete(&state.services, &auth.principal, id).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Deserialize)]
pub struct LogQuery {
    #[serde(default = "default_tail")]
    pub tail: u32,
}

fn default_tail() -> u32 {
    200
}

#[derive(Debug, serde::Serialize, ToSchema)]
pub struct LogResponse {
    pub sandbox_id: Uuid,
    pub content: String,
}

/// Read a sandbox's logs.
pub async fn logs(
    State(state): State<AppState>,
    auth: Auth,
    Path(id): Path<Uuid>,
    Query(query): Query<LogQuery>,
) -> Result<Json<LogResponse>, ApiError> {
    let content = sandbox_service::logs(
        &state.services,
        &auth.principal,
        id,
        query.tail.clamp(1, 5000),
    )
    .await?;
    Ok(Json(LogResponse {
        sandbox_id: id,
        content,
    }))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct ExecRequest {
    /// An argument vector, e.g. `["bash", "-lc", "hermes --version"]`.
    pub command: Vec<String>,
    #[serde(default = "default_exec_timeout")]
    pub timeout_seconds: u32,
}

fn default_exec_timeout() -> u32 {
    60
}

/// Run one command inside a running sandbox and return its output.
pub async fn exec(
    State(state): State<AppState>,
    auth: Auth,
    Path(id): Path<Uuid>,
    Json(body): Json<ExecRequest>,
) -> Result<Json<crate::domain::runtime::ExecResult>, ApiError> {
    let result = sandbox_service::exec(
        &state.services,
        &auth.principal,
        id,
        body.command,
        body.timeout_seconds,
    )
    .await?;
    Ok(Json(result))
}
