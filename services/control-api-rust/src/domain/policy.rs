//! Policy documents, scopes, sections, and network rules.
//!
//! Two facts drive this model (ADR 0007, ADR 0008):
//!
//! 1. The runtime has exactly two scopes: a sandbox policy, and one global
//!    override that replaces every sandbox policy. There is no third scope, so
//!    adding one to this type would create a state the runtime cannot enforce.
//! 2. Only the network section is hot-reloadable. The other three are fixed when
//!    the workload starts, and the API says so rather than letting a save look
//!    like a live change.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

/// Which sandboxes a policy applies to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum PolicyScope {
    /// Applies where assigned via `sandbox_policies`.
    Sandbox,
    /// Replaces every sandbox policy and blocks sandbox-level edits until
    /// deleted.
    Global,
}

impl PolicyScope {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Sandbox => "sandbox",
            Self::Global => "global",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "sandbox" => Some(Self::Sandbox),
            "global" => Some(Self::Global),
            _ => None,
        }
    }
}

/// The four sections of a policy document.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum PolicySection {
    FilesystemPolicy,
    Landlock,
    Process,
    NetworkPolicies,
}

impl PolicySection {
    /// The key in the document.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::FilesystemPolicy => "filesystem_policy",
            Self::Landlock => "landlock",
            Self::Process => "process",
            Self::NetworkPolicies => "network_policies",
        }
    }

    /// Whether a change to this section takes effect on a running sandbox.
    ///
    /// This is the single source of truth for the warning the console shows and
    /// for the outcome the API reports. Encoding it here means the answer cannot
    /// differ between the editor and the save.
    pub fn is_hot_reloadable(self) -> bool {
        matches!(self, Self::NetworkPolicies)
    }

    /// All sections, in document order.
    pub fn all() -> [Self; 4] {
        [
            Self::FilesystemPolicy,
            Self::Landlock,
            Self::Process,
            Self::NetworkPolicies,
        ]
    }
}

/// What a save will actually do.
///
/// Three outcomes, because there are three real cases. Collapsing them to a
/// boolean is what makes a UI claim a change is live when it is not.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ApplyOutcome {
    /// The change is loaded by running sandboxes immediately.
    AppliesNow,
    /// Saved, but only a recreation picks it up.
    RequiresRecreate,
    /// Refused; the runtime rejects the change after the workload started.
    Rejected,
}

/// A policy as stored and returned.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Policy {
    pub id: Uuid,
    pub environment_id: Uuid,
    pub name: String,
    pub scope: PolicyScope,
    /// The complete document. This is what the gateway receives and what diff
    /// and rollback operate on.
    pub spec: serde_json::Value,
    pub version: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// A normalised network rule: the queryable projection of the document.
///
/// The document is authoritative. This table exists so a list or a bulk edit
/// does not have to parse JSON on every request. `policy::rules_from_document`
/// and `policy::document_from_rules` keep the two aligned, and a test asserts
/// the round trip.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct NetworkRule {
    pub id: Uuid,
    pub environment_id: Uuid,
    pub policy_id: Option<Uuid>,
    pub sandbox_id: Option<Uuid>,
    pub name: String,
    pub action: RuleAction,
    pub host: String,
    pub port: Option<i32>,
    pub protocol: RuleProtocol,
    pub methods: Vec<String>,
    /// The allowlist of executables. A script calling `curl` is not `curl`.
    pub binaries: Vec<String>,
    pub enforcement: Enforcement,
    pub priority: i32,
    pub enabled: bool,
    pub version: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum RuleAction {
    Allow,
    Deny,
}

impl RuleAction {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Allow => "allow",
            Self::Deny => "deny",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "allow" => Some(Self::Allow),
            "deny" => Some(Self::Deny),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum RuleProtocol {
    Rest,
    Http,
    Websocket,
    Graphql,
    Mcp,
    Jsonrpc,
    Tcp,
}

impl RuleProtocol {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Rest => "rest",
            Self::Http => "http",
            Self::Websocket => "websocket",
            Self::Graphql => "graphql",
            Self::Mcp => "mcp",
            Self::Jsonrpc => "jsonrpc",
            Self::Tcp => "tcp",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "rest" => Self::Rest,
            "http" => Self::Http,
            "websocket" => Self::Websocket,
            "graphql" => Self::Graphql,
            "mcp" => Self::Mcp,
            "jsonrpc" => Self::Jsonrpc,
            "tcp" => Self::Tcp,
            _ => return None,
        })
    }
}

