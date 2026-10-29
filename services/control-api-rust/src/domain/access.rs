//! How an operator reaches a sandbox from outside: SSH keys, published ports,
//! and the inventory of what is inside.
//!
//! The SSH private key is a file on the node; this module carries a locator
//! and a fingerprint, never the key (ADR 0005). The one place the key crosses
//! the wire is the single download, and the type for that answer is separate
//! so it cannot be returned by accident from a list endpoint.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

/// An SSH key pair as stored: public half, locator, fingerprint.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct SshKey {
    pub id: Uuid,
    pub sandbox_id: Uuid,
    pub public_key: String,
    /// `file://...`; never the material.
    #[serde(skip_serializing)]
    pub private_key_ref: String,
    pub fingerprint: String,
    /// Set on the first download. A second download is refused until rotation.
    pub downloaded_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub rotated_at: Option<DateTime<Utc>>,
}

impl SshKey {
    /// Whether the private key can still be downloaded.
    pub fn downloadable(&self) -> bool {
        self.downloaded_at.is_none()
    }
}

/// The one-time download. A separate type so the private material has exactly
/// one return path and no list or detail endpoint can carry it.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct SshKeyDownload {
    pub sandbox_id: Uuid,
    pub fingerprint: String,
    pub private_key_pem: String,
    pub filename: String,
}

/// What a published port is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ForwardPurpose {
    Ssh,
    Agent,
    Custom,
}

impl ForwardPurpose {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ssh => "ssh",
            Self::Agent => "agent",
            Self::Custom => "custom",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "ssh" => Self::Ssh,
            "agent" => Self::Agent,
            "custom" => Self::Custom,
            _ => return None,
        })
    }
}

/// A port published on the node for a sandbox port.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct PublishedPort {
    pub forward_id: Uuid,
    pub sandbox_id: Uuid,
    pub node_id: Uuid,
    /// The address a client uses from outside the node.
    pub public_address: Option<String>,
    pub public_port: i32,
    pub target_port: i32,
    pub purpose: ForwardPurpose,
    pub label: Option<String>,
    /// pending | active | stopped | failed
    pub state: String,
}

impl PublishedPort {
    /// The `host:port` a client dials, or `None` when the node has no public
    /// address yet.
    pub fn dial(&self) -> Option<String> {
        self.public_address
            .as_ref()
            .map(|address| format!("{address}:{}", self.public_port))
    }
}

/// The user the SSH key logs in as. The same user the agents run as, so a login
/// lands in the home the agent works in.
pub const SSH_USER: &str = "nestbox";

/// One way in: a published port with what to type to use it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct ConnectPort {
    pub forward_id: Uuid,
    pub label: Option<String>,
    pub purpose: ForwardPurpose,
    /// The port inside the sandbox.
    pub target_port: i32,
    /// The port on the node, the one a client dials.
    pub public_port: i32,
    /// pending | active | stopped | failed
    pub state: String,
    /// `host:port`, or `None` while the node has nothing to dial.
    pub dial: Option<String>,
    /// A ready `ssh` line, for the SSH port only.
    pub command: Option<String>,
    /// What this port is, in a sentence, from the pack's catalog entry.
    #[serde(default)]
    pub description: Option<String>,
    /// `ssh`, `web`, `api` or `tcp`: what kind of client connects.
    #[serde(default = "default_kind")]
    pub kind: String,
    /// A browser URL, for a `web` or `api` port that can be dialled.
    #[serde(default)]
    pub url: Option<String>,
    /// The runner holds the node-side socket. `None` when the runner could not
    /// be asked.
    #[serde(default)]
    pub listening: Option<bool>,
    /// Something answers behind it inside the sandbox. `None` when unknown.
    #[serde(default)]
    pub target_up: Option<bool>,
    /// Why the runner could not open this port, when it could not.
    #[serde(default)]
    pub problem: Option<String>,
}

fn default_kind() -> String {
    "tcp".to_string()
}

/// One thing to type, with what it does.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct ConnectCommand {
    pub label: String,
    pub description: String,
    pub command: String,
}

