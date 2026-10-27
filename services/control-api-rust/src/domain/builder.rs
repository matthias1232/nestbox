//! The builder: a recipe is the desired state of a sandbox image.
//!
//! A recipe names a baseline, tools, agents, resources, and mounts. Building it
//! produces an immutable image tagged by the content hash of the recipe, so the
//! same recipe always yields the same image and an edit always yields a new
//! one. Persistent data never lives in the image.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

/// Whether agents in a sandbox share one home or each get their own.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum HomeMode {
    /// `/home/nestbox`, shared by every agent in the sandbox.
    Shared,
    /// `/home/agents/<instance>`, one per agent.
    Isolated,
}

impl HomeMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Shared => "shared",
            Self::Isolated => "isolated",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "shared" => Self::Shared,
            "isolated" => Self::Isolated,
            _ => return None,
        })
    }

    /// The path inside the container for an agent instance.
    pub fn home_path(self, instance: &str) -> String {
        match self {
            Self::Shared => "/home/nestbox".to_string(),
            Self::Isolated => format!("/home/agents/{instance}"),
        }
    }
}

/// Whether an agent's home survives the container.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum Persistence {
    /// The home is a persistent root on the node.
    Persistent,
    /// The home lives in the container and dies with it.
    Ephemeral,
}

impl Persistence {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Persistent => "persistent",
            Self::Ephemeral => "ephemeral",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "persistent" => Self::Persistent,
            "ephemeral" => Self::Ephemeral,
            _ => return None,
        })
    }
}

/// One agent inside a recipe.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct AgentSpec {
    pub pack_key: String,
    /// Instance name, unique within the recipe: `hermes`, `claude`.
    pub instance: String,
    pub home_mode: HomeMode,
    pub persistence: Persistence,
    /// Overrides the pack's `home_paths` when non-empty.
    #[serde(default)]
    pub home_paths: Vec<String>,
    #[serde(default)]
    pub port: Option<u16>,
}

/// The limits a recipe asks for.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct ResourceLimits {
    /// Docker-style string, normalised to bytes by the sandbox service.
    #[serde(default = "default_memory")]
    pub memory_limit: String,
    #[serde(default = "default_cpu")]
    pub cpu_limit: f64,
    #[serde(default)]
    pub gpu_count: u32,
    #[serde(default = "default_pids")]
    pub pids_limit: i64,
}

fn default_memory() -> String {
    "1g".into()
}
fn default_cpu() -> f64 {
    1.0
}
fn default_pids() -> i64 {
    256
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            memory_limit: default_memory(),
            cpu_limit: default_cpu(),
            gpu_count: 0,
            pids_limit: default_pids(),
        }
    }
}

/// A project root mounted into the sandbox a recipe produces.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct ProjectMount {
    pub root_id: Uuid,
    pub mount_path: String,
    #[serde(default)]
    pub read_only: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum RecipeState {
    Draft,
    Building,
    Built,
    Failed,
}

impl RecipeState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Building => "building",
            Self::Built => "built",
            Self::Failed => "failed",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "draft" => Self::Draft,
            "building" => Self::Building,
            "built" => Self::Built,
            "failed" => Self::Failed,
            _ => return None,
        })
    }
}

/// A recipe as stored and returned.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Recipe {
    pub id: Uuid,
    pub environment_id: Uuid,
    pub owner_user_id: Option<Uuid>,
    pub name: String,
    pub baseline_key: String,
    pub tool_profile_id: Option<Uuid>,
    pub tools: Vec<String>,
    pub agents: Vec<AgentSpec>,
    pub resources: ResourceLimits,
    pub project_mounts: Vec<ProjectMount>,
    pub node_id: Uuid,
    pub state: RecipeState,
    pub image_ref: Option<String>,
    pub content_hash: Option<String>,
    pub revision: i64,
    pub build_job_id: Option<Uuid>,
    pub build_error: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// A valid recipe name: lowercase slug, so it can become an image tag.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecipeName(String);

impl RecipeName {
    /// Validate a recipe name.
    pub fn new(value: &str) -> Result<Self, String> {
        let value = value.trim();
        if value.is_empty() || value.len() > 63 {
            return Err("A recipe name is 1 to 63 characters.".into());
        }
        if !value
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
            || value.starts_with('-')
        {
            return Err("A recipe name uses lowercase letters, digits, and hyphens.".into());
        }
        Ok(Self(value.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_recipe_name_is_a_slug() {
        assert!(RecipeName::new("dev-box").is_ok());
        assert!(RecipeName::new("Dev Box").is_err());
        assert!(RecipeName::new("-x").is_err());
        assert!(RecipeName::new("").is_err());
    }

    #[test]
    fn home_mode_decides_the_path() {
        assert_eq!(HomeMode::Shared.home_path("hermes"), "/home/nestbox");
        assert_eq!(HomeMode::Isolated.home_path("hermes"), "/home/agents/hermes");
    }

    #[test]
    fn resource_limits_have_sane_defaults() {
        let r = ResourceLimits::default();
        assert_eq!(r.memory_limit, "1g");
        assert_eq!(r.gpu_count, 0);
    }
}