/// Whether a rule blocks or merely records.
///
/// `Audit` is how a policy is developed without breaking the workload: the
/// attempt is logged and allowed. The console shows it as a warning, not a wall,
/// because an audit-mode block is not a block.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum Enforcement {
    Enforced,
    Audit,
}

impl Enforcement {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Enforced => "enforced",
            Self::Audit => "audit",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "enforced" => Some(Self::Enforced),
            "audit" => Some(Self::Audit),
            _ => None,
        }
    }

    /// Whether this rule actually prevents traffic.
    pub fn blocks(self) -> bool {
        matches!(self, Self::Enforced)
    }
}

/// Validate a policy document and report which sections it touches.
///
/// Returns the sections present so the caller can decide whether a save is a
/// live change or needs a recreation. A document with no network section is
/// valid but touches nothing hot-reloadable.
pub fn validate_document(spec: &serde_json::Value) -> Result<Vec<PolicySection>, PolicyError> {
    let object = spec
        .as_object()
        .ok_or_else(|| PolicyError::NotAnObject)?;

    let mut sections = Vec::new();
    for section in PolicySection::all() {
        if object.contains_key(section.as_str()) {
            sections.push(section);
        }
    }

    if sections.is_empty() {
        return Err(PolicyError::Empty);
    }

    // The network section is the only one with a shape this layer can check
    // meaningfully without duplicating the runtime's schema. It must be an
    // object of named policies.
    if let Some(network) = object.get("network_policies") {
        if !network.is_object() {
            return Err(PolicyError::NetworkNotAnObject);
        }
    }

    Ok(sections)
}

/// Decide what a save will do, given the sections it changes and whether the
/// target sandboxes are already running.
///
/// This is the function that makes ADR 0008 true in code rather than in prose.
pub fn apply_outcome(changed: &[PolicySection], targets_running: bool) -> ApplyOutcome {
    if !targets_running {
        // Nothing is running, so the startup sections are still settable.
        return ApplyOutcome::AppliesNow;
    }
    if changed.iter().all(|s| s.is_hot_reloadable()) {
        ApplyOutcome::AppliesNow
    } else {
        // A startup section on a running sandbox cannot be applied. The runtime
        // rejects it, so the API reports "recreate" rather than pretending.
        ApplyOutcome::RequiresRecreate
    }
}

/// Errors a policy document can produce.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PolicyError {
    #[error("policy document must be a JSON object")]
    NotAnObject,
    #[error("policy document has no recognised section")]
    Empty,
    #[error("network_policies must be an object of named policies")]
    NetworkNotAnObject,
}

impl PolicyError {
    /// A stable code for the API envelope. Clients branch on this, never on the
    /// human-readable message (ADR 0015).
    pub fn code(&self) -> &'static str {
        match self {
            Self::NotAnObject => "policy.not_an_object",
            Self::Empty => "policy.empty",
            Self::NetworkNotAnObject => "policy.network_not_an_object",
        }
    }
}

