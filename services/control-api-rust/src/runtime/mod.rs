//! Die Grenze zwischen Controller und Ausführung.
//!
//! Der Controller kennt nur dieses Trait. Welche Technik darunter liegt –
//! Docker über den Sandbox-Runner, später NVIDIA OpenShell – entscheidet allein
//! die Implementierung. Ein Wechsel betrifft genau eine Datei.

use async_trait::async_trait;
use thiserror::Error;

use crate::domain::runtime::{
    BuildRequest, BuildResult, ExecResult, HardwareReport, ModelDownload, ModelDownloadResult,
    ModelServe, ModelServeResult, RuntimeFacts, SandboxSpec,
};

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

    /// Run one command inside a running sandbox and return its output.
    ///
    /// The command is an argument vector: the adapter never builds a shell
    /// string, so a name or an argument from a request cannot become syntax.
    async fn exec(
        &self,
        name: &str,
        command: &[String],
        timeout_seconds: u32,
    ) -> Result<ExecResult, RuntimeError>;

    /// What the runner's host has: CPU, memory, GPUs, container GPU support.
    ///
    /// Asked of the runner rather than measured here, because the control
    /// plane may run in a container that sees none of the node's devices.
    async fn hardware(&self) -> Result<HardwareReport, RuntimeError>;

    /// Build an image from a generated Dockerfile.
    async fn build(&self, request: BuildRequest) -> Result<BuildResult, RuntimeError>;

    /// Fetch one model artefact onto the node.
    ///
    /// The download runs on the node, not here: the control plane has neither
    /// the disk nor the bandwidth, and a half-written file must not look like a
    /// finished install on a machine it cannot see.
    async fn download_model(&self, request: ModelDownload) -> Result<ModelDownloadResult, RuntimeError>;

    /// Start the model server container for an install.
    async fn serve_model(&self, request: ModelServe) -> Result<ModelServeResult, RuntimeError>;

    /// Stop and remove a model server, if it exists.
    ///
    /// Idempotent: removing a server that is already gone is not an error, so an
    /// uninstall that follows a crashed install still finishes.
    async fn stop_model(&self, model_key: &str) -> Result<(), RuntimeError>;
}

pub mod fake;
pub mod http_runner;