/// Where the SSH key stands, without the key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct ConnectKey {
    pub fingerprint: String,
    /// The private half has not been downloaded yet. It can be once.
    pub downloadable: bool,
}

/// Everything an operator needs to reach one sandbox from their own machine.
///
/// `host` is what to dial: the node's hostname when it has one, its address
/// otherwise. It is the node's, not the container's — a container address on the
/// runtime network is not reachable from outside and is deliberately absent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct ConnectInfo {
    pub sandbox_id: Uuid,
    pub sandbox_name: String,
    pub node_id: Uuid,
    pub node_name: String,
    pub address: Option<String>,
    pub hostname: Option<String>,
    pub host: Option<String>,
    pub ssh_user: String,
    pub key: Option<ConnectKey>,
    pub ports: Vec<ConnectPort>,
    /// A block for `~/.ssh/config`, once there is a host and an SSH port.
    pub ssh_config: Option<String>,
    /// Why the connection cannot work yet, in a sentence, or `None`.
    pub hint: Option<String>,
    /// What this sandbox is, from the pack's catalog entry.
    #[serde(default)]
    pub summary: Option<String>,
    /// The pack that decides the descriptions below.
    #[serde(default)]
    pub pack_key: Option<String>,
    /// Ready-to-run lines beyond the plain SSH one: start the agent, attach a client.
    #[serde(default)]
    pub commands: Vec<ConnectCommand>,
    /// What the SSH door does not do, said once instead of discovered by failing.
    #[serde(default)]
    pub notes: Vec<String>,
    /// `live` when the runner answered about the ports, `unknown` when it did not.
    #[serde(default = "default_forwarding")]
    pub forwarding: String,
}

fn default_forwarding() -> String {
    "unknown".to_string()
}

/// What `build_connect` needs; a struct so the call site names each fact.
pub struct ConnectInput<'a> {
    pub sandbox_id: Uuid,
    pub sandbox_name: &'a str,
    pub node_id: Uuid,
    pub node_name: &'a str,
    pub address: Option<&'a str>,
    pub hostname: Option<&'a str>,
    pub ports: &'a [PublishedPort],
    pub key: Option<ConnectKey>,
}

/// Assemble the connection facts. Pure, so the sentences and the commands are
/// tested without a database.
pub fn build_connect(input: ConnectInput<'_>) -> ConnectInfo {
    let clean = |value: Option<&str>| {
        value
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .map(str::to_string)
    };
    let address = clean(input.address);
    let hostname = clean(input.hostname);
    let host = hostname.clone().or_else(|| address.clone());
    let identity = format!("~/.ssh/nestbox-{}", input.sandbox_name);

    let ports: Vec<ConnectPort> = input
        .ports
        .iter()
        .map(|port| {
            let dial = host.as_ref().map(|host| format!("{host}:{}", port.public_port));
            let command = match (&host, port.purpose) {
                (Some(host), ForwardPurpose::Ssh) => Some(format!(
                    "ssh -i {identity} -p {} {SSH_USER}@{host}",
                    port.public_port
                )),
                _ => None,
            };
            ConnectPort {
                forward_id: port.forward_id,
                label: port.label.clone(),
                purpose: port.purpose,
                target_port: port.target_port,
                public_port: port.public_port,
                state: port.state.clone(),
                dial,
                command,
                description: None,
                kind: if port.purpose == ForwardPurpose::Ssh { "ssh" } else { "tcp" }.to_string(),
                url: None,
                listening: None,
                target_up: None,
                problem: None,
            }
        })
        .collect();

    let ssh_config = host.as_ref().and_then(|host| {
        input
            .ports
            .iter()
            .find(|port| port.purpose == ForwardPurpose::Ssh)
            .map(|port| {
                format!(
                    "Host {name}\n  HostName {host}\n  Port {port}\n  User {SSH_USER}\n  IdentityFile {identity}\n  IdentitiesOnly yes\n",
                    name = input.sandbox_name,
                    port = port.public_port,
                )
            })
    });

    let hint = if host.is_none() {
        Some(
            "This node has no address clients can dial. Published ports are bound to 127.0.0.1 on the node until an address is set."
                .to_string(),
        )
    } else if input.ports.is_empty() {
        Some("No port is published for this sandbox yet.".to_string())
    } else {
        None
    };

    ConnectInfo {
        sandbox_id: input.sandbox_id,
        sandbox_name: input.sandbox_name.to_string(),
        node_id: input.node_id,
        node_name: input.node_name.to_string(),
        address,
        hostname,
        host,
        ssh_user: SSH_USER.to_string(),
        key: input.key,
        ports,
        ssh_config,
        hint,
        summary: None,
        pack_key: None,
        commands: Vec::new(),
        notes: Vec::new(),
        forwarding: default_forwarding(),
    }
}

