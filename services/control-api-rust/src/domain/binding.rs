//! Service-Bindings: deklarierte Kommunikation zwischen Sandboxes.
//!
//! Ein Binding ist beschreibend, keine Firewall-Regel. Es sagt, wer mit wem
//! über welchen Port spricht, und wird in der Topologie gezeichnet.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

fn default_protocol() -> String {
    "http".to_string()
}

fn default_port() -> u16 {
    8080
}

fn default_purpose() -> String {
    "service communication".to_string()
}

/// Deklarierte Verbindung zwischen zwei Sandboxes.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Binding {
    pub id: String,
    pub name: String,
    pub source: String,
    pub target: String,
    pub protocol: String,
    pub port: u16,
    pub purpose: String,
    pub created_at: String,
}

/// Eingabe zum Anlegen eines Bindings.
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct BindingInput {
    pub name: String,
    pub source: String,
    pub target: String,
    #[serde(default = "default_protocol")]
    pub protocol: String,
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default = "default_purpose")]
    pub purpose: String,
}
