//! System endpoints: health, readiness, runtime facts, and the API document.
//!
//! Health and readiness answer different questions on purpose:
//! `healthz` says the process is alive, `readyz` says it can actually serve —
//! which means the database answered. A load balancer that only checks liveness
//! will send traffic to a process that cannot reach its store.

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use serde::Serialize;

use crate::api::principal::Auth;
use crate::error::ApiError;
use crate::openapi;
use crate::state::AppState;

/// Liveness: the process is running.
pub async fn healthz() -> &'static str {
    "ok"
}

/// Readiness: the process can serve, including its database.
pub async fn readyz(State(state): State<AppState>) -> impl IntoResponse {
    if crate::store::postgres::is_ready(state.pool()).await {
        (StatusCode::OK, "ready")
    } else {
        tracing::error!("readiness check failed");
        (StatusCode::SERVICE_UNAVAILABLE, "database unavailable")
    }
}

#[derive(Serialize)]
pub struct RuntimeInfo {
    pub runtime: String,
    pub network: String,
    pub api_version: &'static str,
}

/// What is actually executing sandboxes, stated honestly.
pub async fn runtime_info(State(state): State<AppState>) -> Json<RuntimeInfo> {
    Json(RuntimeInfo {
        runtime: state.services.runtime.name().to_string(),
        network: state.services.runtime.network().to_string(),
        api_version: "v1",
    })
}

/// The OpenAPI document.
pub async fn openapi() -> Json<serde_json::Value> {
    Json(openapi::document())
}

/// A minimal documentation page.
///
/// Swagger UI is deliberately not embedded: it is a large third-party bundle
/// served from the same origin as an authenticated console, and the generated
/// document plus `curl` is enough for the operator this ships to. The page says
/// so rather than pretending a full UI exists.
pub async fn docs() -> impl IntoResponse {
    (
        StatusCode::OK,
        [("content-type", "text/html; charset=utf-8")],
        r#"<!doctype html><html lang="en"><head><meta charset="utf-8">
<title>Nestbox API</title>
<style>body{font:16px/1.6 system-ui,sans-serif;max-width:52rem;margin:3rem auto;padding:0 1rem}
code{background:#eee;padding:.1rem .3rem;border-radius:3px}</style></head><body>
<h1>Nestbox API v1</h1>
<p>The machine-readable contract is at <a href="/api/v1/openapi.json">/api/v1/openapi.json</a>
and also at <code>/api/schema/v1</code>. The prefix <code>/api/v1/</code> is frozen;
a breaking change gets a new prefix.</p>
<p>Authenticate with a session cookie from <code>POST /api/v1/auth/login</code>
(mutating requests also need the <code>x-csrf-token</code> header), or with
<code>Authorization: Token &lt;key&gt;</code> for automation.</p>
</body></html>"#,
    )
}

/// The environments this user may access.
pub async fn environments(
    State(state): State<AppState>,
    auth: Auth,
) -> Result<Json<serde_json::Value>, ApiError> {
    let environments =
        crate::services::environment_service::environments_for_user(&state.services, &auth.principal)
            .await?;
    Ok(Json(serde_json::json!({ "items": environments })))
}

/// A summary of the environment the request is scoped to.
pub async fn current_environment(
    State(state): State<AppState>,
    auth: Auth,
) -> Result<Json<serde_json::Value>, ApiError> {
    let summary =
        crate::services::environment_service::summary(&state.services, &auth.principal).await?;

    Ok(Json(serde_json::json!({
        "id": summary.id,
        "name": summary.name,
        "slug": summary.slug,
        "sandboxes": summary.sandbox_count,
        "gateways": summary.gateway_count,
        "running": summary.running_count,
        "role": auth.principal.role.as_str(),
        "display_name": auth.principal.display_name,
    })))
}

/// Recent audit events.
pub async fn audit_events(
    State(state): State<AppState>,
    auth: Auth,
) -> Result<Json<serde_json::Value>, ApiError> {
    let events =
        crate::services::audit_service::list(&state.services, &auth.principal, 200).await?;
    Ok(Json(serde_json::json!({ "items": events })))
}
