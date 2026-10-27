//! Provider endpoint management.
//!
//! Nothing here ever accepts or returns a credential value, only a locator. That
//! is the point of the resource: it is how an operator points the platform at a
//! model endpoint without the platform becoming a secret store.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use serde::Deserialize;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::api::principal::Auth;
use crate::domain::provider::{ProviderKind, ProviderProfile};
use crate::error::ApiError;
use crate::services::provider_service::{self, NewProvider, ProbeResult};
use crate::state::AppState;

/// List provider profiles.
pub async fn list(
    State(state): State<AppState>,
    auth: Auth,
) -> Result<Json<serde_json::Value>, ApiError> {
    let items = provider_service::list(&state.services, &auth.principal).await?;
    Ok(Json(serde_json::json!({ "items": items })))
}

/// One provider profile.
pub async fn get(
    State(state): State<AppState>,
    auth: Auth,
    Path(id): Path<Uuid>,
) -> Result<Json<ProviderProfile>, ApiError> {
    Ok(Json(
        provider_service::get(&state.services, &auth.principal, id).await?,
    ))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateProviderRequest {
    pub name: String,
    /// One of: nvidia, openai, anthropic, openai_compatible, local_vllm,
    /// omniroute.
    pub kind: String,
    pub endpoint: String,
    pub model: Option<String>,
    /// A locator such as `file:///secrets/omniroute_api_key`. Never a value.
    pub secret_ref: Option<String>,
    #[serde(default)]
    pub labels: Option<serde_json::Value>,
}

/// Register a provider profile.
pub async fn create(
    State(state): State<AppState>,
    auth: Auth,
    Json(body): Json<CreateProviderRequest>,
) -> Result<(StatusCode, Json<ProviderProfile>), ApiError> {
    let kind = ProviderKind::parse(&body.kind).ok_or_else(|| {
        ApiError::validation(
            "Unknown provider kind; expected nvidia, openai, anthropic, \
             openai_compatible, local_vllm or omniroute.",
        )
    })?;

    let profile = provider_service::create(
        &state.services,
        &auth.principal,
        NewProvider {
            name: body.name,
            kind,
            endpoint: body.endpoint,
            model: body.model,
            secret_ref: body.secret_ref,
            labels: body.labels.unwrap_or_else(|| serde_json::json!({})),
        },
    )
    .await?;

    Ok((StatusCode::CREATED, Json(profile)))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateProviderRequest {
    pub endpoint: String,
    pub model: Option<String>,
    pub secret_ref: Option<String>,
    #[serde(default)]
    pub labels: Option<serde_json::Value>,
    pub version: i64,
}

/// Change a provider profile. Name and kind are immutable.
pub async fn update(
    State(state): State<AppState>,
    auth: Auth,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateProviderRequest>,
) -> Result<Json<ProviderProfile>, ApiError> {
    let profile = provider_service::update(
        &state.services,
        &auth.principal,
        id,
        body.version,
        body.endpoint,
        body.model,
        body.secret_ref,
        body.labels.unwrap_or_else(|| serde_json::json!({})),
    )
    .await?;
    Ok(Json(profile))
}

/// Remove a provider profile.
pub async fn delete(
    State(state): State<AppState>,
    auth: Auth,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    provider_service::delete(&state.services, &auth.principal, id).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Probe a provider and report what happened.
pub async fn probe(
    State(state): State<AppState>,
    auth: Auth,
    Path(id): Path<Uuid>,
) -> Result<Json<ProbeResult>, ApiError> {
    Ok(Json(
        provider_service::probe(&state.services, &auth.principal, id).await?,
    ))
}
