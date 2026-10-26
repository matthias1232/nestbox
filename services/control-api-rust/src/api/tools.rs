//! Endpunkte für Tool-Profile und den Werkzeugkatalog.

use axum::Json;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use serde::Deserialize;
use utoipa::IntoParams;

use crate::domain::tool_profile::{ToolDefinition, ToolProfile, ToolProfileInput};
use crate::error::{ApiError, ErrorEnvelope};
use crate::services::tools as service;
use crate::state::AppState;

/// Suchparameter für den Katalog.
#[derive(Debug, Default, Deserialize, IntoParams)]
pub struct CatalogQuery {
    /// Freitextsuche, leer = alle Werkzeuge.
    pub q: Option<String>,
}

#[utoipa::path(
    get,
    path = "/v1/tool-profiles",
    tag = "tools",
    summary = "Alle Tool-Profile",
    responses((status = 200, body = Vec<ToolProfile>))
)]
pub async fn list(State(state): State<AppState>) -> Json<Vec<ToolProfile>> {
    Json(service::list(&state).await)
}

#[utoipa::path(
    post,
    path = "/v1/tool-profiles",
    tag = "tools",
    summary = "Tool-Profil anlegen",
    request_body = ToolProfileInput,
    responses(
        (status = 201, body = ToolProfile),
        (status = 409, body = ErrorEnvelope),
        (status = 422, body = ErrorEnvelope)
    )
)]
pub async fn create(
    State(state): State<AppState>,
    Json(input): Json<ToolProfileInput>,
) -> Result<(StatusCode, Json<ToolProfile>), ApiError> {
    let profile = service::create(&state, input).await?;
    Ok((StatusCode::CREATED, Json(profile)))
}

#[utoipa::path(
    get,
    path = "/v1/tool-catalog",
    tag = "tools",
    summary = "Werkzeugkatalog durchsuchen",
    params(CatalogQuery),
    responses((status = 200, body = Vec<ToolDefinition>))
)]
pub async fn catalog(Query(query): Query<CatalogQuery>) -> Json<Vec<ToolDefinition>> {
    Json(service::catalog(query.q.as_deref().unwrap_or_default()))
}
