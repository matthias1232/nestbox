//! What a pack says about how it is reached, and how that meets the live ports.
//!
//! The catalog (`agent_packs.access`, migration 0038) describes a pack's ports and
//! its SSH line in words. The runner reports which of them are really open. This
//! module joins the two into the answer the Access panel shows, as pure functions,
//! so every sentence is tested without a database or a container.
//!
//! Two facts are kept apart on purpose, because they fail separately:
//!
//! * **listening** — the runner holds the node-side socket, so the port is open
//!   from the LAN;
//! * **target up** — something answers behind it inside the sandbox.
//!
//! An agent that has not been started leaves a port listening with nothing behind
//! it. Calling that "active" would send an operator to a browser tab that hangs.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::domain::access::{ConnectCommand, ConnectInfo, ForwardPurpose, SSH_USER};

/// What kind of client connects to a port.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum PortKind {
    /// A browser opens it.
    Web,
    /// A program on the operator's machine connects to it.
    Api,
}

impl PortKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Web => "web",
            Self::Api => "api",
        }
    }
}

/// One port of a pack, in words.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct PortAccess {
    pub label: String,
    pub kind: PortKind,
    #[serde(default)]
    pub description: String,
    /// The path a browser opens, `/` when absent.
    #[serde(default)]
    pub path: Option<String>,
}

/// What the SSH line gets the operator.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct SshAccess {
    pub description: String,
    /// The command to type after the SSH line to start the agent.
    #[serde(default)]
    pub run: Option<String>,
}

/// A client-side command that talks to a port, e.g. `opencode attach {url}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct AgentAccess {
    pub description: String,
    pub command: String,
}

/// The catalog's description of one pack's access. Every field is optional: a
/// pack nobody described still connects, it is just described less.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct PackAccess {
    #[serde(default)]
    pub summary: Option<String>,
    #[serde(default)]
    pub ssh: Option<SshAccess>,
    /// Keyed by the container port, as a string (JSON object keys are strings).
    #[serde(default)]
    pub ports: BTreeMap<String, PortAccess>,
    #[serde(default)]
    pub agent: Option<AgentAccess>,
}

impl PackAccess {
    /// Read the catalog column. A document that does not parse is treated as
    /// "nothing described" rather than failing the whole Access panel: the
    /// description is help text, and the ports and the SSH line work without it.
    pub fn from_value(value: &serde_json::Value) -> Self {
        serde_json::from_value(value.clone()).unwrap_or_default()
    }

    fn port(&self, target_port: i32) -> Option<&PortAccess> {
        self.ports.get(&target_port.to_string())
    }
}

/// What the runner reports about one forward.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct ForwardState {
    /// The port on the node.
    pub port: i32,
    pub target_port: i32,
    pub purpose: String,
    pub listening: bool,
    pub target_up: bool,
    #[serde(default)]
    pub error: Option<String>,
}

/// What the SSH door does not do, said once.
pub const SSH_LIMITS: &str = "This SSH door opens a shell or runs one command. It has no SFTP and no port forwarding, so VS Code Remote-SSH and ssh -L/-R do not work through it. Legacy scp -O works only because it runs scp inside the sandbox.";

/// The URL a browser or client uses for a port, or `None` when there is no host.
fn url_for(dial: &str, access: &PortAccess) -> String {
    let path = access.path.as_deref().unwrap_or("/");
    let path = if path.starts_with('/') { path.to_string() } else { format!("/{path}") };
    format!("http://{dial}{path}")
}

