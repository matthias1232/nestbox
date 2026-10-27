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
