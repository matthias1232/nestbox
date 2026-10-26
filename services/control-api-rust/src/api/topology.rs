//! Endpunkt für das Topologiebild.

use axum::Json;
use axum::extract::State;

use crate::domain::topology::Topology;
use crate::services::topology as service;
use crate::state::AppState;

#[utoipa::path(
    get,
    path = "/v1/topology",
    tag = "topology",
    summary = "Knoten und Kanten für die grafische Übersicht",
    responses((status = 200, body = Topology))
)]
pub async fn get(State(state): State<AppState>) -> Json<Topology> {
    Json(service::topology(&state).await)
}
