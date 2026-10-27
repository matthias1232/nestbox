//! Base operating systems, agent packs, and the tool catalog.
//!
//! All three are catalog data (ADR 0013): a new agent is a row, not a `match`
//! arm. The types here are what the service reads from the tables and what the
//! builder composes a recipe from.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Which package manager a base image uses; decides which tool name applies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum PackageManager {
    Apt,
    Dnf,
}

impl PackageManager {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Apt => "apt",
            Self::Dnf => "dnf",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "apt" => Self::Apt,
            "dnf" => Self::Dnf,
            _ => return None,
        })
    }
}

/// A base operating system a recipe can start from.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Baseline {
    pub key: String,
    pub name: String,
    pub recommended: bool,
    pub base_image: String,
    pub package_manager: PackageManager,
    pub default_tools: Vec<String>,
    pub openshell_ready: bool,
    pub notes: Option<String>,
}

/// How an agent pack gets into an image.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum InstallKind {
    Script,
    Git,
    Npm,
    Pip,
    Binary,
    /// The pack is its own prebuilt image.
    Image,
    None,
}

impl InstallKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Script => "script",
            Self::Git => "git",
            Self::Npm => "npm",
            Self::Pip => "pip",
            Self::Binary => "binary",
            Self::Image => "image",
            Self::None => "none",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "script" => Self::Script,
            "git" => Self::Git,
            "npm" => Self::Npm,
            "pip" => Self::Pip,
            "binary" => Self::Binary,
            "image" => Self::Image,
            "none" => Self::None,
            _ => return None,
        })
    }
}

/// An agent that can be put into a sandbox.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AgentPack {
    pub key: String,
    pub name: String,
    pub vendor: Option<String>,
    pub description: Option<String>,
    /// Baseline keys this pack supports; empty means all.
    pub base_os: Vec<String>,
    pub install_kind: InstallKind,
    pub install_ref: Option<String>,
    /// Paths under the home that must survive a rebuild.
    pub home_paths: Vec<String>,
    pub requires_gpu: bool,
    pub needs_inference: bool,
    pub default_image: String,
    pub default_command: Vec<String>,
    pub ports: Vec<i32>,
    pub env_slots: Vec<String>,
    pub enabled: bool,
}

impl AgentPack {
    /// Whether this pack can be built on the given baseline.
    pub fn supports(&self, baseline_key: &str) -> bool {
        self.base_os.is_empty() || self.base_os.iter().any(|k| k == baseline_key)
    }
}

/// A tool from the catalog, with its name per package manager.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Tool {
    pub key: String,
    pub name: String,
    pub group: String,
    pub description: Option<String>,
    pub apt_name: Option<String>,
    pub rpm_name: Option<String>,
}

impl Tool {
    /// The package name for this manager, if the tool is packaged for it.
    pub fn package_for(&self, manager: PackageManager) -> Option<&str> {
        match manager {
            PackageManager::Apt => self.apt_name.as_deref(),
            PackageManager::Dnf => self.rpm_name.as_deref(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_base_os_list_means_every_baseline() {
        let pack = AgentPack {
            key: "x".into(),
            name: "X".into(),
            vendor: None,
            description: None,
            base_os: vec![],
            install_kind: InstallKind::None,
            install_ref: None,
            home_paths: vec![],
            requires_gpu: false,
            needs_inference: false,
            default_image: "img".into(),
            default_command: vec![],
            ports: vec![],
            env_slots: vec![],
            enabled: true,
        };
        assert!(pack.supports("rocky-9"));
        let mut limited = pack;
        limited.base_os = vec!["ubuntu-24.04".into()];
        assert!(limited.supports("ubuntu-24.04"));
        assert!(!limited.supports("rocky-9"));
    }

    #[test]
    fn a_tool_resolves_to_the_managers_name() {
        let tool = Tool {
            key: "vim".into(),
            name: "Vim".into(),
            group: "editor".into(),
            description: None,
            apt_name: Some("vim".into()),
            rpm_name: Some("vim-enhanced".into()),
        };
        assert_eq!(tool.package_for(PackageManager::Apt), Some("vim"));
        assert_eq!(tool.package_for(PackageManager::Dnf), Some("vim-enhanced"));
    }
}
