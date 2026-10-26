//! Persistenz, Migration älterer Dateien und Adoption laufender Container.

mod support;

use std::sync::Arc;

use nestbox_control_api::config::Config;
use nestbox_control_api::domain::runtime::SandboxSpec;
use nestbox_control_api::runtime::RuntimeAdapter;
use nestbox_control_api::runtime::fake::FakeRuntime;
use nestbox_control_api::{build, services};
use serde_json::json;
use tempfile::TempDir;

fn config_for(directory: &TempDir) -> Config {
    Config {
        state_path: directory.path().join("state.json"),
        secrets_path: directory.path().join("secrets.json"),
        runner_url: "http://unused".to_string(),
        runtime_network: "nestbox-test".to_string(),
        listen_addr: "127.0.0.1:0".to_string(),
    }
}

#[tokio::test]
async fn migrates_legacy_state_and_keeps_compatible_records() {
    let directory = TempDir::new().expect("Temp");
    let config = config_for(&directory);

    // Altes Format: ein Datensatz mit `"runtime": "custom"` (String) und ein
    // Datensatz ohne pack_key. Beide sind nicht mehr gültig.
    let legacy = json!({
        "sandboxes": {
            "legacy-1": {"id": "legacy-1", "name": "legacy-1", "status": "stopped", "runtime": "custom"},
            "legacy-2": {"id": "legacy-2", "name": "legacy-2", "status": "running", "tool_profile": "x"},
            "hermes-01": {
                "id": "hermes-01", "name": "hermes-01", "pack_key": "hermes-agent",
                "tool_profile": "development-default", "status": "running",
                "created_at": "2026-10-25T22:44:38Z", "runtime": {"id": "c1", "name": "hermes-01", "status": "running"}
            }
        },
        "events": [],
        "providers": {},
        "tool_profiles": {},
        "bindings": {}
    });
    tokio::fs::write(&config.state_path, legacy.to_string())
        .await
        .expect("schreiben");

    let runtime = Arc::new(FakeRuntime::new());
    let state = build(config, runtime).await.expect("State lädt");

    let data = state.data.read().await;
    assert_eq!(
        data.sandboxes.len(),
        1,
        "nur der kompatible Datensatz bleibt"
    );
    assert!(data.sandboxes.values().any(|item| item.name == "hermes-01"));
    assert!(data.tool_profiles.contains_key("development-default"));
}

#[tokio::test]
async fn adopts_sandboxes_that_only_the_runtime_knows() {
    let directory = TempDir::new().expect("Temp");
    let config = config_for(&directory);
    let runtime = Arc::new(FakeRuntime::new());

    // Simuliert einen laufenden Container, der nach einem Upgrade nicht mehr
    // im State steht.
    runtime
        .create(SandboxSpec {
            name: "dev-01".to_string(),
            image: "nestbox-ubuntu-dev:local".to_string(),
            command: vec!["sleep".to_string(), "infinity".to_string()],
            environment: Vec::new(),
            pack_key: "ubuntu-dev".to_string(),
            tool_profile: "development-default".to_string(),
            memory_limit: "1g".to_string(),
            cpu_limit: 1.0,
            pids_limit: 256,
        })
        .await
        .expect("Container");

    let state = build(config, runtime).await.expect("State lädt");
    let items = services::sandboxes::list(&state).await;

    assert_eq!(items.len(), 1);
    assert_eq!(items[0].name, "dev-01");
    assert_eq!(items[0].status, "running");
}
