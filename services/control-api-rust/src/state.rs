//! Persistenter Zustand der Control Plane.
//!
//! Zwei getrennte Dateien:
//!
//! * `state.json` – Soll-Zustand (Sandboxes, Provider-Metadaten, Profile, Bindings, Events)
//! * `secrets.json` – nur API-Keys, Dateirechte 0600
//!
//! Beide werden atomar geschrieben (Temp-Datei + `rename`), damit ein Abbruch
//! mitten im Schreiben keine halbe Datei hinterlässt.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

use crate::audit::AuditEvent;
use crate::config::Config;
use crate::domain::{Binding, ProviderProfile, Sandbox, ToolProfile};
use crate::error::ApiError;
use crate::runtime::RuntimeAdapter;

/// Maximal gespeicherte Audit-Events.
const AUDIT_LIMIT: usize = 1000;

/// Alles, was die Control Plane über die Welt weiß.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Data {
    #[serde(default)]
    pub sandboxes: BTreeMap<String, Sandbox>,
    #[serde(default)]
    pub providers: BTreeMap<String, ProviderProfile>,
    #[serde(default)]
    pub tool_profiles: BTreeMap<String, ToolProfile>,
    #[serde(default)]
    pub bindings: BTreeMap<String, Binding>,
    #[serde(default)]
    pub events: Vec<AuditEvent>,
}

impl Data {
    /// Zustand mit den mitgelieferten Defaults (Standard-Tool-Profil).
    pub fn seeded() -> Self {
        let mut data = Self::default();
        data.tool_profiles.insert(
            "development-default".to_string(),
            ToolProfile {
                name: "development-default".to_string(),
                base: "ubuntu:24.04".to_string(),
                packages: [
                    "git",
                    "nano",
                    "curl",
                    "ca-certificates",
                    "jq",
                    "less",
                    "procps",
                    "iputils-ping",
                    "dnsutils",
                    "ripgrep",
                    "vim",
                ]
                .iter()
                .map(|tool| tool.to_string())
                .collect(),
                description: "Standardwerkzeuge für Ubuntu-24.04-Sandboxes.".to_string(),
                created_at: chrono::Utc::now().to_rfc3339(),
            },
        );
        data
    }
}

/// Secrets, getrennt vom Soll-Zustand.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Secrets {
    /// Provider-Name → API-Key.
    #[serde(default)]
    pub provider_api_keys: BTreeMap<String, String>,
}

/// Gemeinsamer Zustand aller Handler.
#[derive(Clone)]
pub struct AppState {
    pub config: Config,
    pub data: Arc<RwLock<Data>>,
    pub secrets: Arc<RwLock<Secrets>>,
    pub runtime: Arc<dyn RuntimeAdapter>,
}

impl AppState {
    /// Lädt State und Secrets, legt Defaults an und schreibt sie zurück.
    pub async fn load(config: Config, runtime: Arc<dyn RuntimeAdapter>) -> Result<Self, ApiError> {
        let data = read_json::<Data>(&config.state_path)
            .await
            .unwrap_or_else(Data::seeded);
        let secrets = read_json::<Secrets>(&config.secrets_path)
            .await
            .unwrap_or_default();

        let state = Self {
            config,
            data: Arc::new(RwLock::new(data)),
            secrets: Arc::new(RwLock::new(secrets)),
            runtime,
        };
        state.save().await?;
        Ok(state)
    }

    /// Schreibt State und Secrets atomar.
    pub async fn save(&self) -> Result<(), ApiError> {
        let data = self.data.read().await.clone();
        write_json(&self.config.state_path, &data, false).await?;
        let secrets = self.secrets.read().await.clone();
        write_json(&self.config.secrets_path, &secrets, true).await?;
        Ok(())
    }

    /// Hängt ein Audit-Event an und persistiert.
    pub async fn audit(&self, kind: &str, subject: &str, detail: serde_json::Value) {
        {
            let mut data = self.data.write().await;
            data.events.push(AuditEvent::new(kind, subject, detail));
            let overflow = data.events.len().saturating_sub(AUDIT_LIMIT);
            if overflow > 0 {
                data.events.drain(0..overflow);
            }
        }
        if let Err(error) = self.save().await {
            tracing::error!("Audit-Event konnte nicht gespeichert werden: {error}");
        }
    }
}

async fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Option<T> {
    let bytes = tokio::fs::read(path).await.ok()?;
    match serde_json::from_slice(&bytes) {
        Ok(value) => Some(value),
        Err(error) => {
            tracing::warn!("{} ist unlesbar und wird ignoriert: {error}", path.display());
            None
        }
    }
}

async fn write_json<T: Serialize>(path: &Path, value: &T, private: bool) -> Result<(), ApiError> {
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let raw = serde_json::to_vec_pretty(value)?;
    let temporary = path.with_extension("tmp");
    tokio::fs::write(&temporary, &raw).await?;

    #[cfg(unix)]
    if private {
        use std::os::unix::fs::PermissionsExt;
        tokio::fs::set_permissions(&temporary, std::fs::Permissions::from_mode(0o600)).await?;
    }

    tokio::fs::rename(&temporary, path).await?;
    Ok(())
}
