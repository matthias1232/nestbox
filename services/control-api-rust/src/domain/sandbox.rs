//! Sandboxes: the workload, its desired state, and its lifecycle.
//!
//! The distinction that matters: `status` is what the control plane believes,
//! `health` is what the runtime last reported, and `spec` is what was asked
//! for. Keeping all three means a migration between gateways has something to
//! carry, and the console can show "asked for X, running Y".

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

/// Where a sandbox is in its lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum SandboxStatus {
    Pending,
    Provisioning,
    Starting,
    Running,
    Stopping,
    Stopped,
    Failed,
    Deleting,
    Deleted,
}

impl SandboxStatus {
    /// The database representation, kept in step with the migration's `CHECK`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Provisioning => "provisioning",
            Self::Starting => "starting",
            Self::Running => "running",
            Self::Stopping => "stopping",
            Self::Stopped => "stopped",
            Self::Failed => "failed",
            Self::Deleting => "deleting",
            Self::Deleted => "deleted",
        }
    }

    /// Parse the database representation. An unknown value is a data error, not
    /// a default, so it returns `None` and the caller reports it.
    pub fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "pending" => Self::Pending,
            "provisioning" => Self::Provisioning,
            "starting" => Self::Starting,
            "running" => Self::Running,
            "stopping" => Self::Stopping,
            "stopped" => Self::Stopped,
            "failed" => Self::Failed,
            "deleting" => Self::Deleting,
            "deleted" => Self::Deleted,
            _ => return None,
        })
    }

    /// Whether the sandbox currently consumes capacity on its gateway.
    pub fn is_active(self) -> bool {
        matches!(
            self,
            Self::Pending | Self::Provisioning | Self::Starting | Self::Running
        )
    }
}

/// Last reported health, distinct from the lifecycle status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum SandboxHealth {
    Unknown,
    Healthy,
    Degraded,
    Unhealthy,
}

impl SandboxHealth {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Unknown => "unknown",
            Self::Healthy => "healthy",
            Self::Degraded => "degraded",
            Self::Unhealthy => "unhealthy",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "unknown" => Self::Unknown,
            "healthy" => Self::Healthy,
            "degraded" => Self::Degraded,
            "unhealthy" => Self::Unhealthy,
            _ => return None,
        })
    }
}

/// A sandbox as the API returns it.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Sandbox {
    pub id: Uuid,
    pub environment_id: Uuid,
    pub cluster_id: Uuid,
    pub gateway_id: Option<Uuid>,
    pub gateway_name: Option<String>,
    pub supervisor_id: Option<Uuid>,
    pub workspace_id: Option<Uuid>,
    /// The node this sandbox runs on. Every sandbox has one; Phase 1 has one node.
    pub node_id: Uuid,
    /// The node's name, joined for the list view.
    #[serde(default)]
    pub node_name: Option<String>,
    pub name: String,
    pub pack_key: String,
    pub image: String,
    /// The pinned digest. `None` is a configuration error the image policy test
    /// rejects before it can be deployed.
    pub image_digest: Option<String>,
    /// Desired state, kept so a later migration has something to carry.
    pub spec: serde_json::Value,
    pub status: SandboxStatus,
    pub health: SandboxHealth,
    /// Optimistic lock. A write with a stale version is rejected.
    pub version: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    /// Only present when the caller asked for the detail view; the list omits it
    /// so a page of 50 rows does not ship 50 documents.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub spec_detail: Option<serde_json::Value>,
}

impl Sandbox {
    /// Whether this row can accept a lifecycle action.
    ///
    /// A rule, therefore it lives in the domain rather than in the handler: the
    /// API and any future bulk worker must agree on what "running" permits.
    pub fn can_start(&self) -> bool {
        matches!(self.status, SandboxStatus::Stopped | SandboxStatus::Failed)
    }

    pub fn can_stop(&self) -> bool {
        matches!(
            self.status,
            SandboxStatus::Running | SandboxStatus::Starting | SandboxStatus::Provisioning
        )
    }

    pub fn can_delete(&self) -> bool {
        !matches!(self.status, SandboxStatus::Deleting)
    }
}

/// The tenant-scoped fields needed to place a new sandbox.
#[derive(Debug, Clone)]
pub struct SandboxPlacementRequest {
    pub environment_id: Uuid,
    pub cluster_id: Uuid,
    pub pack_key: String,
    pub image: String,
    pub image_digest: Option<String>,
    pub spec: serde_json::Value,
}

/// A validated sandbox name.
///
/// The constructor is the only way to build one, so an invalid name cannot
/// reach a query, a log line, or a container label.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SandboxName(String);

