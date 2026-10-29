//! Nestbox network rules, compiled into an OpenShell sandbox policy.
//!
//! OpenShell denies every connection that no `network_policies` entry allows, so
//! the compiler only has to write the *allows*. A Nestbox deny rule is honoured
//! twice over: it never becomes an entry, and it removes an allow for the same
//! destination that sorts behind it. What a rule cannot express in OpenShell is
//! reported in [`Compiled::skipped`] instead of being dropped silently.
//!
//! The result is a plain `serde_json::Value` (JSON is YAML), so the runner can write
//! it straight to a file for `openshell policy set`.

use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};

use super::policy::{Enforcement, MatcherKind, NetworkRule, RuleAction, RuleProtocol};

/// The workload's filesystem baseline: what a sandbox may read and write.
const READ_ONLY: &[&str] = &["/bin", "/usr", "/lib", "/lib64", "/etc", "/var/log", "/opt"];
const READ_WRITE: &[&str] = &["/tmp", "/home", "/var/tmp", "/workspace", "/data"];

#[derive(Debug, Clone, PartialEq)]
pub struct Compiled {
    pub document: Value,
    /// Rules that could not be expressed, with the reason.
    pub skipped: Vec<(String, String)>,
}

impl Compiled {
    /// A stable fingerprint, so the sync only pushes when something changed.
    pub fn fingerprint(&self) -> String {
        let text = serde_json::to_string(&self.document).unwrap_or_default();
        let digest = Sha256::digest(text.as_bytes());
        digest.iter().map(|byte| format!("{byte:02x}")).collect()
    }
}

/// Compile the effective rules of one sandbox.
pub fn compile(rules: &[NetworkRule]) -> Compiled {
    let mut skipped = Vec::new();
    let mut entries = Map::new();
    let denies: Vec<&NetworkRule> = rules
        .iter()
        .filter(|rule| rule.enabled && rule.action == RuleAction::Deny)
        .collect();

    for (index, rule) in rules.iter().enumerate() {
        if !rule.enabled || rule.action != RuleAction::Allow {
            continue;
        }
        if is_denied(rule, &denies) {
            skipped.push((rule.name.clone(), "a deny rule for the same destination comes first".into()));
            continue;
        }
        match endpoint_for(rule) {
            Ok(endpoint) => {
                let mut key = key_for(&rule.name, index);
                while entries.contains_key(&key) {
                    key.push('x');
                }
                entries.insert(key, json!({ "name": rule.name, "endpoints": [endpoint], "binaries": binaries_for(rule) }));
            }
            Err(reason) => skipped.push((rule.name.clone(), reason)),
        }
    }

    let document = json!({
        "version": 1,
        "filesystem_policy": { "include_workdir": true, "read_only": READ_ONLY, "read_write": READ_WRITE },
        "landlock": { "compatibility": "best_effort" },
        "network_policies": Value::Object(entries),
    });
    Compiled { document, skipped }
}

fn is_denied(allow: &NetworkRule, denies: &[&NetworkRule]) -> bool {
    denies.iter().any(|deny| {
        deny.priority <= allow.priority
            && !deny.host.is_empty()
            && deny.host != "*"
            && deny.host == allow.host
            && (deny.port.is_none() || deny.port == allow.port)
    })
}

fn key_for(name: &str, index: usize) -> String {
    let slug: String = name
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    let slug = slug.trim_matches('_');
    let slug: String = slug.chars().take(48).collect();
    format!("nb{index}_{slug}")
}

fn binaries_for(rule: &NetworkRule) -> Vec<Value> {
    if rule.binaries.is_empty() {
        // "Any program": OpenShell matches nothing for an empty list.
        return vec![json!({ "path": "/**" })];
    }
    rule.binaries.iter().map(|path| json!({ "path": path })).collect()
}