/// Fill a `ConnectInfo` from the pack's description and the runner's report.
///
/// `live` is `None` when the runner could not be asked; the panel then says
/// "unknown" instead of claiming a state it did not read.
pub fn enrich(
    mut info: ConnectInfo,
    pack_key: &str,
    access: &PackAccess,
    live: Option<&[ForwardState]>,
) -> ConnectInfo {
    info.pack_key = Some(pack_key.to_string());
    info.summary = access.summary.clone();
    info.forwarding = if live.is_some() { "live" } else { "unknown" }.to_string();

    let mut first_client_url: Option<String> = None;
    for port in &mut info.ports {
        let described = access.port(port.target_port);
        if let Some(entry) = described {
            port.label = Some(entry.label.clone());
            port.description = Some(entry.description.clone()).filter(|text| !text.is_empty());
            port.kind = entry.kind.as_str().to_string();
            if let Some(dial) = &port.dial {
                let url = url_for(dial, entry);
                first_client_url.get_or_insert_with(|| url.clone());
                port.url = Some(url);
            }
        } else if port.purpose == ForwardPurpose::Ssh {
            port.description = Some(
                access
                    .ssh
                    .as_ref()
                    .map(|ssh| ssh.description.clone())
                    .unwrap_or_else(|| "A shell in the sandbox, over SSH with the sandbox's key.".to_string()),
            );
        }

        if let Some(states) = live {
            match states.iter().find(|state| state.port == port.public_port) {
                Some(state) => {
                    port.listening = Some(state.listening);
                    port.target_up = Some(state.target_up);
                    port.problem = state.error.clone();
                }
                None => {
                    // The database reserved the port and the runner has no
                    // listener for it: exactly the state this feature exists to
                    // make visible instead of showing "active".
                    port.listening = Some(false);
                    port.target_up = Some(false);
                    port.problem = Some("The runner has no listener for this port yet.".to_string());
                }
            }
        }
    }

    if let (Some(host), Some(ssh_port)) = (
        info.host.clone(),
        info.ports.iter().find(|port| port.purpose == ForwardPurpose::Ssh),
    ) {
        let identity = format!("~/.ssh/nestbox-{}", info.sandbox_name);
        if let Some(run) = access.ssh.as_ref().and_then(|ssh| ssh.run.as_deref()).filter(|run| *run != "bash") {
            info.commands.push(ConnectCommand {
                label: format!("Start {run} over SSH"),
                description: access
                    .ssh
                    .as_ref()
                    .map(|ssh| ssh.description.clone())
                    .unwrap_or_default(),
                command: format!(
                    "ssh -t -i {identity} -p {} {SSH_USER}@{host} {run}",
                    ssh_port.public_port
                ),
            });
        }
    }

    if let Some(agent) = &access.agent {
        if let Some(url) = &first_client_url {
            info.commands.push(ConnectCommand {
                label: "Attach a client".to_string(),
                description: agent.description.clone(),
                command: agent.command.replace("{url}", url),
            });
        }
    }

    if info.ports.iter().any(|port| port.purpose == ForwardPurpose::Ssh) {
        info.notes.push(SSH_LIMITS.to_string());
    }
    info
}

