//! Gateways and supervisors.
//!
//! The distinction this file encodes is the one that matters operationally
//! (ADR 0012): a gateway's *settings* are operator configuration with a version
//! and a drift indicator, while *policy* is a separate object with a hot reload
//! path. A supervisor is observed, never authored: the runtime has no
//! "supervisor policy", so neither does this model.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

/// Reachability and health of the gateway process.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum GatewayStatus {
    Unknown,
    Healthy,
    Degraded,
    Unhealthy,
    Offline,
}

impl GatewayStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Unknown => "unknown",
            Self::Healthy => "healthy",
            Self::Degraded => "degraded",
            Self::Unhealthy => "unhealthy",
            Self::Offline => "offline",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "unknown" => Self::Unknown,
            "healthy" => Self::Healthy,
            "degraded" => Self::Degraded,
            "unhealthy" => Self::Unhealthy,
            "offline" => Self::Offline,
            _ => return None,
        })
    }

    /// Whether a new sandbox may be placed here.
    ///
    /// Degraded is excluded on purpose: a gateway that is not applying its
    /// configuration reliably should not receive new workloads.
    pub fn accepts_placement(self) -> bool {
        matches!(self, Self::Healthy)
    }
}

/// A gateway: the control-plane process that provisions and polices sandboxes.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Gateway {
    pub id: Uuid,
    pub cluster_id: Uuid,
    pub node_id: Option<Uuid>,
    pub name: String,
    pub endpoint: String,
    pub status: GatewayStatus,
    pub version: Option<String>,
    /// Operator configuration. Kept as a document because it is a config file
    /// shape, applied as a whole rather than field by field.
    pub settings: serde_json::Value,
    /// What the control plane wants.
    pub settings_version: i64,
    /// What the gateway runs. Drift is `desired != applied`.
    pub settings_applied_version: i64,
    pub capacity: i32,
    pub used: i32,
    pub last_seen: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Gateway {
    /// Whether the gateway is running the configuration the control plane wants.
    ///
    /// This is the single definition of drift. The console badge, the sync
    /// decision, and the placement filter all read it, so they cannot disagree.
    pub fn is_drifted(&self) -> bool {
        self.settings_applied_version < self.settings_version
    }

    /// Free capacity, floored at zero so a gateway that is over-committed does
    /// not produce a negative weight in placement.
    pub fn free_capacity(&self) -> i32 {
        (self.capacity - self.used).max(0)
    }

    /// Whether this gateway is a sensible placement target right now.
    pub fn accepts_placement(&self) -> bool {
        self.status.accepts_placement() && !self.is_drifted() && self.free_capacity() > 0
    }
}

/// What kind of change a settings update requires to take effect.
///
/// The runtime distinguishes these, so the console can tell an operator whether
/// they are looking at a live change or one that needs a restart. Pretending
/// everything is live is the failure this enum prevents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum SettingsApplyMode {
    /// Applied by the sync job without a restart.
    Hot,
    /// Requires restarting the gateway process; affects every sandbox on it.
    RestartRequired,
}

/// A supervisor: the per-sandbox agent that enforces policy inside the sandbox.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Supervisor {
    pub id: Uuid,
    pub gateway_id: Uuid,
    pub sandbox_id: Option<Uuid>,
    pub name: String,
    pub status: SupervisorStatus,
    pub version: Option<String>,
    /// How many configuration deliveries are still pending. Reported by the
    /// runtime, not inferred, so it cannot be wrong in a way we invented.
    pub config_dirtiness: i32,
    pub last_policy_applied_at: Option<DateTime<Utc>>,
    pub last_seen: Option<DateTime<Utc>>,
    /// The last report as the supervisor sent it: its measured checks and the
    /// facts behind them. `None` until the process has reported once, which is
    /// what makes "no report" distinguishable from "reported nothing".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reported: Option<serde_json::Value>,
}

/// One measured best-practice check the supervisor reports.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SupervisorCheck {
    /// `ok`, `warn`, or `fail`.
    pub state: String,
    /// Stable key, e.g. `model_reachable`.
    pub key: String,
    /// What was measured, in one sentence, with the number.
    pub detail: String,
}

/// What a supervisor reports about its sandbox.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SupervisorReport {
    pub status: SupervisorStatus,
    pub version: Option<String>,
    pub config_dirtiness: i32,
    pub last_policy_applied_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub checks: Vec<SupervisorCheck>,
    #[serde(default)]
    pub facts: serde_json::Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum SupervisorStatus {
    Unknown,
    Starting,
    Running,
    Degraded,
    Stopped,
    Failed,
}

impl SupervisorStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Unknown => "unknown",
            Self::Starting => "starting",
            Self::Running => "running",
            Self::Degraded => "degraded",
            Self::Stopped => "stopped",
            Self::Failed => "failed",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "unknown" => Self::Unknown,
            "starting" => Self::Starting,
            "running" => Self::Running,
            "degraded" => Self::Degraded,
            "stopped" => Self::Stopped,
            "failed" => Self::Failed,
            _ => return None,
        })
    }
}

impl Supervisor {
    /// A supervisor with pending configuration is out of step with the control
    /// plane even though it is running. That is worth surfacing separately from
    /// health, because a running supervisor can be applying a stale policy.
    pub fn has_pending_config(&self) -> bool {
        self.config_dirtiness > 0
    }
}

/// The result of a sync attempt, shown after the button is pressed.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SyncOutcome {
    pub gateway_id: Uuid,
    pub job_id: Option<Uuid>,
    pub applied_version: i64,
    pub outcome: SyncResult,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum SyncResult {
    /// Nothing to do: desired equals applied.
    AlreadyInSync,
    /// A job was started to push the configuration.
    Queued,
    /// The gateway must restart; the change cannot be applied live.
    RestartRequired,
    /// The push failed; the desired version is unchanged so the next run retries.
    Failed,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gateway(status: GatewayStatus, desired: i64, applied: i64, cap: i32, used: i32) -> Gateway {
        Gateway {
            id: Uuid::nil(),
            cluster_id: Uuid::nil(),
            node_id: None,
            name: "gw".into(),
            endpoint: "http://gw:8081".into(),
            status,
            version: None,
            settings: serde_json::json!({}),
            settings_version: desired,
            settings_applied_version: applied,
            capacity: cap,
            used,
            last_seen: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn drift_is_desired_above_applied() {
        assert!(!gateway(GatewayStatus::Healthy, 3, 3, 10, 1).is_drifted());
        assert!(gateway(GatewayStatus::Healthy, 4, 3, 10, 1).is_drifted());
        // An applied version ahead of desired is not drift; it is a rollback in
        // progress or a stale row, and treating it as drift would flap.
        assert!(!gateway(GatewayStatus::Healthy, 3, 4, 10, 1).is_drifted());
    }

    #[test]
    fn a_drifted_gateway_is_not_a_placement_target() {
        assert!(gateway(GatewayStatus::Healthy, 1, 1, 10, 1).accepts_placement());
        assert!(!gateway(GatewayStatus::Healthy, 2, 1, 10, 1).accepts_placement());
        assert!(!gateway(GatewayStatus::Degraded, 1, 1, 10, 1).accepts_placement());
        assert!(!gateway(GatewayStatus::Healthy, 1, 1, 10, 10).accepts_placement());
    }

    #[test]
    fn free_capacity_never_goes_negative() {
        assert_eq!(gateway(GatewayStatus::Healthy, 1, 1, 4, 9).free_capacity(), 0);
    }
}
