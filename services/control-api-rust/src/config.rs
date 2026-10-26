//! Laufzeitkonfiguration des Service.
//!
//! Alle Werte kommen aus Umgebungsvariablen, damit dasselbe Image in
//! Entwicklung und auf der Appliance läuft. Es gibt keine versteckten
//! Defaults für Zugangsdaten.

use std::path::PathBuf;

/// Konfiguration, die beim Start einmal gelesen wird.
#[derive(Debug, Clone)]
pub struct Config {
    /// Ablageort der Zustandsdatei (Sandboxes, Provider-Metadaten, Events).
    pub state_path: PathBuf,
    /// Ablageort der Secrets – getrennt vom State, eigene Dateirechte.
    pub secrets_path: PathBuf,
    /// Basis-URL des Runtime-Adapters (heute: Docker-Sandbox-Runner).
    pub runner_url: String,
    /// Name des Runtime-Netzwerks, in dem alle Sandboxes liegen.
    pub runtime_network: String,
    /// Port, auf dem die API lauscht.
    pub listen_addr: String,
}

impl Config {
    /// Liest die Konfiguration aus der Umgebung.
    pub fn from_env() -> Self {
        let state_path = std::env::var("NESTBOX_STATE_PATH")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("/data/state.json"));

        // Secrets liegen bewusst neben dem State, aber in einer eigenen Datei.
        let secrets_path = std::env::var("NESTBOX_SECRETS_PATH")
            .map(PathBuf::from)
            .unwrap_or_else(|_| state_path.with_file_name("secrets.json"));

        Self {
            state_path,
            secrets_path,
            runner_url: std::env::var("NESTBOX_RUNNER_URL")
                .unwrap_or_else(|_| "http://sandbox-runner:9000".to_string()),
            runtime_network: std::env::var("NESTBOX_RUNTIME_NETWORK")
                .unwrap_or_else(|_| "nestbox-runtime".to_string()),
            listen_addr: std::env::var("NESTBOX_LISTEN_ADDR")
                .unwrap_or_else(|_| "0.0.0.0:8000".to_string()),
        }
    }
}