impl SandboxName {
    /// Maximum length chosen to keep a Kubernetes-style label usable and a
    /// hostname derivable from it without truncation.
    const MAX_LEN: usize = 63;

    /// Validate and wrap.
    ///
    /// Lowercase alphanumeric and hyphens, must start and end alphanumeric. The
    /// rule is the intersection of DNS labels and container names, so the same
    /// value works as a hostname, a label, and a route.
    pub fn new(raw: &str) -> Result<Self, String> {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return Err("sandbox name must not be empty".into());
        }
        if trimmed.len() > Self::MAX_LEN {
            return Err(format!(
                "sandbox name must be at most {} characters",
                Self::MAX_LEN
            ));
        }
        let bytes = trimmed.as_bytes();
        let starts_ok = bytes[0].is_ascii_lowercase() || bytes[0].is_ascii_digit();
        let ends_ok = {
            let last = bytes[bytes.len() - 1];
            last.is_ascii_lowercase() || last.is_ascii_digit()
        };
        if !starts_ok || !ends_ok {
            return Err("sandbox name must start and end with a letter or digit".into());
        }
        if !trimmed
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        {
            return Err("sandbox name may contain only lowercase letters, digits and hyphens".into());
        }
        Ok(Self(trimmed.to_string()))
    }

    /// The validated value.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for SandboxName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_a_normal_name() {
        assert!(SandboxName::new("hermes-01").is_ok());
        assert!(SandboxName::new("dev").is_ok());
        assert!(SandboxName::new("a1-b2").is_ok());
    }

    #[test]
    fn rejects_names_that_would_break_a_hostname_or_label() {
        assert!(SandboxName::new("").is_err());
        assert!(SandboxName::new("-leading").is_err());
        assert!(SandboxName::new("trailing-").is_err());
        assert!(SandboxName::new("Upper").is_err());
        assert!(SandboxName::new("with space").is_err());
        assert!(SandboxName::new("with_underscore").is_err());
        assert!(SandboxName::new(&"a".repeat(64)).is_err());
    }

    #[test]
    fn rejects_injection_shaped_input_as_a_name() {
        // The value is data, never SQL. This asserts it does not even survive
        // validation, which removes a whole class of downstream doubt.
        assert!(SandboxName::new("'; DROP TABLE sandboxes; --").is_err());
        assert!(SandboxName::new("x\0y").is_err());
    }

    #[test]
    fn lifecycle_rules_are_explicit() {
        let mut sb = Sandbox {
            id: Uuid::nil(),
            environment_id: Uuid::nil(),
            cluster_id: Uuid::nil(),
            node_id: Uuid::nil(),
            node_name: None,
            gateway_id: None,
            gateway_name: None,
            supervisor_id: None,
            workspace_id: None,
            name: "x".into(),
            pack_key: "ubuntu-dev".into(),
            image: "img@sha256:abc".into(),
            image_digest: Some("sha256:abc".into()),
            spec: serde_json::json!({}),
            status: SandboxStatus::Running,
            health: SandboxHealth::Healthy,
            version: 1,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            spec_detail: None,
        };
        assert!(sb.can_stop());
        assert!(!sb.can_start());
        sb.status = SandboxStatus::Stopped;
        assert!(sb.can_start());
        assert!(!sb.can_stop());
    }
}

/// A sandbox plus what the runtime reports about it.
///
/// Kept separate from [`Sandbox`] because the two answer different questions:
/// the stored row is the *desired* state the control plane maintains, and the
/// runtime facts are what is actually happening. Merging them into one type
/// would make it possible to persist a fact as if it were an intention.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SandboxDetail {
    #[serde(flatten)]
    pub sandbox: Sandbox,
    /// Absent when the runtime is unreachable. The console shows the stored
    /// state and marks the live data as unavailable rather than showing zeros.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runtime: Option<crate::domain::runtime::RuntimeFacts>,
}

/// A sandbox row plus the few runtime facts the list needs.
///
/// The list would otherwise have to either omit the address and uptime or call
/// the runtime once per row. Both are wrong: the first hides the thing an
/// operator scans for, the second makes a page of 50 rows 50 round trips. The
/// service calls the runtime once and fans the result out here.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SandboxSummary {
    #[serde(flatten)]
    pub sandbox: Sandbox,
    /// Present only when the runtime answered.
    pub address: Option<String>,
    pub uptime_seconds: Option<u64>,
    pub container_name: Option<String>,
    /// False when the runtime could not be reached, so the console can label the
    /// row as "stored state only" instead of showing blanks as zeros.
    pub runtime_available: bool,
}
