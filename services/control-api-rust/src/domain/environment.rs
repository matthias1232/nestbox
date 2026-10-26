//! Organization, environment, cluster, and node.
//!
//! The chain organization → environment → cluster → gateway is what makes
//! tenant isolation expressible. Every tenant-scoped row points at exactly one
//! environment, and that pointer is what the database policy checks.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

/// The top-level tenant. Users and environments belong to an organization.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Organization {
    pub id: Uuid,
    pub name: String,
    pub slug: String,
    pub created_at: DateTime<Utc>,
}

/// The tenant boundary. Every tenant-scoped query filters on this id.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Environment {
    pub id: Uuid,
    pub organization_id: Uuid,
    pub name: String,
    pub slug: String,
    pub timezone: String,
    /// Telemetry older than this is dropped by partition maintenance.
    pub retention_days: i32,
    pub created_at: DateTime<Utc>,
}

/// A group of gateways that can host the same workloads.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Cluster {
    pub id: Uuid,
    pub environment_id: Uuid,
    pub name: String,
    pub slug: String,
    /// How placement picks a gateway: `least_loaded`, `zone_aware`, `pinned`.
    pub placement_policy: PlacementPolicy,
    pub created_at: DateTime<Utc>,
}

/// How the scheduler chooses a gateway for a new sandbox.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum PlacementPolicy {
    /// Prefer the gateway with the most free capacity.
    LeastLoaded,
    /// Prefer a gateway in the same zone, then by capacity.
    ZoneAware,
    /// The sandbox names its gateway; the scheduler does not choose.
    Pinned,
}

impl PlacementPolicy {
    /// The database representation. One place, so the enum and the `CHECK`
    /// constraint cannot drift.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::LeastLoaded => "least_loaded",
            Self::ZoneAware => "zone_aware",
            Self::Pinned => "pinned",
        }
    }

    /// Parse the database representation.
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "least_loaded" => Some(Self::LeastLoaded),
            "zone_aware" => Some(Self::ZoneAware),
            "pinned" => Some(Self::Pinned),
            _ => None,
        }
    }
}

/// A machine. Gateways run on nodes; a node's capabilities inform scheduling.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Node {
    pub id: Uuid,
    pub name: String,
    pub zone: String,
    pub region: String,
    pub endpoint: Option<String>,
    /// Free-form facts a scheduler may consult: GPU count, memory, labels.
    pub capabilities: serde_json::Value,
    pub last_seen: Option<DateTime<Utc>>,
}

/// Counts per environment, for the switcher and the overview.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct EnvironmentSummary {
    pub id: Uuid,
    pub name: String,
    pub slug: String,
    pub sandbox_count: i64,
    pub gateway_count: i64,
    pub running_count: i64,
}
