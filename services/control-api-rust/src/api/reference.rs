//! Endpunkt für das Referenz-Deployment der Phase 1.

use axum::Json;
use axum::extract::State;

use crate::services::reference::{DeploymentReport, deploy};
use crate::state::AppState;

#[utoipa::path(
    post,
    path = "/v1/reference/deploy",
    tag = "sandboxes",
    summary = "Referenzumgebung anlegen (dev-01, gateway-01, hermes-01)",
    responses((status = 200, body = DeploymentReport))
)]
pub async fn deploy_reference(State(state): State<AppState>) -> Json<DeploymentReport> {
    Json(deploy(&state).await)
}
