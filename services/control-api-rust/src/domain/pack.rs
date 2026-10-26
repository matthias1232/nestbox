//! Agent-Packs: deklarative Definition eines Sandbox-Typs.
//!
//! Ein Pack ist reine Konfiguration. Welches Image, welcher Startbefehl und
//! welches Tool-Profil verwendet werden, steht hier – nicht im Code der API.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Ein installierbarer Agent- oder Runtime-Typ.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Pack {
    pub key: String,
    pub name: String,
    pub image: String,
    pub command: Vec<String>,
    pub tool_profile: String,
    pub description: String,
}

/// Der mitgelieferte Katalog der Phase 1.
pub fn catalog() -> Vec<Pack> {
    vec![
        Pack {
            key: "ubuntu-dev".to_string(),
            name: "Ubuntu 24.04 development".to_string(),
            image: "nestbox-ubuntu-dev:local".to_string(),
            command: vec!["sleep".to_string(), "infinity".to_string()],
            tool_profile: "development-default".to_string(),
            description: "Leere Ubuntu-24.04-Entwicklungssandbox mit Standardwerkzeugen."
                .to_string(),
        },
        Pack {
            key: "openclaw-gateway".to_string(),
            name: "OpenClaw Gateway".to_string(),
            image: "nestbox-openclaw:local".to_string(),
            command: vec![
                "openclaw".to_string(),
                "gateway".to_string(),
                "run".to_string(),
                "--dev".to_string(),
                "--allow-unconfigured".to_string(),
                "--bind".to_string(),
                "lan".to_string(),
                "--port".to_string(),
                "8080".to_string(),
                "--auth".to_string(),
                "no".to_string(),
            ],
            tool_profile: "development-default".to_string(),
            description: "Dedizierte OpenClaw-Gateway-Sandbox.".to_string(),
        },
        Pack {
            key: "hermes-agent".to_string(),
            name: "Hermes Agent".to_string(),
            image: "nestbox-hermes:local".to_string(),
            command: vec!["sleep".to_string(), "infinity".to_string()],
            tool_profile: "development-default".to_string(),
            description: "Offizieller Nous-Hermes-Agent auf Ubuntu 24.04.".to_string(),
        },
    ]
}

/// Sucht ein Pack über seinen Key.
pub fn find(key: &str) -> Option<Pack> {
    catalog().into_iter().find(|pack| pack.key == key)
}
