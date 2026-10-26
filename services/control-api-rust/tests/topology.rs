//! Bindings und Topologie.

mod support;

use axum::http::StatusCode;
use serde_json::json;
use support::{call, harness};

async fn two_sandboxes(harness: &support::Harness) {
    for (name, pack) in [
        ("gateway-01", "openclaw-gateway"),
        ("hermes-01", "hermes-agent"),
    ] {
        let (status, _) = call(
            &harness.router,
            "POST",
            "/v1/sandboxes",
            Some(json!({ "name": name, "pack_key": pack })),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED);
    }
}

#[tokio::test]
async fn builds_topology_from_sandboxes_and_bindings() {
    let harness = harness().await;
    two_sandboxes(&harness).await;

    let (binding_status, binding) = call(
        &harness.router,
        "POST",
        "/v1/bindings",
        Some(json!({
            "name": "hermes-to-gateway",
            "source": "hermes-01",
            "target": "gateway-01",
            "port": 8080,
            "purpose": "Hermes service integration"
        })),
    )
    .await;
    assert_eq!(binding_status, StatusCode::CREATED);
    assert_eq!(binding["source"], "hermes-01");

    let (status, topology) = call(&harness.router, "GET", "/v1/topology", None).await;
    assert_eq!(status, StatusCode::OK);

    let nodes = topology["nodes"].as_array().expect("Knoten");
    assert_eq!(nodes.len(), 3, "Runtime plus zwei Sandboxes");
    assert_eq!(nodes[0]["id"], "runtime");

    let edges = topology["edges"].as_array().expect("Kanten");
    assert_eq!(edges.len(), 3, "zwei Runtime-Kanten plus ein Binding");
    assert!(
        edges
            .iter()
            .any(|edge| edge["source"] == "hermes-01" && edge["target"] == "gateway-01")
    );
}

#[tokio::test]
async fn rejects_bindings_to_unknown_sandboxes() {
    let harness = harness().await;
    two_sandboxes(&harness).await;

    let (status, body) = call(
        &harness.router,
        "POST",
        "/v1/bindings",
        Some(json!({ "name": "broken", "source": "hermes-01", "target": "does-not-exist" })),
    )
    .await;

    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(body["error"]["code"], "request.invalid");
}