/// What the fixed inventory script found inside the container.
///
/// `env_keys` is names only. The script that fills this never prints a value,
/// and the type has no field to hold one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Inventory {
    pub sandbox_id: Uuid,
    pub taken_at: DateTime<Utc>,
    pub os: serde_json::Value,
    pub packages: serde_json::Value,
    pub agents: serde_json::Value,
    pub tool_versions: serde_json::Value,
    pub listening: serde_json::Value,
    pub env_keys: Vec<String>,
    pub mounts: serde_json::Value,
    pub image: serde_json::Value,
    pub note: Option<String>,
}

/// A terminal or SSH session, for the audit trail.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct SandboxSession {
    pub id: Uuid,
    pub sandbox_id: Uuid,
    pub user_id: Option<Uuid>,
    pub kind: SessionKind,
    pub opened_at: DateTime<Utc>,
    pub closed_at: Option<DateTime<Utc>>,
    pub close_reason: Option<String>,
    pub bytes_in: i64,
    pub bytes_out: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum SessionKind {
    Terminal,
    Ssh,
}

impl SessionKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Terminal => "terminal",
            Self::Ssh => "ssh",
        }
    }

    /// The kind a stored `kind` column names, or `None` for a value this
    /// version does not know.
    pub fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "terminal" => Self::Terminal,
            "ssh" => Self::Ssh,
            _ => return None,
        })
    }
}

/// The range a node publishes ports from, and the address a client outside the
/// node dials. Read from `nodes.public_address` and the pool bounds (0021).
///
/// The pool is a property of the node, not of a sandbox: two sandboxes on one
/// node draw from the same range, which is why the claim is a unique key
/// `(node_id, port)` rather than a per-sandbox counter.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct NodePortPool {
    pub node_id: Uuid,
    /// The address a client uses. `None` until the node is given one.
    pub public_address: Option<String>,
    /// First port of the range, inclusive.
    pub start: i32,
    /// Last port of the range, inclusive.
    pub end: i32,
}

impl NodePortPool {
    /// How many ports the range holds. Never negative, even for a misconfigured
    /// row, so a caller cannot turn a bad range into a huge loop.
    pub fn capacity(&self) -> i32 {
        (self.end - self.start + 1).max(0)
    }

    /// Whether a port belongs to this node's range.
    pub fn contains(&self, port: i32) -> bool {
        port >= self.start && port <= self.end
    }
}

/// Whether a container port can be published at all.
pub fn is_publishable_port(port: i32) -> bool {
    (1..=65535).contains(&port)
}

/// Limits on interactive access, as one place to read them from.
pub struct AccessLimits;

impl AccessLimits {
    /// A terminal that has seen no input for this long is closed.
    pub const TERMINAL_IDLE_SECONDS: u64 = 30 * 60;
    /// Open terminals per sandbox.
    pub const TERMINALS_PER_SANDBOX: usize = 4;
    /// Open terminals per user across sandboxes.
    pub const TERMINALS_PER_USER: usize = 20;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn forward(purpose: ForwardPurpose, public_port: i32, target_port: i32) -> PublishedPort {
        PublishedPort {
            forward_id: Uuid::new_v4(),
            sandbox_id: Uuid::nil(),
            node_id: Uuid::nil(),
            public_address: None,
            public_port,
            target_port,
            purpose,
            label: None,
            state: "active".into(),
        }
    }

