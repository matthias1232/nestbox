//! Provider-Profile: OpenAI-kompatible Endpunkte wie OmniRoute.
//!
//! Wichtig: Das Secret ist **nicht** Teil von [`ProviderProfile`]. Es liegt im
//! [`crate::state::SecretStore`] und verlässt den Prozess nur als Umgebungswert
//! einer Sandbox.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

fn default_kind() -> String {
    "openai-compatible".to_string()
}

/// Öffentliche Sicht auf ein Provider-Profil.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ProviderProfile {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub base_url: String,
    pub model: String,
    /// Nur die Information *ob* ein Key hinterlegt ist – nie der Key selbst.
    pub api_key_configured: bool,
    pub created_at: String,
}

/// Eingabe zum Anlegen eines Provider-Profils.
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct ProviderInput {
    pub name: String,
    #[serde(default = "default_kind")]
    pub kind: String,
    /// Basis-URL des Endpunkts, z. B. `http://omniroute:20128/v1`.
    pub base_url: String,
    /// Modellname, wie ihn der Endpunkt erwartet.
    pub model: String,
    /// API-Key. Wird einmal gespeichert und nie zurückgegeben.
    #[serde(default)]
    pub api_key: Option<String>,
}
