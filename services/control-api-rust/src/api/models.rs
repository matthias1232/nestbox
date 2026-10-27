//! Model catalog, hardware, and install endpoints.

use axum::Json;
use axum::extract::{Path, Query, State};
use serde::Deserialize;

use crate::api::principal::Auth;
use crate::domain::model::HardwareProfile;
use crate::error::ApiError;
use crate::services::model_service;
use crate::services::model_service::CatalogQuery as CatalogFilter;
use crate::state::AppState;

/// Query parameters of the catalog.
///
/// Every filter narrows; omitting one keeps everything. `fits` is the verdict
/// switch the console's "fits this machine" toggle sends, and it selects from
/// the verdicts the server already produced — the client never re-classifies.
#[derive(Debug, Deserialize)]
pub struct CatalogQuery {
    /// Force a fresh hardware probe instead of using the cached result.
    #[serde(default)]
    pub refresh_hardware: bool,
    /// The node the fit is for; the local node when omitted.
    pub node: Option<String>,
    /// Only this family (exact, case-sensitive, as stored).
    pub family: Option<String>,
    /// Only this publisher.
    pub publisher: Option<String>,
    /// `cuda`, `cpu`, or `hybrid`.
    pub runtime: Option<String>,
    /// Only models that run without a GPU.
    #[serde(default)]
    pub cpu_only: bool,
    /// Only models that need a unified-memory pool.
    #[serde(default)]
    pub unified_only: bool,
    /// `true` keeps what fits this machine, `false` keeps the rest.
    pub fits: Option<bool>,
}

impl CatalogQuery {
    fn filter(&self) -> CatalogFilter {
        CatalogFilter {
            node: self.node.clone(),
            family: self.family.clone(),
            publisher: self.publisher.clone(),
            runtime: self.runtime.clone(),
            cpu_only: self.cpu_only,
            unified_only: self.unified_only,
            fits: self.fits,
        }
    }
}

/// The machine as JSON, in one shape for both the catalog envelope and the
/// hardware endpoint.
fn hardware_json(profile: &HardwareProfile) -> serde_json::Value {
    serde_json::json!({
        "hostname": profile.hostname,
        "cpu_model": profile.cpu_model,
        "cpu_cores": profile.cpu_cores,
        "cpu_threads": profile.cpu_threads,
        "system_ram_bytes": profile.system_ram_bytes,
        "gpus": profile.gpus,
        "total_vram_bytes": profile.total_vram_bytes,
        "unified_memory_bytes": profile.unified_memory_bytes,
        "unified_memory_source": profile.unified_memory_source,
        "is_unified_memory": profile.is_unified(),
        "addressable_memory_bytes": profile.addressable_memory_bytes(),
        "addressable_ram_bytes": profile.addressable_ram_bytes(),
        "gpu_count": profile.gpu_count(),
        "has_gpu": profile.has_gpu(),
        "cuda_version": profile.cuda_version,
        "driver_version": profile.driver_version,
        "detected_at": profile.detected_at,
        "note": profile.detection_note,
    })
}

/// The catalog, classified against what one node has left.
pub async fn catalog(
    State(state): State<AppState>,
    auth: Auth,
    Query(query): Query<CatalogQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let view = model_service::catalog(
        &state.services,
        &auth.principal,
        query.refresh_hardware,
        &query.filter(),
    )
    .await?;
    Ok(Json(serde_json::json!({
        "items": view.items,
        "hardware": hardware_json(&view.hardware),
        "budget": view.budget,
        "node": {
            "id": view.node.id,
            "name": view.node.name,
            "is_local": view.node.is_local,
            "container_gpu": view.node.container_gpu,
        },
    })))
}

/// Only the installed models.
pub async fn installed(
    State(state): State<AppState>,
    auth: Auth,
) -> Result<Json<serde_json::Value>, ApiError> {
    let models = model_service::installed(&state.services, &auth.principal).await?;
    Ok(Json(serde_json::json!({ "items": models })))
}

/// The detected hardware.
pub async fn hardware(
    State(state): State<AppState>,
    auth: Auth,
    Query(query): Query<CatalogQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let profile = model_service::hardware(
        &state.services,
        &auth.principal,
        query.refresh_hardware,
        query.node.as_deref(),
    )
    .await?;
    Ok(Json(hardware_json(&profile)))
}

/// Counts for the filter chips.
pub async fn facets(
    State(state): State<AppState>,
    auth: Auth,
) -> Result<Json<serde_json::Value>, ApiError> {
    let facets = model_service::facets(&state.services, &auth.principal).await?;
    Ok(Json(serde_json::to_value(facets).unwrap_or_else(
        |_| serde_json::json!({}),
    )))
}

#[derive(Debug, Deserialize)]
pub struct RequirementsQuery {
    /// The catalog key of the model.
    pub key: String,
}

/// One model as a requirement profile, with the verdict for this machine.
pub async fn requirements(
    State(state): State<AppState>,
    auth: Auth,
    Query(query): Query<RequirementsQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let view = model_service::requirements(&state.services, &auth.principal, &query.key).await?;
    Ok(Json(serde_json::to_value(view).unwrap_or_else(
        |_| serde_json::json!({}),
    )))
}

/// Which installs to list.
#[derive(Debug, Deserialize)]
pub struct InstallsQuery {
    /// Only installs on this node; every node when omitted.
    pub node: Option<String>,
}

/// Installs per node with their reservation and provider profile.
pub async fn installs(
    State(state): State<AppState>,
    auth: Auth,
    Query(query): Query<InstallsQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let items = model_service::installs(
        &state.services,
        &auth.principal,
        query.node.as_deref(),
    )
    .await?;
    Ok(Json(serde_json::json!({ "items": items })))
}

/// Refresh the catalog from the public sources as a job.
///
/// Filled by the `feat/catalog-sync` package.
pub async fn sync(
    State(_state): State<AppState>,
    _auth: Auth,
) -> Result<Json<serde_json::Value>, ApiError> {
    Err(ApiError::not_implemented("POST /models/sync"))
}

/// The body of an install or uninstall: which node, or the local one.
#[derive(Debug, Default, Deserialize)]
pub struct NodeRequest {
    pub node: Option<String>,
}

/// Install a model on a node.
pub async fn install(
    State(state): State<AppState>,
    auth: Auth,
    Path(key): Path<String>,
    body: Option<Json<NodeRequest>>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let node = body.and_then(|Json(request)| request.node);
    Ok(Json(
        model_service::install(&state.services, &auth.principal, &key, node.as_deref()).await?,
    ))
}

/// Uninstall a model from a node.
pub async fn uninstall(
    State(state): State<AppState>,
    auth: Auth,
    Path(key): Path<String>,
    body: Option<Json<NodeRequest>>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let node = body.and_then(|Json(request)| request.node);
    Ok(Json(
        model_service::uninstall(&state.services, &auth.principal, &key, node.as_deref()).await?,
    ))
}
