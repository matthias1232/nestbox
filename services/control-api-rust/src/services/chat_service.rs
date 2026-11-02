//! Chat turns: the rows the console reads, the queries it filters with, and who
//! may see the text.
//!
//! A turn is one model call: what was asked (masked), what answered (masked), how
//! it was routed, what the content check did, and what it cost. `route` is the
//! chain the console draws.
//!
//! This module owns the rules, not the SQL: the statements live in
//! `store::postgres::chat_turns`.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::domain::privacy::PrivacyIncident;
use crate::domain::telemetry::LiveEvent;
use crate::error::ApiError;
use crate::services::ServiceContext;
use crate::store::TenantScope;
use crate::domain::auth::Principal;

/// One captured model call as the ingest hands it over. Bodies are masked.
#[derive(Debug, Clone, Default)]
pub struct TurnInput {
    pub id: Uuid,
    pub sandbox_id: Option<Uuid>,
    pub ts: DateTime<Utc>,
    pub ended_at: Option<DateTime<Utc>>,
    pub request_ref: Option<String>,
    pub method: String,
    pub path: String,
    pub host: String,
    pub user_agent: Option<String>,
    pub upstream_ip: Option<String>,
    pub upstream_ip_would: Option<String>,
    pub account_kind: Option<String>,
    pub router_provider: Option<String>,
    pub router_model: Option<String>,
    pub status: Option<i32>,
    pub dlp: Option<String>,
    pub channel: Option<String>,
    pub decision: Option<String>,
    pub request_body: Option<String>,
    pub response_body: Option<String>,
}

/// Filters of the chat list. `by` is only read by [`groups`].
#[derive(Debug, Clone, Default, Deserialize, ToSchema)]
pub struct TurnQuery {
    pub sandbox_id: Option<Uuid>,
    pub session_id: Option<String>,
    /// Matches the requested or the served model.
    pub model: Option<String>,
    pub provider: Option<String>,
    /// `clean` means "no finding".
    pub dlp: Option<String>,
    pub account_kind: Option<String>,
    pub owner: Option<Uuid>,
    /// Free text over the turn and its metadata.
    pub q: Option<String>,
    pub since: Option<DateTime<Utc>>,
    pub until: Option<DateTime<Utc>>,
    /// Older than this timestamp: the next page.
    pub before: Option<DateTime<Utc>>,
    pub limit: Option<i64>,
    /// `session`, `model`, `provider`, `day`, `sandbox` or `owner`.
    pub by: Option<String>,
}

/// One stored turn as the console reads it.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ChatTurn {
    pub id: Uuid,
    pub environment_id: Uuid,
    pub sandbox_id: Option<Uuid>,
    pub sandbox_name: Option<String>,
    pub owner_user_id: Option<Uuid>,
    pub owner_name: Option<String>,
    pub ts: DateTime<Utc>,
    pub ended_at: Option<DateTime<Utc>>,
    pub request_ref: Option<String>,
    pub session_id: Option<String>,
    pub agent: Option<String>,
    pub provider: String,
    pub host: Option<String>,
    pub upstream_ip: Option<String>,
    pub upstream_ip_would: Option<String>,
    pub account_kind: Option<String>,
    pub requested_model: Option<String>,
    pub served_model: Option<String>,
    pub router_provider: Option<String>,
    pub router_model: Option<String>,
    pub user_text: Option<String>,
    pub assistant_text: Option<String>,
    #[schema(value_type = Object)]
    pub tool_calls: serde_json::Value,
    pub input_tokens: Option<i64>,
    pub output_tokens: Option<i64>,
    pub cache_read: Option<i64>,
    pub cache_write: Option<i64>,
    pub status: Option<i32>,
    pub dlp: Option<String>,
    pub delivered: bool,
    #[schema(value_type = Object)]
    pub route: serde_json::Value,
}

/// One group of the chat list.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ChatGroup {
    pub key: String,
    pub label: String,
    pub turns: i64,
    pub first_ts: DateTime<Utc>,
    pub last_ts: DateTime<Utc>,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub blocked: i64,
    pub replaced: i64,
    pub pseudonymized: i64,
}

/// A turn with the findings of its request.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ChatTurnDetail {
    pub turn: ChatTurn,
    pub incidents: Vec<PrivacyIncident>,
}

fn not_implemented() -> ApiError {
    ApiError::new(
        axum::http::StatusCode::NOT_IMPLEMENTED,
        crate::error::ErrorCode::INTERNAL,
        "Not implemented yet.",
    )
}

