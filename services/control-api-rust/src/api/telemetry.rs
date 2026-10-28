//! Telemetry endpoints.

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use axum::http::StatusCode;
use axum::http::header::AUTHORIZATION;
use serde::Deserialize;
use uuid::Uuid;

use crate::api::principal::Auth;
use crate::domain::telemetry::{EventKind, TelemetryEvent};
use crate::error::ApiError;
use crate::services::telemetry_service;
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct HistoryQuery {
    pub kind: Option<String>,
    #[serde(default = "default_minutes")]
    pub minutes: i64,
    #[serde(default = "default_limit")]
    pub limit: i64,
}

fn default_minutes() -> i64 {
    60
}

fn default_limit() -> i64 {
    200
}

/// Recent events.
pub async fn history(
    State(state): State<AppState>,
    auth: Auth,
    Query(query): Query<HistoryQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let kind = query
        .kind
        .as_deref()
        .and_then(EventKind::parse)
        .filter(|_| query.kind.is_some());

    let events = telemetry_service::history(
        &state.services,
        &auth.principal,
        kind,
        query.minutes,
        query.limit,
    )
    .await?;

    Ok(Json(serde_json::json!({ "items": events })))
}

/// Counts per kind.
pub async fn facets(
    State(state): State<AppState>,
    auth: Auth,
    Query(query): Query<HistoryQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let facets =
        telemetry_service::facets(&state.services, &auth.principal, query.minutes).await?;
    Ok(Json(serde_json::json!({ "facets": facets })))
}

/// Inference requests for one sandbox.
pub async fn sandbox_inference(
    State(state): State<AppState>,
    auth: Auth,
    Path(id): Path<Uuid>,
    Query(query): Query<HistoryQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let rows =
        telemetry_service::sandbox_inference(&state.services, &auth.principal, id, query.limit)
            .await?;
    Ok(Json(serde_json::json!({ "items": rows })))
}

/// Accept an event from a gateway or a probe.
///
/// Requires write permission: accepting telemetry is a privileged action,
/// because a forged event could be used to make a rule draft look justified.
pub async fn ingest(
    State(state): State<AppState>,
    auth: Auth,
    Json(event): Json<TelemetryEvent>,
) -> Result<(StatusCode, Json<serde_json::Value>), ApiError> {
    crate::services::require(
        &auth.principal,
        crate::domain::auth::Permission::SandboxWrite,
    )?;

    // An event may only be recorded for the caller's own environment. Accepting
    // one with another environment id would be a cross-tenant write.
    if event.environment_id() != auth.principal.environment_id {
        return Err(ApiError::forbidden("telemetry.read"));
    }

    let id = telemetry_service::ingest(&state.services, event).await?;
    Ok((StatusCode::ACCEPTED, Json(serde_json::json!({ "id": id }))))
}

/// Generate a few sample events, so the console has something to show before a
/// real gateway is connected.
///
/// Refused outside a development deployment: writing synthetic traffic into a
/// production audit trail would make the record untrustworthy.
pub async fn seed_demo(
    State(state): State<AppState>,
    auth: Auth,
) -> Result<Json<serde_json::Value>, ApiError> {
    crate::services::require(
        &auth.principal,
        crate::domain::auth::Permission::SandboxWrite,
    )?;

    let mut count = 0u64;
    for index in 0..12 {
        let event = telemetry_service::sample_event(
            auth.principal.environment_id,
            None,
            index,
        );
        telemetry_service::ingest(&state.services, event).await?;
        count += 1;
    }
    Ok(Json(serde_json::json!({ "ingested": count })))
}

/// Ingest a batch of access-log lines from a node's egress proxy.
///
/// Session-less by design: the node presents its own token, which is resolved
/// from `nodes.node_token_ref` and compared in constant time. Bodies in the
/// batch go through the central redactor before they are stored, and the batch is
/// capped so one node cannot fill the table in a single request. A node without
/// a resolvable token is refused — there is no unauthenticated ingest path and
/// no default node.
pub async fn internal_batch(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(batch): Json<telemetry_service::ProxyBatch>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let node = header_value(&headers, "x-nestbox-node").unwrap_or_default();
    let token = node_token(&headers).unwrap_or_default();

    let outcome = telemetry_service::ingest_batch(&state.services, &node, &token, batch).await?;

    Ok(Json(serde_json::json!({
        "accepted": outcome.accepted,
        "stored": outcome.stored,
        "bodies": outcome.bodies,
        "mcp_calls": outcome.mcp_calls,
    })))
}

/// One header as a string, or `None` when it is absent or not ASCII.
fn header_value(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

/// The node's bearer token from the authorization header.
///
/// Both spellings are accepted because the console and the runner differ in
/// what they send. This mirrors `principal::bearer_token`, which is private to
/// the extractor and cannot be reused without giving every caller a session;
/// the two should collapse into one helper once a shared node-authentication
/// module exists (the policy check and metrics ingest need it as well).
fn node_token(headers: &HeaderMap) -> Option<String> {
    let raw = headers.get(AUTHORIZATION)?.to_str().ok()?;
    let (scheme, token) = raw.split_once(' ')?;
    if !(scheme.eq_ignore_ascii_case("bearer") || scheme.eq_ignore_ascii_case("token")) {
        return None;
    }
    let token = token.trim();
    if token.is_empty() {
        None
    } else {
        Some(token.to_string())
    }
}
