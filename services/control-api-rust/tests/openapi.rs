//! Der veröffentlichte API-Vertrag.
//!
//! Diese Tests schützen genau das, was die GUI und externe Clients erwarten:
//! Pfade, Schemas und die Zusicherung, dass Secrets nicht im Vertrag stehen.

mod support;

use axum::http::StatusCode;
use support::{call, call_raw, harness};

#[tokio::test]
async fn schema_contains_all_public_endpoints() {
    let harness = harness().await;
    let (status, schema) = call(&harness.router, "GET", "/v1/schema", None).await;
    assert_eq!(status, StatusCode::OK);

    let paths = schema["paths"].as_object().expect("Pfade");
    for expected in [
        "/v1/sandboxes",
        "/v1/sandboxes/{id}",
        "/v1/sandboxes/{id}/lifecycle",
        "/v1/sandboxes/{id}/logs",
        "/v1/providers",
        "/v1/tool-profiles",
        "/v1/tool-catalog",
        "/v1/bindings",
        "/v1/topology",
        "/v1/packs",
        "/v1/runtime",
        "/v1/events",
        "/v1/healthz",
    ] {
        assert!(paths.contains_key(expected), "Pfad fehlt: {expected}");
    }
}

#[tokio::test]
async fn schema_documents_types_and_keeps_api_key_write_only() {
    let harness = harness().await;
    let (_, schema) = call(&harness.router, "GET", "/v1/schema", None).await;

    let schemas = schema["components"]["schemas"]
        .as_object()
        .expect("Schemas");
    for expected in [
        "Sandbox",
        "SandboxInput",
        "ProviderProfile",
        "ProviderInput",
        "Topology",
    ] {
        assert!(schemas.contains_key(expected), "Schema fehlt: {expected}");
    }

    // Der Key ist nur im Eingabeschema erlaubt, niemals im Ausgabeschema.
    assert!(schemas["ProviderInput"]["properties"]["api_key"].is_object());
    assert!(
        schemas["ProviderProfile"]["properties"]
            .get("api_key")
            .is_none()
    );
}

#[tokio::test]
async fn swagger_ui_is_served() {
    let harness = harness().await;

    // /docs leitet auf /docs/ um (SwaggerUi-Verhalten), dort liegt die Seite.
    let (redirect, _) = call_raw(&harness.router, "GET", "/docs", None).await;
    assert!(
        redirect.is_redirection(),
        "erwartet Redirect, war {redirect}"
    );

    let (status, body) = call_raw(&harness.router, "GET", "/docs/", None).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.to_lowercase().contains("swagger"));
}

#[tokio::test]
async fn health_and_runtime_identify_the_rust_implementation() {
    let harness = harness().await;

    let (status, health) = call(&harness.router, "GET", "/v1/healthz", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(health["implementation"], "rust-axum");

    let (_, runtime) = call(&harness.router, "GET", "/v1/runtime", None).await;
    assert_eq!(runtime["adapter"], "fake");
    assert_eq!(runtime["status"], "ready");
}
