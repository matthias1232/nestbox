//! Policy, rule, draft, and traffic-window endpoints.

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use serde::Deserialize;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::api::principal::Auth;
use crate::domain::policy::Policy;
use crate::error::ApiError;
use crate::services::policy_service::{self, PolicyDiff, PolicySaveResult};
use crate::state::AppState;

/// List policies.
pub async fn list(
    State(state): State<AppState>,
    auth: Auth,
) -> Result<Json<serde_json::Value>, ApiError> {
    let policies = policy_service::list(&state.services, &auth.principal).await?;
    let global = policy_service::global(&state.services, &auth.principal).await?;
    Ok(Json(serde_json::json!({
        "items": policies,
        "global": global,
    })))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreatePolicyRequest {
    pub name: String,
    #[serde(default = "default_scope")]
    pub scope: String,
    /// The complete four-section document.
    pub spec: serde_json::Value,
    #[serde(default)]
    pub version: Option<i64>,
}

fn default_scope() -> String {
    "sandbox".to_string()
}

/// Create a policy.
pub async fn create(
    State(state): State<AppState>,
    auth: Auth,
    Json(body): Json<CreatePolicyRequest>,
) -> Result<(StatusCode, Json<PolicySaveResult>), ApiError> {
    let scope = crate::domain::policy::PolicyScope::parse(&body.scope)
        .ok_or_else(|| ApiError::validation("Scope must be 'sandbox' or 'global'."))?;
    let result = policy_service::create(
        &state.services,
        &auth.principal,
        body.name.trim(),
        scope,
        &body.spec,
    )
    .await?;
    Ok((StatusCode::CREATED, Json(result)))
}

/// Read one policy.
pub async fn get(
    State(state): State<AppState>,
    auth: Auth,
    Path(id): Path<Uuid>,
) -> Result<Json<Policy>, ApiError> {
    Ok(Json(
        policy_service::get(&state.services, &auth.principal, id).await?,
    ))
}

/// The active global override.
pub async fn global(
    State(state): State<AppState>,
    auth: Auth,
) -> Result<Json<serde_json::Value>, ApiError> {
    let policy = policy_service::global(&state.services, &auth.principal).await?;
    Ok(Json(serde_json::json!({ "policy": policy })))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdatePolicyRequest {
    pub spec: serde_json::Value,
    pub version: i64,
}

/// Replace a policy document.
pub async fn update(
    State(state): State<AppState>,
    auth: Auth,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdatePolicyRequest>,
) -> Result<Json<PolicySaveResult>, ApiError> {
    let result = policy_service::update(
        &state.services,
        &auth.principal,
        id,
        body.version,
        &body.spec,
    )
    .await?;
    Ok(Json(result))
}

/// Delete a policy.
pub async fn delete(
    State(state): State<AppState>,
    auth: Auth,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    policy_service::delete(&state.services, &auth.principal, id).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Deserialize)]
pub struct DiffQuery {
    pub against: Option<i64>,
}

/// Diff a policy against an earlier revision.
pub async fn diff(
    State(state): State<AppState>,
    auth: Auth,
    Path(id): Path<Uuid>,
    Query(query): Query<DiffQuery>,
) -> Result<Json<PolicyDiff>, ApiError> {
    Ok(Json(
        policy_service::diff(&state.services, &auth.principal, id, query.against).await?,
    ))
}

/// All network rules.
pub async fn list_rules(
    State(state): State<AppState>,
    auth: Auth,
) -> Result<Json<serde_json::Value>, ApiError> {
    let rules = policy_service::list_rules(&state.services, &auth.principal).await?;
    Ok(Json(serde_json::json!({ "items": rules })))
}

/// Rules attached to one sandbox.
pub async fn sandbox_rules(
    State(state): State<AppState>,
    auth: Auth,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let rules =
        policy_service::rules_for_sandbox(&state.services, &auth.principal, id).await?;
    Ok(Json(serde_json::json!({ "items": rules })))
}

/// Create a network rule.
pub async fn create_rule(
    State(state): State<AppState>,
    auth: Auth,
    Json(mut rule): Json<crate::domain::policy::NetworkRule>,
) -> Result<(StatusCode, Json<crate::domain::policy::NetworkRule>), ApiError> {
    // The id is assigned by the database; a caller-supplied one is ignored so a
    // request cannot overwrite an existing row by guessing its id.
    rule.id = Uuid::nil();
    let created = policy_service::create_rule(&state.services, &auth.principal, rule).await?;
    Ok((StatusCode::CREATED, Json(created)))
}

/// Delete a network rule.
pub async fn delete_rule(
    State(state): State<AppState>,
    auth: Auth,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    policy_service::delete_rule(&state.services, &auth.principal, id).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Pending rule drafts.
pub async fn list_drafts(
    State(state): State<AppState>,
    auth: Auth,
) -> Result<Json<serde_json::Value>, ApiError> {
    let drafts = policy_service::pending_drafts(&state.services, &auth.principal).await?;
    Ok(Json(serde_json::json!({ "items": drafts })))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct DraftRequest {
    #[serde(default = "default_source")]
    pub source_kind: String,
    pub event: serde_json::Value,
    pub sandbox_id: Option<Uuid>,
}

fn default_source() -> String {
    "network".to_string()
}

/// Draft a rule from an event.
pub async fn draft_from_event(
    State(state): State<AppState>,
    auth: Auth,
    Json(body): Json<DraftRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), ApiError> {
    let draft = policy_service::draft_from_event(
        &state.services,
        &auth.principal,
        &body.source_kind,
        &body.event,
        body.sandbox_id,
    )
    .await?;
    Ok((StatusCode::CREATED, Json(draft)))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct DecisionRequest {
    #[serde(default)]
    pub reason: Option<String>,
}

/// Approve a draft.
pub async fn approve_draft(
    State(state): State<AppState>,
    auth: Auth,
    Path(id): Path<Uuid>,
    Json(body): Json<DecisionRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    Ok(Json(
        policy_service::decide_draft(
            &state.services,
            &auth.principal,
            id,
            true,
            body.reason.as_deref(),
        )
        .await?,
    ))
}

/// Reject a draft.
pub async fn reject_draft(
    State(state): State<AppState>,
    auth: Auth,
    Path(id): Path<Uuid>,
    Json(body): Json<DecisionRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    Ok(Json(
        policy_service::decide_draft(
            &state.services,
            &auth.principal,
            id,
            false,
            body.reason.as_deref(),
        )
        .await?,
    ))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct WindowRequest {
    pub sandbox_id: Option<Uuid>,
    #[serde(default = "default_minutes")]
    pub minutes: i32,
    pub reason: String,
}

fn default_minutes() -> i32 {
    15
}

/// Open a temporary traffic window.
pub async fn open_window(
    State(state): State<AppState>,
    auth: Auth,
    Json(body): Json<WindowRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), ApiError> {
    let window = policy_service::open_window(
        &state.services,
        &auth.principal,
        body.sandbox_id,
        body.minutes,
        body.reason.trim(),
    )
    .await?;
    Ok((StatusCode::CREATED, Json(window)))
}

/// Close a window early.
pub async fn close_window(
    State(state): State<AppState>,
    auth: Auth,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    policy_service::close_window(&state.services, &auth.principal, id).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Decide one egress request on behalf of a node's proxy.
///
/// The caller is a node, not a browser: it presents the node token, so there is
/// no session and no CSRF token. Default deny applies here — a request that
/// cannot be read is refused, never allowed.
///
/// The facts arrive as **headers**, not as a JSON body. Envoy's `ext_authz`
/// HTTP filter forwards a request that keeps the original method and appends the
/// original path to `path_prefix`; it copies only the headers it is configured
/// to copy. So the route accepts any method, and this handler reads
/// `x-nestbox-sandbox`, the authority, and the original path. A decider that
/// required a POST and a body would refuse every real request.
///
/// The `feat/policy-decider` package fills the body; until then the proxy has
/// nothing to ask, and a stub answering 501 is a deny because Envoy treats a
/// non-200 authorization response as a refusal.
pub async fn internal_check(
    State(_state): State<AppState>,
    _headers: axum::http::HeaderMap,
) -> Result<Json<serde_json::Value>, ApiError> {
    Err(ApiError::not_implemented("POST /internal/policy/check"))
}
