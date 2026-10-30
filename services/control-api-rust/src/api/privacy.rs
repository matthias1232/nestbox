//! Content privacy (DLP): the policy, a single filter test, incidents, purge.
//!
//! The handlers parse and delegate; the rules live in `services::privacy_service`
//! and the pure mode matrix in `domain::privacy`. Nothing here ever returns a
//! detected value: the service hands back masked strings and fingerprints only.

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::api::principal::Auth;
use crate::domain::privacy::{Detector, PrivacySettings, PrivacyUpdate};
use crate::error::ApiError;
use crate::services::privacy_service::{self, InspectResult};
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct ScopeQuery {
    pub sandbox_id: Option<Uuid>,
    #[serde(default)]
    pub limit: Option<i64>,
}

/// One detector as the console lists it.
#[derive(Debug, Serialize, ToSchema)]
pub struct DetectorView {
    pub key: String,
    pub label: String,
    pub default_action: String,
}

/// The settings screen: the effective row and the catalog it refers to.
#[derive(Debug, Serialize, ToSchema)]
pub struct SettingsView {
    pub settings: PrivacySettings,
    pub detectors: Vec<DetectorView>,
}

fn catalog() -> Vec<DetectorView> {
    Detector::ALL
        .iter()
        .map(|detector| DetectorView {
            key: detector.as_str().to_string(),
            label: detector.label().to_string(),
            default_action: detector.default_action().as_str().to_string(),
        })
        .collect()
}

/// The environment default, or one sandbox's effective settings.
pub async fn get_settings(
    State(state): State<AppState>,
    auth: Auth,
    Query(scope): Query<ScopeQuery>,
) -> Result<Json<SettingsView>, ApiError> {
    let settings =
        privacy_service::effective_settings(&state.services, &auth.principal, scope.sandbox_id).await?;
    Ok(Json(SettingsView { settings, detectors: catalog() }))
}

/// A save request: the scope from the body, the values from the update.
#[derive(Debug, Deserialize, ToSchema)]
pub struct SaveBody {
    #[serde(default)]
    pub sandbox_id: Option<Uuid>,
    #[serde(flatten)]
    pub update: PrivacyUpdate,
}

/// Replace the policy for the environment (`sandbox_id` absent) or one sandbox.
pub async fn put_settings(
    State(state): State<AppState>,
    auth: Auth,
    Json(body): Json<SaveBody>,
) -> Result<Json<SettingsView>, ApiError> {
    let settings = privacy_service::set_settings(
        &state.services,
        &auth.principal,
        body.sandbox_id,
        body.update,
    )
    .await?;
    Ok(Json(SettingsView { settings, detectors: catalog() }))
}

/// Drop a sandbox's override so it inherits the environment default again.
pub async fn clear_settings(
    State(state): State<AppState>,
    auth: Auth,
    Path(sandbox_id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    privacy_service::clear_settings(&state.services, &auth.principal, sandbox_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// One filter test.
#[derive(Debug, Deserialize, ToSchema)]
pub struct InspectBody {
    pub sandbox_id: Uuid,
    /// `inbound` (into the model) or `outbound` (back to the agent).
    pub direction: String,
    pub content: String,
    #[serde(default)]
    pub destination: Option<String>,
}

/// Run the policy over one piece of text and answer what would be sent.
pub async fn inspect(
    State(state): State<AppState>,
    auth: Auth,
    Json(body): Json<InspectBody>,
) -> Result<Json<InspectResult>, ApiError> {
    let direction = privacy_service::Direction::parse(&body.direction)
        .ok_or_else(|| ApiError::validation("direction must be 'inbound' or 'outbound'."))?;
    Ok(Json(
        privacy_service::inspect(
            &state.services,
            &auth.principal,
            body.sandbox_id,
            direction,
            &body.content,
            body.destination.as_deref(),
        )
        .await?,
    ))
}

/// The recent incidents of one sandbox, newest first.
pub async fn get_incidents(
    State(state): State<AppState>,
    auth: Auth,
    Query(scope): Query<ScopeQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let sandbox_id = scope
        .sandbox_id
        .ok_or_else(|| ApiError::validation("sandbox_id is required."))?;
    let items = privacy_service::list_incidents(
        &state.services,
        &auth.principal,
        sandbox_id,
        scope.limit.unwrap_or(50),
    )
    .await?;
    Ok(Json(serde_json::json!({ "items": items })))
}

/// How many days of incidents to keep.
#[derive(Debug, Deserialize, ToSchema)]
pub struct PurgeBody {
    #[serde(default = "default_days")]
    pub days: i64,
}

fn default_days() -> i64 {
    30
}

/// Delete incidents older than `days`. Admin only.
pub async fn purge(
    State(state): State<AppState>,
    auth: Auth,
    Json(body): Json<PurgeBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let removed = privacy_service::purge(&state.services, &auth.principal, body.days).await?;
    Ok(Json(serde_json::json!({ "removed": removed, "older_than_days": body.days })))
}
