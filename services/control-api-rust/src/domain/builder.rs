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


// ---------------------------------------------------------------------------
// The supervisor script
// ---------------------------------------------------------------------------

/// One agent as the entrypoint generator needs it.
///
/// Owned rather than borrowed so a caller can collect launches while it walks
/// the packs; the generator itself is a pure function and does not care where
/// the values came from.
pub struct AgentLaunch {
    pub instance: String,
    pub home_mode: HomeMode,
    pub command: Vec<String>,
    pub env_slots: Vec<String>,
    pub port: Option<u16>,
}

/// Render the container command that starts every agent in a sandbox.
///
/// One supervisor script, not one process per container: each agent runs in a
/// subshell that changes into its home, exports the pack's environment slots,
/// and is `exec`'d so the PID the supervisor tracks is the agent itself. The
/// supervisor then waits for the first agent to stop and exits, because a
/// sandbox whose agents have died must not look healthy.
///
/// bash is used rather than `/bin/sh` for `wait -n`, which is how "any child"
/// is expressed without polling. Every sandbox image starts from Ubuntu or
/// Debian and ships bash 5; the pack's own command is unaffected by the choice.
///
/// A pack without a command (it is its own image with its own entrypoint) gets
/// `sleep infinity`, so the supervisor still owns one PID per agent and the
/// health of the sandbox stays tied to the agents it claims to run.
pub fn entrypoint_command(agents: &[AgentLaunch]) -> Vec<String> {
    let mut script = String::from(
        "set -u\n\
         # Nestbox sandbox supervisor: one agent per home, and the sandbox\n\
         # lives exactly as long as all of them do.\n",
    );

    if agents.is_empty() {
        script.push_str(
            "# No agent is configured, so the container stays idle until it is\n\
             # asked to do something with an exec.\n\
             exec sleep infinity\n",
        );
        return vec!["/bin/bash".to_string(), "-c".to_string(), script];
    }

    for agent in agents {
        let home = agent.home_mode.home_path(&agent.instance);
        let quoted = quote(&home);
        script.push_str(&format!(
            "\n# agent {} in {} ({})\nmkdir -p {quoted}\n(\n  cd {quoted} || exit 1\n  export HOME={quoted}\n",
            agent.instance,
            home,
            if agent.home_mode == HomeMode::Shared {
                "shared home"
            } else {
                "isolated home"
            },
        ));
        for slot in agent.env_slots.iter().filter(|slot| is_env_name(slot)) {
            // Exported even when unset, so the agent reads an empty value
            // instead of inheriting whatever the image happens to define.
            script.push_str(&format!("  export {slot}=\"${{{slot}:-}}\"\n"));
        }
        if let Some(port) = agent.port {
            script.push_str(&format!("  export NESTBOX_AGENT_PORT={port}\n"));
        }
        let command = if agent.command.is_empty() {
            vec!["sleep".to_string(), "infinity".to_string()]
        } else {
            agent.command.to_vec()
        };
        let line: Vec<String> = command.iter().map(|part| quote(part)).collect();
        script.push_str(&format!("  exec {}\n) &\n", line.join(" ")));
    }

    script.push_str(
        "\n# The first agent to stop ends the sandbox: a sandbox with a dead\n\
         # agent is not healthy, and restarting is the operator's decision.\n\
         wait -n\n\
         status=$?\n\
         echo \"nestbox: an agent exited with status $status; stopping the sandbox\" >&2\n\
         exit 1\n",
    );
    vec!["/bin/bash".to_string(), "-c".to_string(), script]
}

/// Whether an environment slot name may appear in a generated script.
///
/// Slot names come from the catalog, which is data. A name that is not a shell
/// identifier would turn into a syntax error — or into shell code — in a
/// generated script, so it is dropped instead.
fn is_env_name(value: &str) -> bool {
    let mut bytes = value.bytes();
    match bytes.next() {
        Some(first) if first.is_ascii_uppercase() || first == b'_' => {}
        _ => return false,
    }
    bytes.all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'_')
}

/// Quote one argument for the supervisor script.
///
/// Only the characters that are safe unquoted stay unquoted; everything else is
/// wrapped in single quotes, which is the one form the shell does not interpret.
fn quote(value: &str) -> String {
    let safe = !value.is_empty()
        && value.chars().all(|letter| {
            letter.is_ascii_alphanumeric() || matches!(letter, '_' | '-' | '.' | '/' | ':' | '@' | '=')
        });
    if safe {
        value.to_string()
    } else {
        format!("'{}'", value.replace('\'', "'\\''"))
    }
}

#[cfg(test)]
mod supervisor_tests {
    use super::*;

