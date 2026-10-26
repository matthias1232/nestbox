//! Soll-Zustand einer Sandbox und der Wunsch, sie zu erzeugen.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::domain::runtime::RuntimeFacts;

fn default_pack() -> String {
    "ubuntu-dev".to_string()
}

fn default_memory() -> String {
    "1g".to_string()
}

fn default_cpu() -> f64 {
    1.0
}

fn default_pids() -> i64 {
    256
}

/// Eine vom Controller verwaltete Sandbox.
///
/// `runtime` ist der zuletzt bekannte Ist-Zustand. Er wird bei jedem
/// Listenaufruf neu vom Adapter geholt.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Sandbox {
    pub id: String,
    pub name: String,
    pub pack_key: String,
    pub tool_profile: String,
    #[serde(default)]
    pub provider_id: Option<String>,
    pub status: String,
    pub created_at: String,
    #[serde(default)]
    pub runtime: Option<RuntimeFacts>,
}

/// Erstellungswunsch für eine Sandbox.
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct SandboxInput {
    /// DNS-tauglicher Name; wird zugleich der Container-Hostname.
    pub name: String,
    /// Agent-Pack aus dem Katalog.
    #[serde(default = "default_pack")]
    pub pack_key: String,
    /// Tool-Profil; leer bedeutet: Default des Packs.
    #[serde(default)]
    pub tool_profile: Option<String>,
    /// Provider-Profil, dessen Zugangsdaten als Umgebung übergeben werden.
    #[serde(default)]
    pub provider_id: Option<String>,
    #[serde(default = "default_memory")]
    pub memory_limit: String,
    #[serde(default = "default_cpu")]
    pub cpu_limit: f64,
    #[serde(default = "default_pids")]
    pub pids_limit: i64,
    /// Zusätzliche Umgebungswerte (keine Secrets in der Oberfläche speichern).
    #[serde(default)]
    pub environment: std::collections::BTreeMap<String, String>,
}

/// Laufzeitaktion auf einer Sandbox.
#[derive(Debug, Clone, Copy, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum LifecycleAction {
    Start,
    Stop,
    Restart,
    Remove,
}

impl LifecycleAction {
    /// Name der Aktion, wie sie im Audit-Log und in der API erscheint.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Start => "start",
            Self::Stop => "stop",
            Self::Restart => "restart",
            Self::Remove => "remove",
        }
    }
}

/// Anfragekörper für eine Lifecycle-Aktion.
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct LifecycleRequest {
    pub action: LifecycleAction,
}

/// Antwort der Log-Abfrage.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct SandboxLogs {
    pub name: String,
    pub tail: u32,
    pub logs: String,
}
