//! Accounting and the internal ingest the proxy calls.
//!
//! The browser API reads aggregates and rows; the internal routes are called by
//! the accounting proxy with a bearer token, and they are the only writer of
//! `accounting_requests`. Neither returns content: a row is a model, a token
//! count, a cost and a decision.

use axum::Json;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode, header};
use serde::Deserialize;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::api::principal::Auth;
use crate::domain::accounting::{
    AccountingSettings, Direction, NewAccountingRow, Overview, Timeframe, Usage,
};

use crate::error::{ApiError, ErrorCode};
use crate::services::accounting_service::{self, RecordedRequest};
use crate::services::privacy_service::{self, InspectResult};
use crate::state::AppState;

/// The window and the optional sandbox on a read.
#[derive(Debug, Deserialize)]
pub struct ViewQuery {
    #[serde(default)]
    pub timeframe: Option<String>,
    #[serde(default)]
    pub sandbox_id: Option<Uuid>,
    #[serde(default)]
    pub limit: Option<i64>,
}

/// The organisation (or one sandbox) at a glance.
pub async fn overview(
    State(state): State<AppState>,
    auth: Auth,
    Query(query): Query<ViewQuery>,
) -> Result<Json<Overview>, ApiError> {
    let timeframe = parse_timeframe(query.timeframe.as_deref())?;
    Ok(Json(
        accounting_service::overview(&state.services, &auth.principal, timeframe, query.sandbox_id)
            .await?,
    ))
}

/// Recent requests, newest first.
pub async fn requests(
    State(state): State<AppState>,
    auth: Auth,
    Query(query): Query<ViewQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let timeframe = parse_timeframe(query.timeframe.as_deref())?;
    let items = accounting_service::requests(
        &state.services,
        &auth.principal,
        timeframe,
        query.sandbox_id,
        query.limit.unwrap_or(100),
    )
    .await?;
    Ok(Json(serde_json::json!({ "items": items })))
}

fn parse_timeframe(raw: Option<&str>) -> Result<Timeframe, ApiError> {
    match raw {
        None => Ok(Timeframe::H24),
        Some(value) => Timeframe::parse(value)
            .ok_or_else(|| ApiError::validation("timeframe must be 5m, 1h, 24h, 7d or 30d.")),
    }
}

/// The retention settings of this environment.
pub async fn get_settings(
    State(state): State<AppState>,
    auth: Auth,
) -> Result<Json<AccountingSettings>, ApiError> {
    Ok(Json(accounting_service::settings(&state.services, &auth.principal).await?))
}

/// A settings save: the whole row, so a partial write cannot clear a field.
#[derive(Debug, Deserialize, ToSchema)]
pub struct SettingsBody {
    pub retention_days: i32,
    #[serde(default = "default_keep_daily")]
    pub keep_daily: bool,
}

fn default_keep_daily() -> bool {
    true
}

