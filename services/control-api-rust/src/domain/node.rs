//! A node: the machine a sandbox or a model runs on.
//!
//! The control plane coordinates nodes; it is not one of them. Phase 1 has one
//! node, `local`, which is the host the control plane runs on. The type carries
//! the fields a remote node will need (`kind`, `runner_url`) so that adding one
//! later is a row and an adapter, not a schema change.
//!
//! Capacity is **not** a field here. It is the latest `HardwareProfile` for the
//! node minus what its model installs reserve, computed in the service layer,
//! so there is one number for "how much VRAM does node-a have".

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

/// What kind of machine a node is. Decides the adapter it will need.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum NodeKind {
    /// The host the control plane runs on.
    Local,
    Vm,
    Wsl,
    BareMetal,
    /// Declared so the data model needs no change later; no adapter exists.
    Kubernetes,
}

impl NodeKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::Vm => "vm",
            Self::Wsl => "wsl",
            Self::BareMetal => "bare_metal",
            Self::Kubernetes => "kubernetes",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "local" => Self::Local,
            "vm" => Self::Vm,
            "wsl" => Self::Wsl,
            "bare_metal" => Self::BareMetal,
            "kubernetes" => Self::Kubernetes,
            _ => return None,
        })
    }

    /// Whether an adapter exists that can run sandboxes on this kind.
    pub fn is_operable(self) -> bool {
        !matches!(self, Self::Kubernetes)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum NodeStatus {
    Online,
    Offline,
    Degraded,
    Unknown,
}

impl NodeStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Online => "online",
            Self::Offline => "offline",
            Self::Degraded => "degraded",
            Self::Unknown => "unknown",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "online" => Self::Online,
            "offline" => Self::Offline,
            "degraded" => Self::Degraded,
            "unknown" => Self::Unknown,
            _ => return None,
        })
    }
}

/// A node as stored and returned.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Node {
    pub id: Uuid,
    /// Stable identifier used in URLs: `local`, `gpu-box-01`.
    pub name: String,
    pub display_name: String,
    pub kind: NodeKind,
    pub status: NodeStatus,
    pub is_local: bool,
    /// Empty for the local node; its runner is the control plane's own.
    pub runner_url: String,
    /// Whether containers on this node can be given a GPU. Measured by the
    /// runner; a GPU model on a node without it is `no_gpu`.
    pub container_gpu: bool,
    pub zone: String,
    pub region: String,
    pub description: Option<String>,
    pub last_seen: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

/// What a node has left after its installs reserved their share.
///
/// `total` is the last measurement, `reserved` the sum over installed models,
/// `free` the difference, never below zero. The fit classifier runs against
/// `free`, so a second model that no longer fits is reported as such.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct NodeBudget {
    pub ram_total_bytes: i64,
    pub ram_reserved_bytes: i64,
    pub ram_free_bytes: i64,
    pub vram_total_bytes: i64,
    pub vram_reserved_bytes: i64,
    pub vram_free_bytes: i64,
    pub cpu_cores: i32,
    pub gpu_count: i32,
    /// How many installs contributed to `reserved`.
    pub installs: i32,
    /// How many sandboxes sit on this node.
    pub sandboxes: i32,
}

impl NodeBudget {
    /// Subtract a reservation, clamping at zero.
    ///
    /// Clamping rather than going negative keeps a bar drawable and a verdict
    /// explainable: "0 GiB free" is a fact, "-3 GiB free" is a bug report.
    pub fn reserve(&mut self, ram_bytes: i64, vram_bytes: i64) {
        self.ram_reserved_bytes += ram_bytes.max(0);
        self.vram_reserved_bytes += vram_bytes.max(0);
        self.ram_free_bytes = (self.ram_total_bytes - self.ram_reserved_bytes).max(0);
        self.vram_free_bytes = (self.vram_total_bytes - self.vram_reserved_bytes).max(0);
        self.installs += 1;
    }

    /// Whether a request for this much RAM and this many cores fits what is
    /// left. Cores are not reserved per install, so only RAM is subtracted.
    pub fn can_host(&self, ram_bytes: i64, cores: f64) -> Result<(), InsufficientResources> {
        if ram_bytes > self.ram_free_bytes {
            return Err(InsufficientResources::Ram {
                requested: ram_bytes,
                free: self.ram_free_bytes,
            });
        }
        if self.cpu_cores > 0 && cores > f64::from(self.cpu_cores) {
            return Err(InsufficientResources::Cpu {
                requested: cores,
                available: self.cpu_cores,
            });
        }
        Ok(())
    }
}

/// Why a node cannot host a request.
#[derive(Debug, Clone, PartialEq)]
pub enum InsufficientResources {
    Ram { requested: i64, free: i64 },
    Cpu { requested: f64, available: i32 },
}

impl InsufficientResources {
    /// A sentence for the error response.
    pub fn message(&self) -> String {
        const GIB: f64 = 1024.0 * 1024.0 * 1024.0;
        match self {
            Self::Ram { requested, free } => format!(
                "The node has {:.1} GiB of RAM free; the request needs {:.1} GiB.",
                *free as f64 / GIB,
                *requested as f64 / GIB
            ),
            Self::Cpu { requested, available } => format!(
                "The node has {available} CPU cores; the request asks for {requested}."
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GIB: i64 = 1024 * 1024 * 1024;

    fn budget(ram: i64, vram: i64, cores: i32) -> NodeBudget {
        NodeBudget {
            ram_total_bytes: ram,
            ram_free_bytes: ram,
            vram_total_bytes: vram,
            vram_free_bytes: vram,
            cpu_cores: cores,
            ..NodeBudget::default()
        }
    }

    #[test]
    fn a_reservation_reduces_what_is_free() {
        let mut b = budget(16 * GIB, 0, 4);
        b.reserve(5 * GIB, 0);
        assert_eq!(b.ram_free_bytes, 11 * GIB);
        assert_eq!(b.installs, 1);
    }

    #[test]
    fn free_never_goes_negative() {
        let mut b = budget(8 * GIB, 0, 4);
        b.reserve(12 * GIB, 0);
        assert_eq!(b.ram_free_bytes, 0);
        assert_eq!(b.ram_reserved_bytes, 12 * GIB);
    }

    #[test]
    fn a_request_beyond_the_free_budget_is_refused_with_the_numbers() {
        let mut b = budget(8 * GIB, 0, 4);
        b.reserve(6 * GIB, 0);
        let err = b.can_host(4 * GIB, 1.0).expect_err("should not fit");
        assert_eq!(err, InsufficientResources::Ram { requested: 4 * GIB, free: 2 * GIB });
        assert!(err.message().contains("2.0 GiB of RAM free"));
    }

    #[test]
    fn more_cores_than_the_node_has_is_refused() {
        let b = budget(8 * GIB, 0, 4);
        assert!(b.can_host(GIB, 8.0).is_err());
        assert!(b.can_host(GIB, 4.0).is_ok());
    }

    #[test]
    fn kubernetes_is_declared_but_not_operable() {
        assert!(!NodeKind::Kubernetes.is_operable());
        assert!(NodeKind::Local.is_operable());
        assert_eq!(NodeKind::parse("bare_metal"), Some(NodeKind::BareMetal));
    }
}
