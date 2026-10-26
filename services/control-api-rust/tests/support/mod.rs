//! Gemeinsame Testhilfen: App mit Fake-Runtime und temporärem State.

use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use nestbox_control_api::config::Config;
use nestbox_control_api::runtime::fake::FakeRuntime;
use nestbox_control_api::state::AppState;
use nestbox_control_api::{app, build};
use serde_json::Value;
use tempfile::TempDir;
use tower::ServiceExt;

/// Ein Testsystem aus App, State und Fake-Runtime.
pub struct Harness {
    pub router: Router,
    pub state: AppState,
    pub runtime: Arc<FakeRuntime>,
    _directory: TempDir,
}

/// Baut ein frisches Testsystem mit eigenem State-Verzeichnis.
pub async fn harness() -> Harness {
    let directory = TempDir::new().expect("Temp-Verzeichnis");
    let config = Config {
        state_path: directory.path().join("state.json"),
        secrets_path: directory.path().join("secrets.json"),
        runner_url: "http://unused".to_string(),
        runtime_network: "nestbox-test".to_string(),
        listen_addr: "127.0.0.1:0".to_string(),
    };
    let runtime = Arc::new(FakeRuntime::new());
    let state = build(config, runtime.clone())
        .await
        .expect("State wird gebaut");
    Harness {
        router: app(state.clone()),
        state,
        runtime,
        _directory: directory,
    }
}

/// Schickt eine Anfrage und liefert Status und JSON-Körper.
pub async fn call(
    router: &Router,
    method: &str,
    uri: &str,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let request = Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json")
        .body(match body {
            Some(value) => Body::from(value.to_string()),
            None => Body::empty(),
        })
        .expect("Anfrage ist gültig");

    let response = router.clone().oneshot(request).await.expect("Antwort");
    let status = response.status();
    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("Körper lesbar")
        .to_bytes();

    let json = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap_or(Value::Null)
    };
    (status, json)
}

/// Schickt eine Anfrage und liefert den rohen Text (für HTML und Secret-Prüfungen).
pub async fn call_raw(router: &Router, method: &str, uri: &str, body: Option<Value>) -> (StatusCode, String) {
    let request = Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json")
        .body(match body {
            Some(value) => Body::from(value.to_string()),
            None => Body::empty(),
        })
        .expect("Anfrage ist gültig");

    let response = router.clone().oneshot(request).await.expect("Antwort");
    let status = response.status();
    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("Körper lesbar")
        .to_bytes();
    (status, String::from_utf8_lossy(&bytes).to_string())
}
