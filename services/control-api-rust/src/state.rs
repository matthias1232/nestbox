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
use crate::domain::pack;
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
    /// Ergänzt fehlende Defaults, ohne bestehende Einträge zu überschreiben.
    pub fn seed_defaults(&mut self) {
        // `or_insert_with` schützt ein vom Operator angepasstes Profil.
        self.tool_profiles
            .entry("development-default".to_string())
            .or_insert_with(|| ToolProfile {
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
            });
    }

    /// Zustand mit den mitgelieferten Defaults (Standard-Tool-Profil).
    pub fn seeded() -> Self {
        let mut data = Self::default();
        data.seed_defaults();
        data
    }
}

/// Secrets, getrennt vom Soll-Zustand.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Secrets {
    /// Provider-Name → API-Key.
    #[serde(default)]
    pub provider_api_keys: BTreeMap<String, String>,
    /// Sandbox-Name → generiertes Dienst-Token (z. B. OpenClaw Gateway).
    #[serde(default)]
    pub sandbox_tokens: BTreeMap<String, String>,
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
        let mut data = read_data(&config.state_path).await;
        data.seed_defaults();
        let secrets = read_json::<Secrets>(&config.secrets_path)
            .await
            .unwrap_or_default();

        let state = Self {
            config,
            data: Arc::new(RwLock::new(data)),
            secrets: Arc::new(RwLock::new(secrets)),
            runtime,
        };

        // Container, die der Runtime kennt, aber der State nicht (z. B. nach
        // einem Upgrade), werden übernommen statt verwaist zu bleiben.
        state.adopt_existing().await;
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

    /// Übernimmt Sandboxes, die im Runtime existieren, aber im State fehlen.
    async fn adopt_existing(&self) {
        let Ok(items) = self.runtime.list().await else {
            return;
        };

        let mut data = self.data.write().await;
        for facts in items {
            let known = data.sandboxes.values().any(|item| item.name == facts.name);
            if known || pack::find(&facts.pack_key).is_none() {
                continue;
            }

            tracing::info!("Übernehme vorhandene Sandbox '{}' vom Runtime", facts.name);
            let sandbox = Sandbox {
                id: uuid::Uuid::new_v4().to_string(),
                name: facts.name.clone(),
                pack_key: facts.pack_key.clone(),
                tool_profile: facts.tool_profile.clone(),
                provider_id: None,
                status: facts.status.clone(),
                created_at: facts
                    .started_at
                    .clone()
                    .unwrap_or_else(|| chrono::Utc::now().to_rfc3339()),
                runtime: Some(facts),
            };
            data.sandboxes.insert(sandbox.id.clone(), sandbox);
        }
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

/// Liest den State und verträgt dabei ältere Dateiformate.
async fn read_data(path: &Path) -> Data {
    let Some(mut value) = read_json::<serde_json::Value>(path).await else {
        return Data::seeded();
    };

    migrate(&mut value);

    match serde_json::from_value::<Data>(value) {
        Ok(data) => data,
        Err(error) => {
            tracing::warn!("State passt nicht zum aktuellen Format und wird neu angelegt: {error}");
            Data::seeded()
        }
    }
}

/// Entfernt Einträge, die nicht mehr zum aktuellen Format passen.
///
/// Der Prototyp der Phase 1 schrieb `"runtime": "custom"` als String. Solche
/// Datensätze werden verworfen; Sandboxes, die noch im Runtime laufen, kommen
/// über [`AppState::adopt_existing`] zurück.
fn migrate(value: &mut serde_json::Value) {
    if let Some(sandboxes) = value.get_mut("sandboxes").and_then(|node| node.as_object_mut()) {
        sandboxes.retain(|_, item| {
            item.get("pack_key").is_some_and(|pack| pack.as_str().is_some_and(|name| !name.is_empty()))
                && item.get("name").and_then(|name| name.as_str()).is_some()
        });
        for item in sandboxes.values_mut() {
            let runtime_is_object = item
                .get("runtime")
                .map(serde_json::Value::is_object)
                .unwrap_or(false);
            if !runtime_is_object {
                item["runtime"] = serde_json::Value::Null;
            }
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