/// Project the `network_policies` section into flat rules.
///
/// The document is authoritative; this is a read-side projection. Keeping the
/// direction one-way means the projection can be rebuilt at any time and can
/// never disagree with the document it came from.
pub fn rules_from_document(spec: &serde_json::Value) -> Vec<NetworkRuleSeed> {
    let mut out = Vec::new();
    let Some(policies) = spec.get("network_policies").and_then(|v| v.as_object()) else {
        return out;
    };

    for (rule_name, policy) in policies {
        let Some(endpoints) = policy.get("endpoints").and_then(|v| v.as_array()) else {
            continue;
        };
        for endpoint in endpoints {
            let host = endpoint
                .get("host")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            if host.is_empty() {
                continue;
            }
            let port = endpoint
                .get("port")
                .and_then(|v| v.as_i64())
                .and_then(|p| i32::try_from(p).ok());
            let protocol = endpoint
                .get("protocol")
                .and_then(|v| v.as_str())
                .and_then(RuleProtocol::parse)
                .unwrap_or(RuleProtocol::Rest);
            let action = match endpoint.get("access").and_then(|v| v.as_str()) {
                Some("deny") => RuleAction::Deny,
                _ => RuleAction::Allow,
            };
            let enforcement = endpoint
                .get("enforcement")
                .and_then(|v| v.as_str())
                .and_then(Enforcement::parse)
                .unwrap_or(Enforcement::Enforced);
            let methods = endpoint
                .get("methods")
                .and_then(|v| v.as_array())
                .map(|a| {
                    a.iter()
                        .filter_map(|m| m.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default();
            let binaries = endpoint
                .get("binaries")
                .and_then(|v| v.as_array())
                .map(|a| {
                    a.iter()
                        .filter_map(|b| b.get("path").and_then(|p| p.as_str()))
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_default();

            out.push(NetworkRuleSeed {
                name: rule_name.clone(),
                action,
                host,
                port,
                protocol,
                methods,
                binaries,
                enforcement,
            });
        }
    }
    out
}

/// A rule extracted from a document, before it has an identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetworkRuleSeed {
    pub name: String,
    pub action: RuleAction,
    pub host: String,
    pub port: Option<i32>,
    pub protocol: RuleProtocol,
    pub methods: Vec<String>,
    pub binaries: Vec<String>,
    pub enforcement: Enforcement,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn network_is_the_only_hot_reloadable_section() {
        assert!(PolicySection::NetworkPolicies.is_hot_reloadable());
        assert!(!PolicySection::FilesystemPolicy.is_hot_reloadable());
        assert!(!PolicySection::Landlock.is_hot_reloadable());
        assert!(!PolicySection::Process.is_hot_reloadable());
    }

    #[test]
    fn a_network_only_change_on_a_running_sandbox_applies_now() {
        assert_eq!(
            apply_outcome(&[PolicySection::NetworkPolicies], true),
            ApplyOutcome::AppliesNow
        );
    }

    #[test]
    fn a_startup_section_change_on_a_running_sandbox_requires_recreate() {
        assert_eq!(
            apply_outcome(&[PolicySection::FilesystemPolicy], true),
            ApplyOutcome::RequiresRecreate
        );
        assert_eq!(
            apply_outcome(
                &[PolicySection::NetworkPolicies, PolicySection::Process],
                true
            ),
            ApplyOutcome::RequiresRecreate
        );
    }

    #[test]
    fn nothing_running_means_everything_applies() {
        assert_eq!(
            apply_outcome(&[PolicySection::Landlock], false),
            ApplyOutcome::AppliesNow
        );
    }

    #[test]
    fn validation_rejects_a_non_object() {
        assert_eq!(
            validate_document(&json!("nope")),
            Err(PolicyError::NotAnObject)
        );
        assert_eq!(validate_document(&json!({})), Err(PolicyError::Empty));
    }

    #[test]
    fn validation_reports_the_sections_present() {
        let doc = json!({
            "process": {"run_as_user": "sandbox"},
            "network_policies": {}
        });
        let sections = validate_document(&doc).expect("valid");
        assert_eq!(
            sections,
            vec![PolicySection::Process, PolicySection::NetworkPolicies]
        );
    }

    #[test]
    fn projection_flattens_endpoints_into_rules() {
        let doc = json!({
            "network_policies": {
                "model-api": {
                    "endpoints": [{
                        "host": "api.example.com",
                        "port": 443,
                        "protocol": "rest",
                        "enforcement": "enforced",
                        "access": "allow",
                        "methods": ["GET", "POST"],
                        "binaries": [{"path": "/usr/local/bin/agent"}]
                    }]
                }
            }
        });
        let rules = rules_from_document(&doc);
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].host, "api.example.com");
        assert_eq!(rules[0].port, Some(443));
        assert_eq!(rules[0].action, RuleAction::Allow);
        assert_eq!(rules[0].methods, vec!["GET", "POST"]);
        assert_eq!(rules[0].binaries, vec!["/usr/local/bin/agent"]);
    }

    #[test]
    fn projection_skips_an_endpoint_without_a_host() {
        let doc = json!({"network_policies": {"x": {"endpoints": [{"port": 1}]}}});
        assert!(rules_from_document(&doc).is_empty());
    }

    #[test]
    fn audit_enforcement_does_not_block() {
        assert!(!Enforcement::Audit.blocks());
        assert!(Enforcement::Enforced.blocks());
    }
}
