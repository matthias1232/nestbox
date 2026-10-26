//! Tool-Profile: wiederverwendbare Paketlisten für Ubuntu-24.04-Sandboxes.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Ein installierbares Werkzeug aus dem Katalog (heute: Ubuntu/APT).
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ToolDefinition {
    pub name: String,
    pub source: String,
    pub release: String,
}

/// Ein benanntes, versionierbares Profil aus mehreren Werkzeugen.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ToolProfile {
    pub name: String,
    pub base: String,
    pub packages: Vec<String>,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub created_at: String,
}

/// Eingabe zum Anlegen eines Tool-Profils.
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct ToolProfileInput {
    pub name: String,
    pub packages: Vec<String>,
    #[serde(default)]
    pub description: String,
}