    fn launch(
        instance: &str,
        home_mode: HomeMode,
        command: &[String],
        env_slots: &[String],
        port: Option<u16>,
    ) -> AgentLaunch {
        AgentLaunch {
            instance: instance.to_string(),
            home_mode,
            command: command.to_vec(),
            env_slots: env_slots.to_vec(),
            port,
        }
    }

    #[test]
    fn two_agents_with_different_homes_get_one_supervisor() {
        let hermes_command = vec!["sleep".to_string(), "infinity".to_string()];
        let hermes_env = vec!["OPENAI_BASE_URL".to_string(), "OPENAI_API_KEY".to_string()];
        let claude_command = vec!["claude".to_string(), "--dangerously-skip-permissions".to_string()];
        let claude_env = vec!["ANTHROPIC_API_KEY".to_string()];

        let agents = vec![
            launch("hermes", HomeMode::Shared, &hermes_command, &hermes_env, None),
            launch("claude", HomeMode::Isolated, &claude_command, &claude_env, Some(3000)),
        ];
        let command = entrypoint_command(&agents);

        assert_eq!(command.len(), 3);
        assert_eq!(command[0], "/bin/bash");
        assert_eq!(command[1], "-c");
        let script = &command[2];

        // One supervisor, one script, one `wait`.
        assert_eq!(script.matches("wait -n").count(), 1);
        assert!(script.ends_with("exit 1\n"));

        // The shared agent lands in /home/nestbox, the isolated one in its own.
        assert!(script.contains("mkdir -p /home/nestbox"));
        assert!(script.contains("cd /home/nestbox || exit 1"));
        assert!(script.contains("mkdir -p /home/agents/claude"));
        assert!(script.contains("cd /home/agents/claude || exit 1"));
        assert!(!script.contains("/home/agents/hermes"));

        // Each agent is started, and its own slots are exported for it only.
        assert!(script.contains("exec sleep infinity"));
        assert!(script.contains("exec claude --dangerously-skip-permissions"));
        assert!(script.contains("export OPENAI_BASE_URL=\"${OPENAI_BASE_URL:-}\""));
        assert!(script.contains("export OPENAI_API_KEY=\"${OPENAI_API_KEY:-}\""));
        assert!(script.contains("export ANTHROPIC_API_KEY=\"${ANTHROPIC_API_KEY:-}\""));
        assert!(script.contains("export NESTBOX_AGENT_PORT=3000"));

        // One background subshell per agent.
        assert_eq!(script.matches(") &").count(), 2);

        // The claude block must not carry hermes's slots.
        let claude_block = script.split("# agent claude").nth(1).unwrap_or_default();
        assert!(!claude_block.contains("OPENAI_API_KEY"));
    }

    #[test]
    fn an_agent_without_a_command_keeps_its_slot() {
        let empty: Vec<String> = Vec::new();
        let slots: Vec<String> = Vec::new();
        let command = entrypoint_command(&[launch(
            "openclaw",
            HomeMode::Isolated,
            &empty,
            &slots,
            None,
        )]);
        assert!(command[2].contains("exec sleep infinity"));
        assert_eq!(command[2].matches(") &").count(), 1);
    }

    #[test]
    fn a_sandbox_without_agents_stays_idle() {
        let command = entrypoint_command(&[]);
        assert_eq!(command[0], "/bin/bash");
        assert!(command[2].contains("exec sleep infinity"));
        assert!(!command[2].contains("wait -n"));
    }

    #[test]
    fn a_hostile_command_is_quoted() {
        let command = vec!["sh".to_string(), "-c".to_string(), "echo 'hi there'".to_string()];
        let slots: Vec<String> = Vec::new();
        let rendered = entrypoint_command(&[launch("x", HomeMode::Shared, &command, &slots, None)]);
        assert!(rendered[2].contains("exec sh -c 'echo '\\''hi there'\\'''"));
    }

    #[test]
    fn an_env_slot_that_is_not_a_name_is_dropped() {
        assert!(is_env_name("OPENAI_API_KEY"));
        assert!(is_env_name("_X1"));
        assert!(!is_env_name("openai"));
        assert!(!is_env_name("A B"));
        assert!(!is_env_name("A;rm -rf /"));

        let slots = vec!["A;rm -rf /".to_string(), "GOOD_ONE".to_string()];
        let command = vec!["sleep".to_string(), "infinity".to_string()];
        let rendered = entrypoint_command(&[launch("x", HomeMode::Shared, &command, &slots, None)]);
        assert!(!rendered[2].contains("rm -rf"));
        assert!(rendered[2].contains("export GOOD_ONE="));
    }
}
