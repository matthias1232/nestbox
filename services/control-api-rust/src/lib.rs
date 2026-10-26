//! Nestbox Control API – Bibliothek.
//!
//! Die Binärdatei (`main.rs`) startet nur. Alles Fachliche liegt hier, damit
//! Tests die API ohne Netzwerk und ohne Docker bauen können.

pub mod api;
pub mod audit;
pub mod config;
pub mod domain;
pub mod error;
pub mod openapi;
pub mod runtime;
pub mod services;
pub mod state;

use std::sync::Arc;

use axum::Router;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

use crate::config::Config;
use crate::error::ApiError;
use crate::openapi::ApiDoc;
use crate::runtime::RuntimeAdapter;
use crate::state::AppState;

/// Baut State und Router für den Start (und für Tests).
pub async fn build(config: Config, runtime: Arc<dyn RuntimeAdapter>) -> Result<AppState, ApiError> {
    AppState::load(config, runtime).await
}

/// Baut die vollständige Anwendung inklusive API-Dokumentation.
pub fn app(state: AppState) -> Router {
    api::router(state)
        .merge(SwaggerUi::new("/docs").url("/v1/openapi.json", ApiDoc::openapi()))
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
}
