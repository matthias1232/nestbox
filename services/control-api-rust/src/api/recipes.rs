//! Recipes: the builder.
//!
//! Handlers are filled by the `feat/recipes-build` package.

use axum::Json;
use axum::extract::{Path, State};
use uuid::Uuid;

use crate::api::principal::Auth;
use crate::error::ApiError;
use crate::state::AppState;

/// List recipes.
pub async fn list(
    State(_state): State<AppState>,
    _auth: Auth,
) -> Result<Json<serde_json::Value>, ApiError> {
    Err(ApiError::not_implemented("GET /recipes"))
}

/// Create a draft.
pub async fn create(
    State(_state): State<AppState>,
    _auth: Auth,
    Json(_body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, ApiError> {
    Err(ApiError::not_implemented("POST /recipes"))
}

/// Read a recipe.
pub async fn get(
    State(_state): State<AppState>,
    _auth: Auth,
    Path(_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
    Err(ApiError::not_implemented("GET /recipes/{id}"))
}

/// Edit a recipe.
pub async fn update(
    State(_state): State<AppState>,
    _auth: Auth,
    Path(_id): Path<Uuid>,
    Json(_body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, ApiError> {
    Err(ApiError::not_implemented("PATCH /recipes/{id}"))
}

/// Delete a recipe.
pub async fn delete(
    State(_state): State<AppState>,
    _auth: Auth,
    Path(_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
    Err(ApiError::not_implemented("DELETE /recipes/{id}"))
}

/// Build the image.
pub async fn build(
    State(_state): State<AppState>,
    _auth: Auth,
    Path(_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
    Err(ApiError::not_implemented("POST /recipes/{id}/build"))
}

/// Create a sandbox from a built recipe.
pub async fn instantiate(
    State(_state): State<AppState>,
    _auth: Auth,
    Path(_id): Path<Uuid>,
    Json(_body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, ApiError> {
    Err(ApiError::not_implemented("POST /recipes/{id}/instantiate"))
}
