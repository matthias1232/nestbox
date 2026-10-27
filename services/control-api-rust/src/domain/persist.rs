//! Persistent roots and mounts: data that lives outside the container.
//!
//! A root is a directory on a node's filesystem. A mount binds it into a
//! sandbox. The rule that matters: one read-write mount per root at a time, so
//! two containers never write the same files. Read-only sharing is unlimited.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum RootKind {
    /// One agent's home directory.
    AgentHome,
    /// A project folder, mounted into one or more sandboxes.
    Project,
}

impl RootKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::AgentHome => "agent_home",
            Self::Project => "project",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "agent_home" => Self::AgentHome,
            "project" => Self::Project,
            _ => return None,
        })
    }

    /// The directory prefix on the node, under the persistent base.
    pub fn prefix(self) -> &'static str {
        match self {
            Self::AgentHome => "agent",
            Self::Project => "project",
        }
    }
}

/// A persistent root as stored and returned.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PersistentRoot {
    pub id: Uuid,
    pub environment_id: Uuid,
    pub node_id: Uuid,
    pub kind: RootKind,
    pub owner_user_id: Option<Uuid>,
    pub slug: String,
    /// The directory on the node. Always under the node's persistent base.
    pub host_path: String,
    pub bytes_used: i64,
    pub measured_at: Option<DateTime<Utc>>,
    pub archived_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    /// Sandboxes this root is currently mounted in; filled by the service.
    #[serde(default)]
    pub mounted_in: Vec<MountRef>,
}

/// Where a root is mounted, for the list view.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct MountRef {
    pub sandbox_id: Uuid,
    pub sandbox_name: String,
    pub mount_path: String,
    pub read_only: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum MountStatus {
    Mounted,
    Dismounted,
    /// The host directory is gone.
    Missing,
}

impl MountStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Mounted => "mounted",
            Self::Dismounted => "dismounted",
            Self::Missing => "missing",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "mounted" => Self::Mounted,
            "dismounted" => Self::Dismounted,
            "missing" => Self::Missing,
            _ => return None,
        })
    }
}

/// A mount as stored.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Mount {
    pub id: Uuid,
    pub root_id: Uuid,
    pub sandbox_id: Uuid,
    pub mount_path: String,
    pub read_only: bool,
    pub status: MountStatus,
    pub created_at: DateTime<Utc>,
}

/// The bind the runtime applies. Source paths come from the control plane,
/// never from a request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct MountSpec {
    pub source: String,
    pub target: String,
    #[serde(default)]
    pub read_only: bool,
}

/// A valid root slug.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RootSlug(String);

impl RootSlug {
    /// Validate a slug: lowercase, digits, hyphens, 1–63 characters.
    pub fn new(value: &str) -> Result<Self, String> {
        let value = value.trim();
        if value.is_empty() || value.len() > 63 {
            return Err("A slug is 1 to 63 characters.".into());
        }
        if !value
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
            || value.starts_with('-')
        {
            return Err("A slug uses lowercase letters, digits, and hyphens.".into());
        }
        Ok(Self(value.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// The host directory for a root: `<base>/<owner>/<kind>--<slug>`.
///
/// Computed here, from validated parts, so a request can never name a path.
pub fn host_path(base: &str, owner: &str, kind: RootKind, slug: &RootSlug) -> String {
    format!(
        "{}/{}/{}--{}",
        base.trim_end_matches('/'),
        owner,
        kind.prefix(),
        slug.as_str()
    )
}

/// A valid mount target inside the container: absolute, no `..`.
pub fn validate_mount_path(value: &str) -> Result<String, String> {
    let value = value.trim();
    if !value.starts_with('/') {
        return Err("A mount path is absolute.".into());
    }
    if value.split('/').any(|part| part == "..") {
        return Err("A mount path contains no '..'.".into());
    }
    if value == "/" {
        return Err("A mount path is not the root.".into());
    }
    Ok(value.trim_end_matches('/').to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_host_path_is_derived_not_supplied() {
        let slug = RootSlug::new("alpha").expect("slug");
        assert_eq!(
            host_path("/var/lib/nestbox/persistent/", "u1", RootKind::Project, &slug),
            "/var/lib/nestbox/persistent/u1/project--alpha"
        );
    }

    #[test]
    fn a_mount_path_is_absolute_and_clean() {
        assert_eq!(validate_mount_path("/workspace/a/"), Ok("/workspace/a".into()));
        assert!(validate_mount_path("workspace").is_err());
        assert!(validate_mount_path("/a/../b").is_err());
        assert!(validate_mount_path("/").is_err());
    }

    #[test]
    fn a_slug_rejects_path_characters() {
        assert!(RootSlug::new("a/b").is_err());
        assert!(RootSlug::new("ok-1").is_ok());
    }
}
