//! HTTP-Schicht: ein Modul pro Ressource, ein Router, eine Komposition.

use axum::Router;
use axum::routing::get;

use crate::state::AppState;

pub mod bindings;
pub mod packs;
pub mod providers;
pub mod reference;
pub mod sandboxes;
pub mod system;
pub mod tools;
pub mod topology;

/// Baut den vollständigen API-Router.
///
/// Alle fachlichen Endpunkte liegen unter `/v1`, passend zum Reverse-Proxy
/// (`/api/` → `:8000/`), sodass die GUI `/api/v1/...` aufruft.
pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/healthz", get(system::health))
        .nest("/v1", v1())
        .with_state(state)
}

fn v1() -> Router<AppState> {
    Router::new()
        .route("/healthz", get(system::health))
        .route("/runtime", get(system::runtime_status))
        .route("/schema", get(system::schema))
        .route("/events", get(system::events))
        .route("/packs", get(packs::list))
        .route("/providers", get(providers::list).post(providers::create))
        .route("/tool-profiles", get(tools::list).post(tools::create))
        .route("/tool-catalog", get(tools::catalog))
        .route("/sandboxes", get(sandboxes::list).post(sandboxes::create))
        .route("/sandboxes/{id}", get(sandboxes::get))
        .route(
            "/sandboxes/{id}/lifecycle",
            axum::routing::post(sandboxes::lifecycle),
        )
        .route("/sandboxes/{id}/logs", get(sandboxes::logs))
        .route("/bindings", get(bindings::list).post(bindings::create))
        .route("/topology", get(topology::get))
        .route(
            "/reference/deploy",
            axum::routing::post(reference::deploy_reference),
        )
}
