//! Referenzumgebung: die drei Sandboxes der Phase 1 in einem Schritt.
//!
//! Bewusst dünn: die Funktion nutzt ausschließlich die öffentlichen
//! Service-Funktionen, damit sich Verhalten und Fehler nicht unterscheiden.

use serde::Serialize;
use utoipa::ToSchema;

use crate::domain::sandbox::SandboxInput;
use crate::error::ApiError;
use crate::services::{sandboxes, topology};
use crate::state::AppState;

/// Ein Schritt im Referenz-Deployment.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct DeploymentStep {
    pub name: String,
    pub status: String,
    #[serde(default)]
    pub id: String,
}

/// Ergebnis des Referenz-Deployments.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct DeploymentReport {
    pub status: String,
    pub steps: Vec<DeploymentStep>,
    #[serde(default)]
    pub error: Option<String>,
}

/// Legt `dev-01`, `gateway-01` und `hermes-01` an und verbindet Hermes mit dem Gateway.
///
/// Idempotent: bereits vorhandene Sandboxes werden übersprungen.
pub async fn deploy(state: &AppState) -> DeploymentReport {
    let specs = [
        ("dev-01", "ubuntu-dev"),
        ("gateway-01", "openclaw-gateway"),
        ("hermes-01", "hermes-agent"),
    ];

    let mut steps = Vec::new();
    for (name, pack) in specs {
        let exists = state
            .data
            .read()
            .await
            .sandboxes
            .values()
            .any(|item| item.name == name);

        if exists {
            steps.push(DeploymentStep {
                name: name.to_string(),
                status: "existing".to_string(),
                id: String::new(),
            });
            continue;
        }

        let input = SandboxInput {
            name: name.to_string(),
            pack_key: pack.to_string(),
            tool_profile: None,
            provider_id: None,
            memory_limit: "1g".to_string(),
            cpu_limit: 1.0,
            pids_limit: 256,
            environment: Default::default(),
        };

        match sandboxes::create(state, input).await {
            Ok(sandbox) => steps.push(DeploymentStep {
                name: sandbox.name,
                status: sandbox.status,
                id: sandbox.id,
            }),
            Err(error) => {
                state
                    .audit(
                        "reference.deployment.failed",
                        name,
                        serde_json::json!({ "error": error.to_string() }),
                    )
                    .await;
                return DeploymentReport {
                    status: "failed".to_string(),
                    steps,
                    error: Some(error.to_string()),
                };
            }
        }
    }

    // Hermes → Gateway deklarieren, sofern beide existieren.
    let binding_exists = state
        .data
        .read()
        .await
        .bindings
        .contains_key("hermes-to-gateway");

    if !binding_exists {
        let request = crate::domain::binding::BindingInput {
            name: "hermes-to-gateway".to_string(),
            source: "hermes-01".to_string(),
            target: "gateway-01".to_string(),
            protocol: "http".to_string(),
            port: 8080,
            purpose: "Hermes service integration".to_string(),
        };
        // Ein fehlendes Binding darf das Deployment nicht als Ganzes kippen.
        if let Err(ApiError::Invalid(message)) | Err(ApiError::Conflict { name: message, .. }) =
            topology::create(state, request).await
        {
            state
                .audit(
                    "reference.binding.skipped",
                    "hermes-to-gateway",
                    serde_json::json!({ "reason": message }),
                )
                .await;
        }
    }

    state
        .audit(
            "reference.deployment.completed",
            "reference",
            serde_json::json!({ "steps": steps.len() }),
        )
        .await;

    DeploymentReport {
        status: "running".to_string(),
        steps,
        error: None,
    }
}