    fn connect(address: Option<&str>, hostname: Option<&str>, ports: &[PublishedPort]) -> ConnectInfo {
        build_connect(ConnectInput {
            sandbox_id: Uuid::nil(),
            sandbox_name: "hermes",
            node_id: Uuid::nil(),
            node_name: "vm200",
            address,
            hostname,
            ports,
            key: None,
        })
    }

    #[test]
    fn the_hostname_is_dialled_before_the_address() {
        let ports = [forward(ForwardPurpose::Ssh, 22101, 22)];
        let info = connect(Some("10.0.0.5"), Some("box.example.net"), &ports);
        assert_eq!(info.host.as_deref(), Some("box.example.net"));
        assert_eq!(info.ports[0].dial.as_deref(), Some("box.example.net:22101"));
        let without = connect(Some("10.0.0.5"), None, &ports);
        assert_eq!(without.host.as_deref(), Some("10.0.0.5"));
    }

    #[test]
    fn only_the_ssh_port_gets_an_ssh_line_and_the_config_names_it() {
        let ports = [
            forward(ForwardPurpose::Ssh, 22101, 22),
            forward(ForwardPurpose::Agent, 22102, 8080),
        ];
        let info = connect(Some("10.0.0.5"), None, &ports);
        assert_eq!(
            info.ports[0].command.as_deref(),
            Some("ssh -i ~/.ssh/nestbox-hermes -p 22101 nestbox@10.0.0.5")
        );
        assert!(info.ports[1].command.is_none());
        let config = info.ssh_config.expect("config");
        assert!(config.contains("Host hermes") && config.contains("Port 22101"));
        assert!(info.hint.is_none());
    }

    #[test]
    fn a_node_without_an_address_says_why_nothing_can_be_dialled() {
        let ports = [forward(ForwardPurpose::Ssh, 22101, 22)];
        let info = connect(None, Some("  "), &ports);
        assert!(info.host.is_none());
        assert!(info.ports[0].dial.is_none() && info.ssh_config.is_none());
        assert!(info.hint.expect("hint").contains("127.0.0.1"));
    }

    #[test]
    fn no_published_port_is_said_rather_than_shown_as_an_empty_table() {
        let info = connect(Some("10.0.0.5"), None, &[]);
        assert_eq!(info.hint.as_deref(), Some("No port is published for this sandbox yet."));
    }
    use chrono::Utc;

    #[test]
    fn a_key_is_downloadable_exactly_until_it_has_been_downloaded() {
        let mut key = SshKey {
            id: Uuid::nil(),
            sandbox_id: Uuid::nil(),
            public_key: "ssh-ed25519 AAAA".into(),
            private_key_ref: "file:///secrets/ssh/x.key".into(),
            fingerprint: "SHA256:abc".into(),
            downloaded_at: None,
            created_at: Utc::now(),
            rotated_at: None,
        };
        assert!(key.downloadable());
        key.downloaded_at = Some(Utc::now());
        assert!(!key.downloadable());
    }

    #[test]
    fn the_private_key_locator_is_never_serialised() {
        let key = SshKey {
            id: Uuid::nil(),
            sandbox_id: Uuid::nil(),
            public_key: "ssh-ed25519 AAAA".into(),
            private_key_ref: "file:///secrets/ssh/x.key".into(),
            fingerprint: "SHA256:abc".into(),
            downloaded_at: None,
            created_at: Utc::now(),
            rotated_at: None,
        };
        let json = serde_json::to_string(&key).expect("serialises");
        assert!(!json.contains("private_key_ref"));
        assert!(!json.contains("/secrets/"));
    }

    #[test]
    fn a_port_dials_only_when_the_node_has_an_address() {
        let mut port = PublishedPort {
            forward_id: Uuid::nil(),
            sandbox_id: Uuid::nil(),
            node_id: Uuid::nil(),
            public_address: None,
            public_port: 22001,
            target_port: 22,
            purpose: ForwardPurpose::Ssh,
            label: None,
            state: "active".into(),
        };
        assert_eq!(port.dial(), None);
        port.public_address = Some("192.168.55.50".into());
        assert_eq!(port.dial().as_deref(), Some("192.168.55.50:22001"));
    }
}