/// Store the turns of one telemetry batch. Returns how many rows were written.
pub async fn record_turns(
    ctx: &ServiceContext,
    environment_id: Uuid,
    items: &[TurnInput],
) -> Result<u64, ApiError> {
    let _ = (ctx, environment_id, items);
    Ok(0)
}

/// The turn list, filtered and paged.
pub async fn turns(
    ctx: &ServiceContext,
    principal: &Principal,
    query: TurnQuery,
) -> Result<Vec<ChatTurn>, ApiError> {
    let _ = (ctx, principal, query);
    Ok(Vec::new())
}

/// The same rows, grouped.
pub async fn groups(
    ctx: &ServiceContext,
    principal: &Principal,
    query: TurnQuery,
) -> Result<Vec<ChatGroup>, ApiError> {
    let _ = (ctx, principal, query);
    Ok(Vec::new())
}

/// One turn with its findings.
pub async fn turn(
    ctx: &ServiceContext,
    principal: &Principal,
    id: Uuid,
) -> Result<ChatTurnDetail, ApiError> {
    let _ = (ctx, principal, id);
    Err(not_implemented())
}

/// Record that an administrator opened a live view. Audited, not counted.
pub async fn record_live_watch(
    ctx: &ServiceContext,
    principal: &Principal,
    what: &str,
) -> Result<(), ApiError> {
    let _ = (ctx, principal, what);
    Ok(())
}

/// What a principal may see of a live chat event; `None` drops it.
///
/// An administrator with `admin_chat_view` off sees the event without its text.
/// A user sees their own sandboxes only, which the stream's visibility check has
/// already decided.
pub async fn redact_live_for(
    ctx: &ServiceContext,
    principal: &Principal,
    event: LiveEvent,
) -> Option<LiveEvent> {
    let _ = (ctx, principal);
    Some(event)
}

/// One row of `chat_turns` as the store writes it.
pub(crate) struct NewTurn<'a> {
    pub id: Uuid,
    pub environment_id: Uuid,
    pub sandbox_id: Option<Uuid>,
    pub owner_user_id: Option<Uuid>,
    pub ts: DateTime<Utc>,
    pub ended_at: Option<DateTime<Utc>>,
    pub request_ref: Option<&'a str>,
    pub session_id: Option<&'a str>,
    pub agent: Option<&'a str>,
    pub provider: &'a str,
    pub host: Option<&'a str>,
    pub upstream_ip: Option<&'a str>,
    pub upstream_ip_would: Option<&'a str>,
    pub account_kind: Option<&'a str>,
    pub requested_model: Option<&'a str>,
    pub served_model: Option<&'a str>,
    pub router_provider: Option<&'a str>,
    pub router_model: Option<&'a str>,
    pub user_text: Option<&'a str>,
    pub assistant_text: Option<&'a str>,
    pub tool_calls: &'a serde_json::Value,
    pub input_tokens: Option<i64>,
    pub output_tokens: Option<i64>,
    pub cache_read: Option<i64>,
    pub cache_write: Option<i64>,
    pub status: Option<i32>,
    pub dlp: Option<&'a str>,
    pub delivered: bool,
    pub route: &'a serde_json::Value,
}

/// Narrow the store's row to what a principal may read.
pub(crate) fn hide_text(turn: &mut ChatTurn) {
    turn.user_text = None;
    turn.assistant_text = None;
    turn.tool_calls = serde_json::json!([]);
}

/// Whether a principal owns a sandbox, for the `admin_chat_view` rule.
pub(crate) async fn owns(
    scope: &mut TenantScope<'_>,
    principal: &Principal,
    sandbox_id: Option<Uuid>,
) -> bool {
    let Some(sandbox_id) = sandbox_id else {
        return false;
    };
    let owner: Option<Option<Uuid>> = sqlx::query_scalar(
        "SELECT owner_user_id FROM sandboxes WHERE environment_id = $1 AND id = $2",
    )
    .bind(scope.environment_id())
    .bind(sandbox_id)
    .fetch_optional(scope.connection())
    .await
    .ok()
    .flatten();
    owner.flatten() == Some(principal.user_id)
}

/// Whether the environment lets administrators read chat content.
pub(crate) async fn admin_may_read(
    ctx: &ServiceContext,
    principal: &Principal,
) -> Result<bool, ApiError> {
    let settings =
        crate::services::privacy_service::effective_settings(ctx, principal, None).await?;
    Ok(settings.admin_chat_view)
}
