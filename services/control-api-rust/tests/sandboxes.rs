//! Verhalten der Sandbox-Endpunkte.

mod support;

use axum::http::StatusCode;
use serde_json::json;
use support::{call, harness};

#[tokio::test]
async fn creates_a_sandbox_from_a_pack() {
    let harness = harness().await;
    let (status, body) = call(
        &harness.router,
        "POST",
        "/v1/sandboxes",
        Some(json!({ "name": "hermes-01", "pack_key": "hermes-agent" })),
    )
    .await;

    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(body["name"], "hermes-01");
    assert_eq!(body["status"], "running");
    assert_eq!(body["tool_profile"], "development-default");
    assert_eq!(body["runtime"]["ip"], "172.30.0.2");
}

#[tokio::test]
async fn rejects_duplicate_names() {
    let harness = harness().await;
    let payload = json!({ "name": "dev-01" });

    let (first, _) = call(
        &harness.router,
        "POST",
        "/v1/sandboxes",
        Some(payload.clone()),
    )
    .await;
    let (second, body) = call(&harness.router, "POST", "/v1/sandboxes", Some(payload)).await;

    assert_eq!(first, StatusCode::CREATED);
    assert_eq!(second, StatusCode::CONFLICT);
    assert_eq!(body["error"]["code"], "sandbox.conflict");
}

#[tokio::test]
async fn rejects_unknown_pack_and_invalid_name() {
    let harness = harness().await;

    let (unknown_pack, _) = call(
        &harness.router,
        "POST",
        "/v1/sandboxes",
        Some(json!({ "name": "x-01", "pack_key": "does-not-exist" })),
    )
    .await;
    let (invalid_name, _) = call(
        &harness.router,
        "POST",
        "/v1/sandboxes",
        Some(json!({ "name": "UPPER_CASE" })),
    )
    .await;

    assert_eq!(unknown_pack, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(invalid_name, StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn lists_and_mutates_lifecycle() {
    let harness = harness().await;
    let (_, created) = call(
        &harness.router,
        "POST",
        "/v1/sandboxes",
        Some(json!({ "name": "gateway-01", "pack_key": "openclaw-gateway" })),
    )
    .await;
    let id = created["id"].as_str().expect("ID").to_string();

    let (list_status, list) = call(&harness.router, "GET", "/v1/sandboxes", None).await;
    assert_eq!(list_status, StatusCode::OK);
    assert_eq!(list.as_array().expect("Liste").len(), 1);

    let (stop_status, stopped) = call(
        &harness.router,
        "POST",
        &format!("/v1/sandboxes/{id}/lifecycle"),
        Some(json!({ "action": "stop" })),
    )
    .await;
    assert_eq!(stop_status, StatusCode::OK, "Antwort: {stopped}");
    assert_eq!(stopped["status"], "exited");

    let (_, started) = call(
        &harness.router,
        "POST",
        &format!("/v1/sandboxes/{id}/lifecycle"),
        Some(json!({ "action": "start" })),
    )
    .await;
    assert_eq!(started["status"], "running");

    let (_, removed) = call(
        &harness.router,
        "POST",
        &format!("/v1/sandboxes/{id}/lifecycle"),
        Some(json!({ "action": "remove" })),
    )
    .await;
    assert_eq!(removed["status"], "deleted");

    let (_, empty) = call(&harness.router, "GET", "/v1/sandboxes", None).await;
    assert_eq!(empty.as_array().expect("Liste").len(), 0);
}

#[tokio::test]
async fn returns_logs_and_handles_unknown_ids() {
    let harness = harness().await;
    let (_, created) = call(
        &harness.router,
        "POST",
        "/v1/sandboxes",
        Some(json!({ "name": "dev-02" })),
    )
    .await;
    let id = created["id"].as_str().expect("ID").to_string();

    let (logs_status, logs) = call(
        &harness.router,
        "GET",
        &format!("/v1/sandboxes/{id}/logs?tail=50"),
        None,
    )
    .await;
    assert_eq!(logs_status, StatusCode::OK);
    assert_eq!(logs["tail"], 50);
    assert!(logs["logs"].as_str().expect("Logs").contains("dev-02"));

    let (missing_status, missing) =
        call(&harness.router, "GET", "/v1/sandboxes/unknown", None).await;
    assert_eq!(missing_status, StatusCode::NOT_FOUND);
    assert_eq!(missing["error"]["code"], "sandbox.not_found");
}

#[tokio::test]
async fn generates_a_gateway_token_that_stays_server_side() {
    let harness = harness().await;
    let (status, created) = call(
        &harness.router,
        "POST",
        "/v1/sandboxes",
        Some(json!({ "name": "gateway-09", "pack_key": "openclaw-gateway" })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);

    let specs = harness.runtime.created_specs();
    let environment: std::collections::BTreeMap<String, String> =
        specs[0].environment.iter().cloned().collect();
    let token = environment
        .get("OPENCLAW_GATEWAY_TOKEN")
        .expect("Token wird injiziert");
    assert!(!token.is_empty());

    // Das Token darf in der Antwort nicht auftauchen.
    assert!(!created.to_string().contains(token));
}

#[tokio::test]
async fn applies_pack_resource_defaults_and_allows_overrides() {
    let harness = harness().await;

    // Ohne Angabe: Empfehlung des Packs (Gateway braucht mehr als 1 GB).
    call(
        &harness.router,
        "POST",
        "/v1/sandboxes",
        Some(json!({ "name": "gateway-10", "pack_key": "openclaw-gateway" })),
    )
    .await;
    // Mit Angabe: der explizite Wert gewinnt.
    call(
        &harness.router,
        "POST",
        "/v1/sandboxes",
        Some(json!({ "name": "dev-10", "memory_limit": "512m", "cpu_limit": 0.5 })),
    )
    .await;

    let specs = harness.runtime.created_specs();
    let gateway = specs.iter().find(|spec| spec.name == "gateway-10").expect("Gateway");
    let dev = specs.iter().find(|spec| spec.name == "dev-10").expect("Dev");

    assert_eq!(gateway.memory_limit, "2g");
    assert_eq!(dev.memory_limit, "512m");
    assert_eq!(dev.cpu_limit, 0.5);
}
