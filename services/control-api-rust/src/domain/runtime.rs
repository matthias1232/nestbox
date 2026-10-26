//! Ist-Zustand einer Sandbox, wie ihn der Runtime-Adapter meldet.
//!
//! Diese Typen sind bewusst getrennt vom Soll-Zustand (`Sandbox`): der
//! Controller speichert Wünsche, der Adapter liefert Fakten.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Netzwerkschnittstelle einer Sandbox.
#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
pub struct NetworkInterface {
    pub name: String,
    #[serde(default)]
    pub ip: String,
    #[serde(default)]
    pub gateway: String,
}

/// Ressourcenlimits, die der Runtime tatsächlich gesetzt hat.
#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
pub struct RuntimeResources {
    #[serde(default)]
    pub memory_limit: u64,
    #[serde(default)]
    pub nano_cpus: u64,
    #[serde(default)]
    pub pids_limit: i64,
}

/// Live-Fakten zu einer Sandbox.
#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
pub struct RuntimeFacts {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub container_name: String,
    pub status: String,
    #[serde(default)]
    pub image: String,
    #[serde(default)]
    pub pack_key: String,
    #[serde(default)]
    pub tool_profile: String,
    #[serde(default)]
    pub ip: String,
    #[serde(default)]
    pub gateway: String,
    #[serde(default)]
    pub network: Vec<NetworkInterface>,
    #[serde(default)]
    pub uptime_seconds: u64,
    #[serde(default)]
    pub health: String,
    #[serde(default)]
    pub started_at: Option<String>,
    #[serde(default)]
    pub exit_code: Option<i64>,
    #[serde(default)]
    pub resources: RuntimeResources,
}

/// Wunsch zum Erzeugen einer Sandbox – die Sprache des Adapters.
#[derive(Debug, Clone)]
pub struct SandboxSpec {
    pub name: String,
    pub image: String,
    pub command: Vec<String>,
    pub environment: Vec<(String, String)>,
    pub pack_key: String,
    pub tool_profile: String,
    pub memory_limit: String,
    pub cpu_limit: f64,
    pub pids_limit: i64,
}
