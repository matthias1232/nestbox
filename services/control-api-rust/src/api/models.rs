//! Model catalog, hardware, and install endpoints.

use axum::Json;
use axum::extract::{Path, Query, State};
use serde::Deserialize;

use crate::api::principal::Auth;
use crate::error::ApiError;
use crate::services::model_service;
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct CatalogQuery {
    /// Force a fresh hardware probe instead of using the cached result.
    #[serde(default)]
    pub refresh_hardware: bool,
}

/// The catalog, classified against this machine.
pub async fn catalog(
    State(state): State<AppState>,
    auth: Auth,
    Query(query): Query<CatalogQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let models =
        model_service::catalog(&state.services, &auth.principal, query.refresh_hardware).await?;
    let hardware = model_service::hardware(&state.services, &auth.principal, false).await?;
    Ok(Json(serde_json::json!({
        "items": models,
        "hardware": {
            "gpus": hardware.gpus,
            "total_vram_bytes": hardware.total_vram_bytes,
            "unified_memory_bytes": hardware.unified_memory_bytes,
            "addressable_memory_bytes": hardware.addressable_memory_bytes(),
            "gpu_count": hardware.gpu_count(),
            "detected_at": hardware.detected_at,
            "note": hardware.detection_note,
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
    let profile =
        model_service::hardware(&state.services, &auth.principal, query.refresh_hardware).await?;
    Ok(Json(serde_json::json!({
        "gpus": profile.gpus,
        "total_vram_bytes": profile.total_vram_bytes,
        "unified_memory_bytes": profile.unified_memory_bytes,
        "addressable_memory_bytes": profile.addressable_memory_bytes(),
        "gpu_count": profile.gpu_count(),
        "cuda_version": profile.cuda_version,
        "driver_version": profile.driver_version,
        "detected_at": profile.detected_at,
        "note": profile.detection_note,
    })))
}

/// Install a model.
pub async fn install(
    State(state): State<AppState>,
    auth: Auth,
    Path(key): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    Ok(Json(
        model_service::install(&state.services, &auth.principal, &key).await?,
    ))
}

/// Uninstall a model.
pub async fn uninstall(
    State(state): State<AppState>,
    auth: Auth,
    Path(key): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    Ok(Json(
        model_service::uninstall(&state.services, &auth.principal, &key).await?,
    ))
}