/// The two sets of ports a stack diagram draws for a sandbox: what an operator
/// reaches from outside, with its live state. Kept here so the diagram and the
/// Access panel read the same words.
pub fn port_state_word(listening: Option<bool>, target_up: Option<bool>) -> &'static str {
    match (listening, target_up) {
        (Some(true), Some(true)) => "up",
        (Some(true), _) => "open, nothing behind it",
        (Some(false), _) => "closed",
        (None, _) => "unknown",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::access::{build_connect, ConnectInput, PublishedPort};
    use uuid::Uuid;

    fn forward(purpose: ForwardPurpose, public_port: i32, target_port: i32) -> PublishedPort {
        PublishedPort {
            forward_id: Uuid::new_v4(),
            sandbox_id: Uuid::nil(),
            node_id: Uuid::nil(),
            public_address: Some("192.168.55.50".into()),
            public_port,
            target_port,
            purpose,
            label: None,
            state: "pending".into(),
        }
    }

    fn base(ports: &[PublishedPort]) -> ConnectInfo {
        build_connect(ConnectInput {
            sandbox_id: Uuid::nil(),
            sandbox_name: "oc-01",
            node_id: Uuid::nil(),
            node_name: "vm200",
            address: Some("192.168.55.50"),
            hostname: None,
            ports,
            key: None,
        })
    }

    fn opencode() -> PackAccess {
        PackAccess::from_value(&serde_json::json!({
            "summary": "Terminal coding agent.",
            "ssh": { "description": "A shell.", "run": "opencode" },
            "ports": { "4096": { "label": "OpenCode server", "kind": "api", "description": "Server port.", "path": "/" } },
            "agent": { "description": "Attach a client.", "command": "opencode attach {url}" }
        }))
    }

    #[test]
    fn a_described_port_gets_its_label_url_and_kind() {
        let ports = [forward(ForwardPurpose::Ssh, 22201, 22), forward(ForwardPurpose::Agent, 22202, 4096)];
        let info = enrich(base(&ports), "opencode", &opencode(), None);
        let port = &info.ports[1];
        assert_eq!(port.label.as_deref(), Some("OpenCode server"));
        assert_eq!(port.kind, "api");
        assert_eq!(port.url.as_deref(), Some("http://192.168.55.50:22202/"));
        assert_eq!(port.description.as_deref(), Some("Server port."));
        assert_eq!(info.summary.as_deref(), Some("Terminal coding agent."));
    }

    #[test]
    fn the_pack_adds_a_start_line_and_a_client_line() {
        let ports = [forward(ForwardPurpose::Ssh, 22201, 22), forward(ForwardPurpose::Agent, 22202, 4096)];
        let info = enrich(base(&ports), "opencode", &opencode(), None);
        assert_eq!(info.commands.len(), 2);
        assert_eq!(
            info.commands[0].command,
            "ssh -t -i ~/.ssh/nestbox-oc-01 -p 22201 nestbox@192.168.55.50 opencode"
        );
        assert_eq!(info.commands[1].command, "opencode attach http://192.168.55.50:22202/");
    }

    #[test]
    fn a_plain_shell_pack_does_not_get_a_start_line() {
        let access = PackAccess::from_value(&serde_json::json!({
            "ssh": { "description": "A shell.", "run": "bash" }
        }));
        let ports = [forward(ForwardPurpose::Ssh, 22201, 22)];
        let info = enrich(base(&ports), "ubuntu-dev", &access, None);
        assert!(info.commands.is_empty());
    }

    #[test]
    fn a_port_the_runner_does_not_hold_is_called_closed_not_active() {
        let ports = [forward(ForwardPurpose::Ssh, 22201, 22)];
        let info = enrich(base(&ports), "opencode", &opencode(), Some(&[]));
        assert_eq!(info.forwarding, "live");
        assert_eq!(info.ports[0].listening, Some(false));
        assert!(info.ports[0].problem.as_deref().unwrap_or_default().contains("no listener"));
    }

    #[test]
    fn open_with_nothing_behind_it_is_said_as_that() {
        let ports = [forward(ForwardPurpose::Agent, 22202, 4096)];
        let live = [ForwardState {
            port: 22202,
            target_port: 4096,
            purpose: "agent".into(),
            listening: true,
            target_up: false,
            error: None,
        }];
        let info = enrich(base(&ports), "opencode", &opencode(), Some(&live));
        assert_eq!(info.ports[0].listening, Some(true));
        assert_eq!(info.ports[0].target_up, Some(false));
        assert_eq!(port_state_word(Some(true), Some(false)), "open, nothing behind it");
    }

    #[test]
    fn without_a_runner_answer_the_state_is_unknown_not_invented() {
        let ports = [forward(ForwardPurpose::Ssh, 22201, 22)];
        let info = enrich(base(&ports), "opencode", &opencode(), None);
        assert_eq!(info.forwarding, "unknown");
        assert_eq!(info.ports[0].listening, None);
    }

    #[test]
    fn the_ssh_limits_are_stated_when_there_is_an_ssh_port() {
        let ports = [forward(ForwardPurpose::Ssh, 22201, 22)];
        let info = enrich(base(&ports), "x", &PackAccess::default(), None);
        assert_eq!(info.notes, vec![SSH_LIMITS.to_string()]);
        assert!(info.ports[0].description.is_some());
    }

    #[test]
    fn a_document_that_does_not_parse_describes_nothing_instead_of_failing() {
        let access = PackAccess::from_value(&serde_json::json!({ "ports": "not a map" }));
        assert_eq!(access, PackAccess::default());
    }
}
