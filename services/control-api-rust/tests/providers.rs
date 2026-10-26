//! Verhalten der Provider-Profile und der Secret-Behandlung.

mod support;

use axum::http::StatusCode;
use serde_json::json;
use support::{call, call_raw, harness};

const SECRET: &str = "sk-test-geheim-123";

#[tokio::test]
async fn never_returns_the_api_key() {
    let harness = harness().await;

    let (status, raw) = call_raw(
        &harness.router,
        "POST",
        "/v1/providers",
        Some(json!({
            "name": "omniroute",
            "base_url": "http://omniroute:20128/v1",
            "model": "auto",
            "api_key": SECRET
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert!(!raw.contains(SECRET), "Antwort darf den Key nicht enthalten");

    let body: serde_json::Value = serde_json::from_str(&raw).expect("JSON");
    assert_eq!(body["api_key_configured"], true);

    let (_, list_raw) = call_raw(&harness.router, "GET", "/v1/providers", None).await;
    assert!(!list_raw.contains(SECRET), "Liste darf den Key nicht enthalten");
}

#[tokio::test]
async fn rejects_duplicate_and_invalid_profiles() {
    let harness = harness().await;
    let payload = json!({
        "name": "omniroute",
        "base_url": "http://omniroute:20128/v1",
        "model": "auto"
    });

    let (first, _) = call(&harness.router, "POST", "/v1/providers", Some(payload.clone())).await;
    let (second, body) = call(&harness.router, "POST", "/v1/providers", Some(payload)).await;
    let (invalid, _) = call(
        &harness.router,
        "POST",
        "/v1/providers",
        Some(json!({ "name": "leer", "base_url": "", "model": "" })),
    )
    .await;

    assert_eq!(first, StatusCode::CREATED);
    assert_eq!(second, StatusCode::CONFLICT);
    assert_eq!(body["error"]["code"], "provider.conflict");
    assert_eq!(invalid, StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn injects_provider_credentials_into_the_sandbox_environment() {
    let harness = harness().await;
    call(
        &harness.router,
        "POST",
        "/v1/providers",
        Some(json!({
            "name": "omniroute",
            "base_url": "http://omniroute:20128/v1",
            "model": "auto",
            "api_key": SECRET
        })),
    )
    .await;

    let (status, _) = call(
        &harness.router,
        "POST",
        "/v1/sandboxes",
        Some(json!({ "name": "hermes-01", "pack_key": "hermes-agent", "provider_id": "omniroute" })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);

    let specs = harness.runtime.created_specs();
    let environment: std::collections::BTreeMap<String, String> =
        specs[0].environment.iter().cloned().collect();

    assert_eq!(
        environment.get("OPENAI_BASE_URL").map(String::as_str),
        Some("http://omniroute:20128/v1")
    );
    assert_eq!(environment.get("OPENAI_MODEL").map(String::as_str), Some("auto"));
    assert_eq!(environment.get("OPENAI_API_KEY").map(String::as_str), Some(SECRET));
}

#[tokio::test]
async fn rejects_unknown_provider_reference() {
    let harness = harness().await;
    let (status, body) = call(
        &harness.router,
        "POST",
        "/v1/sandboxes",
        Some(json!({ "name": "hermes-02", "provider_id": "unbekannt" })),
    )
    .await;

    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(body["error"]["code"], "request.invalid");
}
