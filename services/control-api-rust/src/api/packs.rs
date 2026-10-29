//! Baselines, agent packs, and the tool catalog.
//!
//! Three read-only catalogs. A handler parses `?baseline=` and delegates; the
//! compatibility rule and the package-manager resolution live in the service so
//! the same answers are available to the builder without going through HTTP.

use axum::Json;
use axum::extract::{Query, State};
use serde::Deserialize;

use crate::api::principal::Auth;
use crate::error::ApiError;
use crate::domain::runtime::CatalogueAnswer;
use crate::services::catalogue_service;
use crate::services::pack_service;
use crate::state::AppState;

/// The one query parameter the catalogs share.
///
/// Naming a baseline narrows the packs to those it can carry and resolves the
/// tool names for its package manager. Omitted or empty means "no baseline",
/// which shows the whole catalog without claiming compatibility.
#[derive(Debug, Deserialize)]
pub struct CatalogQuery {
    pub baseline: Option<String>,
}

/// Base operating systems, recommended first.
pub async fn baselines(
    State(state): State<AppState>,
    auth: Auth,
) -> Result<Json<serde_json::Value>, ApiError> {
    let baselines = pack_service::list_baselines(&state.services, &auth.principal).await?;
    Ok(Json(serde_json::json!({ "items": baselines })))
}

/// Agent packs, optionally only those a baseline supports.
pub async fn agent_packs(
    State(state): State<AppState>,
    auth: Auth,
    Query(query): Query<CatalogQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let packs = pack_service::list_agent_packs(
        &state.services,
        &auth.principal,
        query.baseline.as_deref(),
    )
    .await?;
    Ok(Json(serde_json::json!({ "items": packs })))
}

/// The tool catalog, optionally resolved for one baseline's package manager.
pub async fn tool_catalog(
    State(state): State<AppState>,
    auth: Auth,
    Query(query): Query<CatalogQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let tools = pack_service::tool_catalog(
        &state.services,
        &auth.principal,
        query.baseline.as_deref(),
    )
    .await?;
    Ok(Json(serde_json::json!({ "items": tools })))
}

/// A live package search: which distribution, and what fragment.
///
/// The baseline decides the distribution and the package manager; omitting it
/// keeps the console's current baseline usable without a second request.
#[derive(Debug, Deserialize)]
pub struct PackageQuery {
    pub baseline: Option<String>,
    pub q: Option<String>,
}

/// Search the live package index of one baseline's distribution.
pub async fn catalogue(
    State(state): State<AppState>,
    auth: Auth,
    Query(query): Query<PackageQuery>,
) -> Result<Json<CatalogueAnswer>, ApiError> {
    let baseline = query.baseline.as_deref().unwrap_or("ubuntu-24.04");
    let answer = catalogue_service::search(
        &state.services,
        &auth.principal,
        baseline,
        query.q.as_deref().unwrap_or(""),
    )
    .await?;
    Ok(Json(answer))
}