fn endpoint_for(rule: &NetworkRule) -> Result<Value, String> {
    let mut host = rule.host.trim().to_string();
    let mut cidr: Option<String> = None;
    let mut port = rule.port;
    let mut port_range: Option<(u16, u16)> = None;
    for matcher in &rule.matchers {
        let value = matcher.value.clone().unwrap_or_default();
        match matcher.kind {
            MatcherKind::Host if host.is_empty() || host == "*" => host = value,
            MatcherKind::HostSuffix => host = format!("*.{}", value.trim_start_matches('.')),
            MatcherKind::Cidr => cidr = Some(value),
            MatcherKind::Port if port.is_none() => {
                if let Some((low, high)) = value.split_once('-') {
                    if let (Ok(low), Ok(high)) = (low.parse::<u16>(), high.parse::<u16>()) {
                        port_range = Some((low, high));
                    }
                } else {
                    port = value.parse().ok();
                }
            }
            _ => {}
        }
    }
    let mut endpoint = Map::new();
    if host.is_empty() || host == "*" {
        let Some(cidr) = cidr else {
            return Err("no host or address range to allow".into());
        };
        endpoint.insert("allowed_ips".into(), json!([cidr]));
    } else {
        if host.starts_with("*.") && host.matches('.').count() < 2 {
            return Err(format!("wildcard '{host}' has fewer than three labels"));
        }
        endpoint.insert("host".into(), json!(host));
    }
    match (port, port_range) {
        (Some(port), _) => {
            endpoint.insert("port".into(), json!(port));
        }
        (None, Some((low, high))) if high >= low && high - low < 64 => {
            endpoint.insert("ports".into(), json!((low..=high).collect::<Vec<_>>()));
        }
        (None, Some(_)) => return Err("port range wider than 64 ports".into()),
        (None, None) => {
            endpoint.insert("port".into(), json!(443));
        }
    }
    let enforce = if rule.enforcement == Enforcement::Enforced { "enforce" } else { "audit" };
    match rule.protocol {
        RuleProtocol::Rest | RuleProtocol::Http => {
            endpoint.insert("protocol".into(), json!("rest"));
            endpoint.insert("enforcement".into(), json!(enforce));
            if rule.methods.is_empty() {
                endpoint.insert("access".into(), json!("full"));
            } else {
                let allowed: Vec<Value> = rule
                    .methods
                    .iter()
                    .map(|method| json!({ "allow": { "method": method.to_uppercase(), "path": "/**" } }))
                    .collect();
                endpoint.insert("rules".into(), Value::Array(allowed));
            }
        }
        RuleProtocol::Websocket => {
            endpoint.insert("protocol".into(), json!("websocket"));
            endpoint.insert("enforcement".into(), json!(enforce));
            endpoint.insert("access".into(), json!("full"));
        }
        RuleProtocol::Graphql => {
            endpoint.insert("protocol".into(), json!("graphql"));
            endpoint.insert("enforcement".into(), json!(enforce));
            endpoint.insert("access".into(), json!("full"));
        }
        RuleProtocol::Mcp => {
            endpoint.insert("protocol".into(), json!("mcp"));
            endpoint.insert("enforcement".into(), json!(enforce));
            endpoint.insert("mcp".into(), json!({ "allow_all_known_mcp_methods": true }));
        }
        RuleProtocol::Tcp => {
            if endpoint.contains_key("allowed_ips") {
                return Err("a native TCP rule needs a hostname".into());
            }
            endpoint.insert("protocol".into(), json!("tcp"));
        }
        // JSON-RPC needs explicit method rules, which a flat rule does not carry:
        // fall back to a plain endpoint rather than refuse the destination.
        RuleProtocol::Jsonrpc => {}
    }
    Ok(Value::Object(endpoint))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::policy::Matcher;
    use uuid::Uuid;

    fn rule(name: &str, action: RuleAction, host: &str, port: Option<i32>, protocol: RuleProtocol) -> NetworkRule {
        NetworkRule {
            id: Uuid::nil(),
            environment_id: Uuid::nil(),
            policy_id: None,
            sandbox_id: None,
            name: name.into(),
            action,
            host: host.into(),
            port,
            protocol,
            methods: vec![],
            binaries: vec![],
            matchers: vec![],
            enforcement: Enforcement::Enforced,
            priority: 100,
            enabled: true,
            version: 1,
        }
    }

    fn entries(compiled: &Compiled) -> &Map<String, Value> {
        compiled.document["network_policies"].as_object().unwrap()
    }

    #[test]
    fn an_empty_rule_set_is_a_policy_that_allows_nothing() {
        let compiled = compile(&[]);
        assert!(entries(&compiled).is_empty());
        assert_eq!(compiled.document["version"], 1);
    }

    #[test]
    fn an_allow_becomes_an_endpoint_for_any_program() {
        let compiled = compile(&[rule("Model", RuleAction::Allow, "10.90.92.30", Some(20128), RuleProtocol::Http)]);
        let entry = entries(&compiled).values().next().unwrap();
        assert_eq!(entry["endpoints"][0]["host"], "10.90.92.30");
        assert_eq!(entry["endpoints"][0]["port"], 20128);
        assert_eq!(entry["endpoints"][0]["protocol"], "rest");
        assert_eq!(entry["endpoints"][0]["access"], "full");
        assert_eq!(entry["binaries"][0]["path"], "/**");
    }

    #[test]
    fn methods_become_request_rules_and_binaries_are_kept() {
        let mut r = rule("Read", RuleAction::Allow, "api.example.com", Some(443), RuleProtocol::Rest);
        r.methods = vec!["get".into(), "head".into()];
        r.binaries = vec!["/usr/bin/curl".into()];
        let compiled = compile(&[r]);
        let entry = entries(&compiled).values().next().unwrap();
        assert_eq!(entry["endpoints"][0]["rules"][0]["allow"]["method"], "GET");
        assert_eq!(entry["endpoints"][0]["rules"][1]["allow"]["method"], "HEAD");
        assert_eq!(entry["binaries"][0]["path"], "/usr/bin/curl");
    }

    #[test]
    fn a_deny_is_never_an_entry_and_removes_a_matching_allow() {
        let mut deny = rule("Block", RuleAction::Deny, "bad.example.com", None, RuleProtocol::Tcp);
        deny.priority = 50;
        let allow = rule("Allow", RuleAction::Allow, "bad.example.com", Some(443), RuleProtocol::Rest);
        let other = rule("Other", RuleAction::Allow, "ok.example.com", Some(443), RuleProtocol::Rest);
        let compiled = compile(&[deny, allow, other]);
        assert_eq!(entries(&compiled).len(), 1);
        assert_eq!(compiled.skipped.len(), 1);
    }

    #[test]
    fn a_host_suffix_becomes_a_wildcard_and_a_short_one_is_reported() {
        let mut ok = rule("Suffix", RuleAction::Allow, "", Some(443), RuleProtocol::Rest);
        ok.matchers = vec![Matcher { kind: MatcherKind::HostSuffix, value: Some(".example.com".into()) }];
        let mut short = rule("Short", RuleAction::Allow, "", Some(443), RuleProtocol::Rest);
        short.matchers = vec![Matcher { kind: MatcherKind::HostSuffix, value: Some("com".into()) }];
        let compiled = compile(&[ok, short]);
        assert_eq!(entries(&compiled).values().next().unwrap()["endpoints"][0]["host"], "*.example.com");
        assert_eq!(compiled.skipped.len(), 1);
    }

    #[test]
    fn a_disabled_rule_and_a_rule_without_destination_are_not_written() {
        let mut off = rule("Off", RuleAction::Allow, "a.example.com", Some(443), RuleProtocol::Rest);
        off.enabled = false;
        let nothing = rule("Nothing", RuleAction::Allow, "", None, RuleProtocol::Rest);
        let compiled = compile(&[off, nothing]);
        assert!(entries(&compiled).is_empty());
        assert_eq!(compiled.skipped.len(), 1);
    }

    #[test]
    fn the_fingerprint_changes_with_the_rules_and_not_otherwise() {
        let a = compile(&[rule("A", RuleAction::Allow, "a.example.com", Some(443), RuleProtocol::Rest)]);
        let a_again = compile(&[rule("A", RuleAction::Allow, "a.example.com", Some(443), RuleProtocol::Rest)]);
        let b = compile(&[rule("A", RuleAction::Allow, "b.example.com", Some(443), RuleProtocol::Rest)]);
        assert_eq!(a.fingerprint(), a_again.fingerprint());
        assert_ne!(a.fingerprint(), b.fingerprint());
    }
}
