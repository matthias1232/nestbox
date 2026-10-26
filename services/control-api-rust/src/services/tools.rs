//! Tool-Profile und der Werkzeugkatalog.

use chrono::Utc;

use crate::domain::tool_profile::{ToolDefinition, ToolProfile, ToolProfileInput};
use crate::error::ApiError;
use crate::state::AppState;

/// Werkzeuge, die der Katalog heute kennt (Ubuntu 24.04, APT).
const CATALOG: &[&str] = &[
    "git",
    "nano",
    "vim",
    "curl",
    "wget",
    "jq",
    "less",
    "procps",
    "iputils-ping",
    "dnsutils",
    "ripgrep",
    "ca-certificates",
    "build-essential",
    "python3",
    "python3-venv",
    "nodejs",
    "npm",
    "openssh-client",
];

/// Alle gespeicherten Profile.
pub async fn list(state: &AppState) -> Vec<ToolProfile> {
    state.data.read().await.tool_profiles.values().cloned().collect()
}

/// Legt ein Profil an.
pub async fn create(state: &AppState, input: ToolProfileInput) -> Result<ToolProfile, ApiError> {
    if input.packages.is_empty() {
        return Err(ApiError::Invalid(
            "Ein Tool-Profil braucht mindestens ein Paket.".to_string(),
        ));
    }
    {
        let data = state.data.read().await;
        if data.tool_profiles.contains_key(&input.name) {
            return Err(ApiError::Conflict {
                resource: "tool_profile",
                name: input.name,
            });
        }
    }

    let profile = ToolProfile {
        name: input.name.clone(),
        base: "ubuntu:24.04".to_string(),
        packages: input.packages,
        description: input.description,
        created_at: Utc::now().to_rfc3339(),
    };

    state
        .data
        .write()
        .await
        .tool_profiles
        .insert(input.name.clone(), profile.clone());

    state
        .audit(
            "tool_profile.created",
            &profile.name,
            serde_json::json!({ "packages": profile.packages }),
        )
        .await;

    Ok(profile)
}

/// Sucht Werkzeuge im Katalog.
pub fn catalog(query: &str) -> Vec<ToolDefinition> {
    let needle = query.trim().to_lowercase();
    CATALOG
        .iter()
        .filter(|tool| needle.is_empty() || tool.contains(&needle))
        .map(|tool| ToolDefinition {
            name: tool.to_string(),
            source: "apt".to_string(),
            release: "24.04".to_string(),
        })
        .collect()
}
