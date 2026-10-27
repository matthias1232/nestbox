//! One copy of a catalog model on one node.
//!
//! The catalog describes a model; an install is that model on a machine. The
//! same model may be installed on two nodes. An install reserves memory on its
//! node for as long as it exists, which is how a second model that no longer
//! fits is reported honestly.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use super::model::Model;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum InstallState {
    Installing,
    Installed,
    Updating,
    Uninstalling,
    Failed,
}

impl InstallState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Installing => "installing",
            Self::Installed => "installed",
            Self::Updating => "updating",
            Self::Uninstalling => "uninstalling",
            Self::Failed => "failed",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "installing" => Self::Installing,
            "installed" => Self::Installed,
            "updating" => Self::Updating,
            "uninstalling" => Self::Uninstalling,
            "failed" => Self::Failed,
            _ => return None,
        })
    }

    /// Whether the install still holds its reservation.
    ///
    /// A failed install holds nothing; an uninstalling one still does until the
    /// job finishes, because the server may still be running.
    pub fn reserves(self) -> bool {
        !matches!(self, Self::Failed)
    }
}

/// A model install as stored and returned.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ModelInstall {
    pub id: Uuid,
    pub environment_id: Uuid,
    pub model_key: String,
    pub node_id: Uuid,
    pub state: InstallState,
    pub installed_version: Option<String>,
    pub reserved_ram_bytes: i64,
    pub reserved_vram_bytes: i64,
    pub provider_profile_id: Option<Uuid>,
    pub job_id: Option<Uuid>,
    pub error: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// What an install of this model reserves on a node.
///
/// Recommended where the catalog has it, minimum otherwise. A GPU model
/// reserves VRAM; a CPU model reserves RAM; a hybrid reserves whichever path
/// the node will take, which the caller decides from the node's hardware.
pub fn reservation_for(model: &Model, cpu_path: bool) -> (i64, i64) {
    let ram = model
        .recommended_ram_bytes
        .or(model.min_ram_bytes)
        .unwrap_or(0);
    let vram = model
        .recommended_vram_bytes
        .or(model.min_vram_bytes)
        .unwrap_or(0);
    if cpu_path {
        (ram, 0)
    } else {
        // A GPU server still needs host RAM to stage the weights; the catalog's
        // RAM figure covers that where present.
        (ram, vram)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::model::{ModelRuntime, ModelState};

    const GIB: i64 = 1024 * 1024 * 1024;

    fn model(min_vram: Option<i64>, rec_vram: Option<i64>, min_ram: Option<i64>, rec_ram: Option<i64>) -> Model {
        Model {
            id: Uuid::nil(),
            key: "m".into(),
            name: "M".into(),
            family: "f".into(),
            parameters: "x".into(),
            context_length: None,
            quantization: None,
            weights_bytes: None,
            min_vram_bytes: min_vram,
            recommended_vram_bytes: rec_vram,
            min_gpu_count: 1,
            min_ram_bytes: min_ram,
            recommended_ram_bytes: rec_ram,
            min_cpu_cores: None,
            recommended_cpu_cores: None,
            runtime: ModelRuntime::Hybrid,
            arch: None,
            artifact_format: None,
            publisher: None,
            license: None,
            description: None,
            unified_memory_only: false,
            is_cpu_only: false,
            source_uri: None,
            version: "1".into(),
            installed_version: None,
            state: ModelState::NotInstalled,
            installed_at: None,
            is_remote: false,
            provider_key: None,
        }
    }

    #[test]
    fn the_recommended_figure_is_reserved_when_present() {
        let m = model(Some(8 * GIB), Some(12 * GIB), Some(8 * GIB), Some(16 * GIB));
        assert_eq!(reservation_for(&m, false), (16 * GIB, 12 * GIB));
        assert_eq!(reservation_for(&m, true), (16 * GIB, 0));
    }

    #[test]
    fn the_minimum_is_the_fallback() {
        let m = model(Some(8 * GIB), None, Some(8 * GIB), None);
        assert_eq!(reservation_for(&m, false), (8 * GIB, 8 * GIB));
    }

    #[test]
    fn a_failed_install_holds_nothing() {
        assert!(!InstallState::Failed.reserves());
        assert!(InstallState::Uninstalling.reserves());
    }
}