pub async fn put_settings(
    State(state): State<AppState>,
    auth: Auth,
    Json(body): Json<SettingsBody>,
) -> Result<Json<AccountingSettings>, ApiError> {
    let update = AccountingSettings {
        retention_days: body.retention_days,
        keep_daily: body.keep_daily,
        updated_by: None,
        updated_at: None,
    };
    Ok(Json(accounting_service::set_settings(&state.services, &auth.principal, update).await?))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct PurgeBody {
    pub days: i32,
}

/// Delete raw rows older than `days`, keeping the daily rollups.
pub async fn purge(
    State(state): State<AppState>,
    auth: Auth,
    Json(body): Json<PurgeBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let outcome =
        accounting_service::purge_older_than(&state.services, &auth.principal, body.days).await?;
    Ok(Json(serde_json::json!(outcome)))
}

// --- internal: the proxy ---------------------------------------------------

/// The token the accounting proxy presents.
fn ingest_token() -> Option<String> {
    if let Ok(path) = std::env::var("NESTBOX_ACCOUNTING_TOKEN_FILE") {
        if let Ok(value) = std::fs::read_to_string(&path) {
            let value = value.trim().to_string();
            if !value.is_empty() {
                return Some(value);
            }
        }
    }
    std::env::var("NESTBOX_ACCOUNTING_TOKEN")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn authorised(headers: &HeaderMap) -> Result<(), ApiError> {
    use sha2::{Digest, Sha256};
    use subtle::ConstantTimeEq;

    let Some(expected) = ingest_token() else {
        return Err(ApiError::new(
            StatusCode::SERVICE_UNAVAILABLE,
            ErrorCode::INTERNAL,
            "The accounting ingest has no token configured.",
        ));
    };
    let presented = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .unwrap_or("")
        .trim();
    let expected_hash = Sha256::digest(expected.as_bytes());
    let presented_hash = Sha256::digest(presented.as_bytes());
    if presented.is_empty() || expected_hash.ct_eq(&presented_hash).unwrap_u8() != 1 {
        return Err(ApiError::unauthenticated("The accounting token is not valid."));
    }
    Ok(())
}

/// One request as the proxy reports it: metadata and counters only.
#[derive(Debug, Deserialize, ToSchema)]
pub struct RecordBody {
    pub sandbox: String,
    #[serde(default)]
    pub agent_instance: Option<String>,
    #[serde(default)]
    pub provider: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub direction: Option<String>,
    #[serde(default)]
    pub endpoint: Option<String>,
    #[serde(default)]
    pub status: Option<i32>,
    #[serde(default)]
    pub latency_ms: Option<i32>,
    #[serde(default)]
    pub usage: Option<UsageBody>,
    #[serde(default)]
    pub content_digest: Option<String>,
    #[serde(default)]
    pub privacy_action: Option<String>,
    #[serde(default)]
    pub privacy_detector: Option<String>,
}

#[derive(Debug, Default, Deserialize, ToSchema)]
pub struct UsageBody {
    #[serde(default)]
    pub prompt_tokens: i64,
    #[serde(default)]
    pub completion_tokens: i64,
    #[serde(default)]
    pub cached_tokens: i64,
    #[serde(default)]
    pub reasoning_tokens: i64,
    #[serde(default)]
    pub total_tokens: i64,
}

/// Record one proxied request.
pub async fn ingest(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<RecordBody>,
) -> Result<(StatusCode, Json<RecordedRequest>), ApiError> {
    authorised(&headers)?;
    let environment = crate::services::media_service::bridge_environment(&state.services).await?;
    let sandbox_id = sandbox_id_by_name(&state.services, environment, &body.sandbox).await?;
    let usage = body.usage.unwrap_or_default();
    let row = NewAccountingRow {
        environment_id: environment,
        sandbox_id,
        agent_instance: body.agent_instance,
        provider: body.provider,
        model: body.model,
        direction: body
            .direction
            .as_deref()
            .and_then(Direction::parse)
            .unwrap_or(Direction::Out),
        endpoint: body.endpoint,
        status: body.status.unwrap_or(0),
        latency_ms: body.latency_ms.unwrap_or(0),
        usage: Usage::new(
            usage.prompt_tokens,
            usage.completion_tokens,
            usage.cached_tokens,
            usage.reasoning_tokens,
        ),
        content_digest: body.content_digest,
        reported_cost_micros: None,
        privacy_action: body.privacy_action,
        privacy_detector: body.privacy_detector,
    };
    let recorded = accounting_service::record(&state.services, row).await?;
    Ok((StatusCode::CREATED, Json(recorded)))
}

/// Inspect one outbound body for the proxy.
#[derive(Debug, Deserialize, ToSchema)]
pub struct InspectBody {
    pub sandbox: String,
    #[serde(default)]
    pub direction: Option<String>,
    pub content: String,
    #[serde(default)]
    pub destination: Option<String>,
}

/// The privacy decision the proxy acts on.
pub async fn inspect(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<InspectBody>,
) -> Result<Json<InspectResult>, ApiError> {
    authorised(&headers)?;
    let environment = crate::services::media_service::bridge_environment(&state.services).await?;
    let sandbox_id = sandbox_id_by_name(&state.services, environment, &body.sandbox).await?;
    let direction = body
        .direction
        .as_deref()
        .and_then(privacy_service::Direction::parse)
        .unwrap_or(privacy_service::Direction::Inbound);
    use crate::domain::auth::{CredentialSource, Principal, Role};
    // The proxy speaks for the appliance, not for a person: a system principal in
    // the sandbox's own environment, with no user id.
    let system = Principal {
        user_id: Uuid::nil(),
        environment_id: environment,
        role: Role::Admin,
        display_name: "accounting proxy".into(),
        via: CredentialSource::Token,
    };
    Ok(Json(
        privacy_service::inspect(
            &state.services,
            &system,
            sandbox_id,
            direction,
            &body.content,
            body.destination.as_deref(),
        )
        .await?,
    ))
}

/// Resolve a sandbox by name inside one environment.
async fn sandbox_id_by_name(
    ctx: &crate::services::ServiceContext,
    environment: Uuid,
    name: &str,
) -> Result<Uuid, ApiError> {
    let mut scope = crate::store::TenantScope::begin(&ctx.pool, environment)
        .await
        .map_err(ApiError::from)?;
    let id = crate::store::postgres::media::sandbox_id_by_name(&mut scope, name)
        .await
        .map_err(ApiError::from)?;
    scope.commit().await.map_err(ApiError::from)?;
    id.ok_or_else(|| ApiError::not_found(ErrorCode::NOT_FOUND, "No such sandbox in this environment."))
}

