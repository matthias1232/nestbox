//! Die Grenze zwischen Controller und Ausführung.
//!
//! Der Controller kennt nur dieses Trait. Welche Technik darunter liegt –
//! Docker über den Sandbox-Runner, später NVIDIA OpenShell – entscheidet allein
//! die Implementierung. Ein Wechsel betrifft genau eine Datei.

use async_trait::async_trait;
use thiserror::Error;

use crate::domain::runtime::{RuntimeFacts, SandboxSpec};

/// Fehler aus der Ausführungsschicht.
#[derive(Debug, Error)]
pub enum RuntimeError {
    #[error("Sandbox '{0}' existiert im Runtime nicht")]
    NotFound(String),

    #[error("Runtime lehnt die Anfrage ab: {0}")]
    Rejected(String),

    #[error("Runtime ist nicht erreichbar: {0}")]
    Unavailable(String),
}

/// Was der Controller vom Runtime braucht.
#[async_trait]
pub trait RuntimeAdapter: Send + Sync {
    /// Technischer Name, erscheint in `/v1/runtime`.
    fn name(&self) -> &'static str;

    /// Netzwerk, in dem die Sandboxes liegen.
    fn network(&self) -> &str;

    /// Prüft, ob der Runtime erreichbar ist.
    async fn health(&self) -> Result<(), RuntimeError>;

    /// Erzeugt eine Sandbox und liefert ihren Ist-Zustand.
    async fn create(&self, spec: SandboxSpec) -> Result<RuntimeFacts, RuntimeError>;

    /// Liest den Ist-Zustand einer Sandbox.
    async fn inspect(&self, name: &str) -> Result<RuntimeFacts, RuntimeError>;

    /// Listet alle verwalteten Sandboxes.
    async fn list(&self) -> Result<Vec<RuntimeFacts>, RuntimeError>;

    async fn start(&self, name: &str) -> Result<RuntimeFacts, RuntimeError>;
    async fn stop(&self, name: &str) -> Result<RuntimeFacts, RuntimeError>;
    async fn restart(&self, name: &str) -> Result<RuntimeFacts, RuntimeError>;
    async fn remove(&self, name: &str) -> Result<(), RuntimeError>;

    /// Liest die letzten `tail` Logzeilen.
    async fn logs(&self, name: &str, tail: u32) -> Result<String, RuntimeError>;
}

pub mod fake;
pub mod http_runner;
