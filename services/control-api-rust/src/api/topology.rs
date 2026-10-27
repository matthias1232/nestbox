//! The cluster graph, and the live traffic edges the Admin Center adds to it.
//!
//! The existing graph is static: nodes, gateways, sandboxes, and what belongs to
//! what. The `feat/live-traffic` package extends this handler with `?live=5m`,
//! which adds edges from recent egress traffic (sandbox → destination, counted
//! and marked when anything was blocked). The graph itself is computed on the
//! server so the console never aggregates a week of rows in a browser.

use axum::Json;
use axum::extract::State;

use crate::api::principal::Auth;
use crate::error::ApiError;
use crate::services::gateway_service;
use crate::state::AppState;

/// The cluster graph.
pub async fn topology(
    State(state): State<AppState>,
    auth: Auth,
) -> Result<Json<serde_json::Value>, ApiError> {
    Ok(Json(
        gateway_service::topology(&state.services, &auth.principal).await?,
    ))
}
