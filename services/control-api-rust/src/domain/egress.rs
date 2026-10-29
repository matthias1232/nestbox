//! Egress: the decision the proxy asks for, and the evidence it leaves.
//!
//! Every outbound connection from a sandbox goes through the node's egress
//! proxy, and the proxy asks the control API what to do. The answer is one of
//! three words, and the rule that produces it is this module's `decide`. The
//! function is pure so the proxy's behaviour can be proven in a unit test
//! without a proxy.
//!
//! The order is the rule:
//!
//! 1. an open traffic window for the sandbox or the environment → allow;
//! 2. the first enabled rule, by ascending priority, whose matchers (host,
//!    suffix, CIDR, port range, protocol, method, binary) all match → its
//!    action (or `audit` when the rule is
//!    audit-only);
//! 3. nothing matched → deny.
//!
//! Default deny is the property that makes the policy screen true: a sandbox
//! with no rule and no window reaches nothing.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use super::policy::{
    Enforcement, MatchSubject, NetworkRule, RuleAction, RuleProtocol,
};

/// What the proxy asks: who wants to reach what.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct CheckRequest {
    pub sandbox_id: Option<Uuid>,
    pub node_id: Option<Uuid>,
    pub host: String,
    pub port: i32,
    /// `http`, `https`, `tcp`, `mcp`, `dns`.
    #[serde(default)]
    pub protocol: Option<String>,
    pub method: Option<String>,
    pub path: Option<String>,
    pub user_agent: Option<String>,
    pub mcp_tool: Option<String>,
    pub mcp_session_id: Option<String>,
}

/// The three answers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum Decision {
    Allow,
    Deny,
    /// Let it through and log it: the rule was audit-only.
    Audit,
}

impl Decision {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Allow => "allow",
            Self::Deny => "deny",
            Self::Audit => "audit",
        }
    }

    /// Whether the connection proceeds.
    pub fn passes(self) -> bool {
        !matches!(self, Self::Deny)
    }
}

/// Why the decision was what it was.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum DecisionReason {
    Rule,
    Window,
    DefaultDeny,
    /// The proxy could not reach the control API and failed closed.
    BackendUnavailable,
}

impl DecisionReason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Rule => "rule",
            Self::Window => "window",
            Self::DefaultDeny => "default_deny",
            Self::BackendUnavailable => "backend_unavailable",
        }
    }
}

/// The answer the proxy receives.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct CheckResponse {
    pub decision: Decision,
    pub reason: DecisionReason,
    pub rule_id: Option<Uuid>,
    pub window_id: Option<Uuid>,
    /// A sentence the console shows verbatim.
    pub explanation: String,
    /// The hash of the rule set this decision came from, so the proxy can tell
    /// whether it is enforcing the current policy.
    pub policy_hash: String,
    /// How long the proxy may cache this answer for the same key.
    pub cache_ttl_seconds: u32,
}

/// An open traffic window, as the decision function needs it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct OpenWindow {
    pub id: Uuid,
    pub sandbox_id: Option<Uuid>,
    pub expires_at: DateTime<Utc>,
    pub reason: String,
}

impl OpenWindow {
    /// Whether this window covers the request now.
    pub fn covers(&self, sandbox: Option<Uuid>, now: DateTime<Utc>) -> bool {
        if self.expires_at <= now {
            return false;
        }
        match self.sandbox_id {
            None => true,
            Some(scoped) => sandbox == Some(scoped),
        }
    }
}

/// The rule set as compiled for one environment, in the order the proxy and the
/// decision function apply it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct CompiledPolicy {
    pub environment_id: Uuid,
    pub hash: String,
    pub compiled_at: DateTime<Utc>,
    pub rules: Vec<NetworkRule>,
    pub windows: Vec<OpenWindow>,
}

/// Decide what happens to one request.
pub fn decide(
    request: &CheckRequest,
    policy: &CompiledPolicy,
    now: DateTime<Utc>,
) -> CheckResponse {
    let base = |decision: Decision, reason: DecisionReason, explanation: String| CheckResponse {
        decision,
        reason,
        rule_id: None,
        window_id: None,
        explanation,
        policy_hash: policy.hash.clone(),
        cache_ttl_seconds: 5,
    };

    if let Some(window) = policy
        .windows
        .iter()
        .find(|window| window.covers(request.sandbox_id, now))
    {
        let mut response = base(
            Decision::Allow,
            DecisionReason::Window,
            format!(
                "Allowed by a temporary window ({}), open until {}.",
                window.reason,
                window.expires_at.format("%H:%M:%S UTC")
            ),
        );
        response.window_id = Some(window.id);
        // A window is time-bounded; cache no longer than its remaining life.
        let remaining = (window.expires_at - now).num_seconds().max(0) as u32;
        response.cache_ttl_seconds = remaining.min(5);
        return response;
    }

    let mut candidates: Vec<&NetworkRule> = policy
        .rules
        .iter()
        .filter(|rule| rule.enabled)
        .filter(|rule| rule.sandbox_id.is_none() || rule.sandbox_id == request.sandbox_id)
        .filter(|rule| rule_matches(rule, request))
        .collect();
    candidates.sort_by_key(|rule| rule.priority);

    if let Some(rule) = candidates.first() {
        let (decision, verb) = match (rule.action, rule.enforcement) {
            (RuleAction::Allow, _) => (Decision::Allow, "Allowed"),
            (RuleAction::Deny, Enforcement::Enforced) => (Decision::Deny, "Blocked"),
            (RuleAction::Deny, Enforcement::Audit) => (Decision::Audit, "Would be blocked (audit only)"),
        };
        let mut response = base(
            decision,
            DecisionReason::Rule,
            format!(
                "{verb} by rule '{}' ({} {}{}).",
                if rule.name.is_empty() { "unnamed" } else { &rule.name },
                rule.action.as_str(),
                rule.host,
                rule.port.map(|p| format!(":{p}")).unwrap_or_default()
            ),
        );
        response.rule_id = Some(rule.id);
        return response;
    }

    base(
        Decision::Deny,
        DecisionReason::DefaultDeny,
        format!(
            "No rule allows {}:{}. Add an allow rule or open a temporary window.",
            request.host, request.port
        ),
    )
}

/// Whether one rule applies to one request. Host is a glob; port, protocol, and
/// method are exact when the rule states them and wildcards when it does not.
fn rule_matches(rule: &NetworkRule, request: &CheckRequest) -> bool {
    // The matcher form is authoritative since migration 0032. A rule that
    // arrives without matchers (a legacy caller, an old row) is read through
    // the flat fields it was written from, so both shapes mean the same thing.
    if !rule.matchers.is_empty() {
        let subject = MatchSubject {
            host: &request.host,
            port: request.port,
            protocol: request.protocol.as_deref(),
            method: request.method.as_deref(),
            // The executor is not part of the wire request yet; a binary rule
            // does not match rather than matching on a guess.
            binary: None,
        };
        return rule.matchers.iter().all(|matcher| matcher.matches(&subject));
    }

    if !host_glob_matches(&rule.host, &request.host) {
        return false;
    }
    if let Some(port) = rule.port {
        if port != request.port {
            return false;
        }
    }
    if !protocol_matches(rule.protocol, request.protocol.as_deref()) {
        return false;
    }
    if !rule.methods.is_empty() {
        let Some(method) = request.method.as_deref() else {
            return false;
        };
        if !rule
            .methods
            .iter()
            .any(|allowed| allowed.eq_ignore_ascii_case(method))
        {
            return false;
        }
    }
    true
}

/// A rule's protocol against the request's wire protocol.
///
/// `tcp` is the wildcard; `rest`/`http` cover both http and https; `mcp`
/// matches only what the proxy identified as MCP; `dns` is its own thing.
fn protocol_matches(rule: RuleProtocol, request: Option<&str>) -> bool {
    let request = request.unwrap_or("tcp").to_ascii_lowercase();
    match rule {
        RuleProtocol::Tcp => true,
        RuleProtocol::Rest | RuleProtocol::Http => {
            matches!(request.as_str(), "http" | "https" | "rest" | "mcp" | "jsonrpc" | "graphql")
        }
        RuleProtocol::Websocket => matches!(request.as_str(), "websocket" | "ws" | "wss"),
        RuleProtocol::Graphql => request == "graphql",
        RuleProtocol::Mcp => request == "mcp",
        RuleProtocol::Jsonrpc => matches!(request.as_str(), "jsonrpc" | "mcp"),
    }
}

/// Host globs: `*` matches any run of characters, `*.example.com` matches
/// subdomains but not the apex, and a bare `*` matches everything. Matching is
/// case-insensitive, as hostnames are.
pub fn host_glob_matches(pattern: &str, host: &str) -> bool {
    let pattern = pattern.to_ascii_lowercase();
    let host = host.to_ascii_lowercase();
    if pattern == "*" {
        return true;
    }
    if !pattern.contains('*') {
        return pattern == host;
    }
    glob(&pattern, &host)
}

fn glob(pattern: &str, text: &str) -> bool {
    let p: Vec<char> = pattern.chars().collect();
    let t: Vec<char> = text.chars().collect();
    let (mut pi, mut ti) = (0usize, 0usize);
    let (mut star, mut mark): (Option<usize>, usize) = (None, 0);
    while ti < t.len() {
        if pi < p.len() && (p[pi] == t[ti]) {
            pi += 1;
            ti += 1;
        } else if pi < p.len() && p[pi] == '*' {
            star = Some(pi);
            mark = ti;
            pi += 1;
        } else if let Some(s) = star {
            pi = s + 1;
            mark += 1;
            ti = mark;
        } else {
            return false;
        }
    }
    while pi < p.len() && p[pi] == '*' {
        pi += 1;
    }
    pi == p.len()
}

/// One line of traffic as the console shows it: the proxy's access-log entry
/// after the control API has attached the decision and the MCP facts.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct TrafficEvent {
    pub id: Uuid,
    pub sandbox_id: Option<Uuid>,
    pub node_id: Option<Uuid>,
    pub ts: DateTime<Utc>,
    pub host: String,
    pub port: i32,
    pub protocol: String,
    pub method: Option<String>,
    pub path: Option<String>,
    pub status: Option<i32>,
    pub duration_ms: Option<i64>,
    pub bytes_in: i64,
    pub bytes_out: i64,
    pub decision: Decision,
    pub reason: DecisionReason,
    pub rule_id: Option<Uuid>,
    pub window_id: Option<Uuid>,
    /// Guessed from the user agent; the console says "probably".
    pub agent_hint: Option<String>,
    pub mcp_session_id: Option<String>,
    pub mcp_method: Option<String>,
    pub mcp_tool: Option<String>,
    /// Whether the proxy could read the request (it had the CA) or only saw a
    /// TLS tunnel.
    pub inspected: bool,
    pub has_body: bool,
    /// The path this request took, for the badge and the colour.
    pub channel: TrafficChannel,
    /// The listener that logged it, kept as the proxy wrote it.
    pub listener: Option<String>,
    /// The name the client asked for; the only destination a tunnel reveals.
    pub sni: Option<String>,
    /// What Envoy recognised the application protocol as (`http/1.1`, `h2`).
    pub app_protocol: Option<String>,
    /// ISO 3166-1 alpha-2 from the destination's registry record, when known.
    pub country: Option<String>,
    /// The autonomous system the destination's address belongs to, when the
    /// registry record names it.
    pub asn: Option<i64>,
    /// The program that opened the connection, when the enforcement saw it (an OpenShell
    /// supervisor knows the real executable behind every request).
    #[serde(default)]
    pub binary: Option<String>,
    /// The enforcement's own words for the decision, such as the policy that matched or the
    /// reason it refused, kept as written.
    #[serde(default)]
    pub reason_text: Option<String>,
}

/// The path one outbound request took, as the console colours it.
///
/// One value per row, because a row is one request and a request took one path.
/// The words are the proxy's (`listener`, `connect`, `sni`), translated once
/// here so the table, the filter, and the badge cannot disagree.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum TrafficChannel {
    /// A plain forward-proxy request: the method and the path are visible.
    Proxy,
    /// A CONNECT the proxy did not terminate: only the SNI is visible.
    Tunnel,
    /// A CONNECT the proxy terminated and decrypted.
    Inspected,
    /// A connection into the sandbox, not out of it.
    Inbound,
    /// The caller arrived over a tunnel interface (WireGuard, `tun0`).
    Vpn,
}

impl TrafficChannel {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Proxy => "proxy",
            Self::Tunnel => "tunnel",
            Self::Inspected => "inspected",
            Self::Inbound => "inbound",
            Self::Vpn => "vpn",
        }
    }

    /// The word the console shows on the badge.
    pub fn label(self) -> &'static str {
        match self {
            Self::Proxy => "Proxy",
            Self::Tunnel => "Tunnel",
            Self::Inspected => "Inspected",
            Self::Inbound => "Inbound",
            Self::Vpn => "VPN",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "proxy" => Self::Proxy,
            "tunnel" => Self::Tunnel,
            "inspected" => Self::Inspected,
            "inbound" => Self::Inbound,
            "vpn" => Self::Vpn,
            _ => return None,
        })
    }

    /// All channels, in the order the legend shows them.
    pub fn all() -> [Self; 5] {
        [
            Self::Proxy,
            Self::Tunnel,
            Self::Inspected,
            Self::Inbound,
            Self::Vpn,
        ]
    }
}

/// What the proxy's access log says about the path a request took.
///
/// Kept as one struct so the derivation is a pure function of what was observed,
/// and a test can state every combination without a proxy.
#[derive(Debug, Clone, Copy, Default)]
pub struct ChannelFacts<'a> {
    /// The listener that logged the line: `proxy`, `terminated`, `inbound`, `vpn`.
    pub listener: Option<&'a str>,
    /// The line is a CONNECT, not a plain request.
    pub connect: bool,
    /// The name the client asked for. Present on a CONNECT and on a terminated
    /// request; absent from a plain HTTP/1.1 request, which carries a Host header.
    pub sni: Option<&'a str>,
    /// The caller as the proxy saw it: an address, or the interface name.
    pub client: Option<&'a str>,
}

/// Which channel a request took.
///
/// The order is the rule: a connection into the sandbox is inbound whatever else
/// it looks like; a tunnel caller is a VPN caller even when the request behind it
/// was decrypted; a terminated CONNECT is inspected; a CONNECT that was not
/// terminated is a tunnel; everything else is a plain proxy request.
pub fn channel_of(facts: &ChannelFacts<'_>) -> TrafficChannel {
    let listener = facts.listener.unwrap_or_default().trim().to_ascii_lowercase();
    if listener == "inbound" || listener == "ingress" {
        return TrafficChannel::Inbound;
    }
    if listener == "vpn" || client_is_tunnel(facts.client) {
        return TrafficChannel::Vpn;
    }
    if listener == "terminated" || listener == "inspected" {
        return TrafficChannel::Inspected;
    }
    if facts.connect {
        return TrafficChannel::Tunnel;
    }
    // A request with an SNI but no CONNECT is a TLS request the proxy accepted
    // directly; it did not have to be decrypted, because it never got a tunnel.
    TrafficChannel::Proxy
}

/// Whether the caller address names a tunnel interface.
///
/// The proxy reports an address, and the tunnel interfaces on a node are named
/// `wg0`, `tun0`, `vpn0`. Only those prefixes are accepted: guessing "not a
/// private address" would label a LAN caller a VPN caller.
fn client_is_tunnel(client: Option<&str>) -> bool {
    let Some(client) = client else {
        return false;
    };
    let name = client.trim().to_ascii_lowercase();
    ["wg", "tun", "vpn", "tailscale"]
        .iter()
        .any(|prefix| name.starts_with(prefix))
}

/// An edge in the live map: one sandbox to one destination over a window.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct TrafficEdge {
    pub sandbox_id: Uuid,
    pub host: String,
    pub port: i32,
    pub protocol: String,
    pub requests: i64,
    pub allowed: i64,
    pub denied: i64,
    pub bytes: i64,
    pub last_seen: DateTime<Utc>,
    /// Tool names seen on this edge when it is MCP.
    pub tools: Vec<String>,
}

/// Guess the agent from a user agent string. Returns `None` rather than a
/// wrong name; the console labels any value "probably".
pub fn agent_hint(user_agent: Option<&str>) -> Option<String> {
    let ua = user_agent?.to_ascii_lowercase();
    const KNOWN: &[(&str, &str)] = &[
        ("hermes", "hermes"),
        ("claude-code", "claude-code"),
        ("claude", "claude"),
        ("opencode", "opencode"),
        ("openchamber", "openchamber"),
        ("openclaw", "openclaw"),
        ("antigravity", "antigravity"),
        ("mcp", "mcp-client"),
        ("python-requests", "python"),
        ("python-httpx", "python"),
        ("aiohttp", "python"),
        ("node", "node"),
        ("curl", "curl"),
        ("wget", "wget"),
    ];
    KNOWN
        .iter()
        .find(|(needle, _)| ua.contains(needle))
        .map(|(_, name)| (*name).to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    fn rule(host: &str, port: Option<i32>, action: RuleAction, priority: i32) -> NetworkRule {
        NetworkRule {
            id: Uuid::new_v4(),
            environment_id: Uuid::nil(),
            policy_id: None,
            sandbox_id: None,
            name: format!("{} {host}", action.as_str()),
            action,
            host: host.into(),
            port,
            protocol: RuleProtocol::Tcp,
            methods: vec![],
            binaries: vec![],
            matchers: vec![],
            enforcement: Enforcement::Enforced,
            priority,
            enabled: true,
            version: 1,
        }
    }

    fn request(host: &str, port: i32) -> CheckRequest {
        CheckRequest {
            sandbox_id: Some(Uuid::nil()),
            node_id: None,
            host: host.into(),
            port,
            protocol: Some("https".into()),
            method: Some("GET".into()),
            path: Some("/".into()),
            user_agent: None,
            mcp_tool: None,
            mcp_session_id: None,
        }
    }

    fn policy(rules: Vec<NetworkRule>, windows: Vec<OpenWindow>) -> CompiledPolicy {
        CompiledPolicy {
            environment_id: Uuid::nil(),
            hash: "h".into(),
            compiled_at: Utc::now(),
            rules,
            windows,
        }
    }

    #[test]
    fn nothing_matching_is_denied_by_default() {
        let out = decide(&request("example.com", 443), &policy(vec![], vec![]), Utc::now());
        assert_eq!(out.decision, Decision::Deny);
        assert_eq!(out.reason, DecisionReason::DefaultDeny);
        assert!(out.explanation.contains("example.com:443"));
    }

    #[test]
    fn an_allow_rule_allows_and_names_itself() {
        let r = rule("example.com", Some(443), RuleAction::Allow, 10);
        let id = r.id;
        let out = decide(&request("example.com", 443), &policy(vec![r], vec![]), Utc::now());
        assert_eq!(out.decision, Decision::Allow);
        assert_eq!(out.rule_id, Some(id));
    }

    #[test]
    fn lower_priority_number_wins() {
        let deny = rule("*.example.com", None, RuleAction::Deny, 5);
        let allow = rule("api.example.com", None, RuleAction::Allow, 50);
        let out = decide(&request("api.example.com", 443), &policy(vec![allow, deny], vec![]), Utc::now());
        assert_eq!(out.decision, Decision::Deny);
    }

    #[test]
    fn an_audit_deny_passes_but_is_marked() {
        let mut r = rule("example.com", None, RuleAction::Deny, 1);
        r.enforcement = Enforcement::Audit;
        let out = decide(&request("example.com", 443), &policy(vec![r], vec![]), Utc::now());
        assert_eq!(out.decision, Decision::Audit);
        assert!(out.decision.passes());
    }

    #[test]
    fn a_window_beats_every_rule_until_it_expires() {
        let deny = rule("*", None, RuleAction::Deny, 1);
        let now = Utc::now();
        let window = OpenWindow {
            id: Uuid::new_v4(),
            sandbox_id: Some(Uuid::nil()),
            expires_at: now + Duration::minutes(10),
            reason: "debugging".into(),
        };
        let out = decide(&request("anything.net", 80), &policy(vec![deny.clone()], vec![window.clone()]), now);
        assert_eq!(out.decision, Decision::Allow);
        assert_eq!(out.reason, DecisionReason::Window);

        let later = decide(&request("anything.net", 80), &policy(vec![deny], vec![window]), now + Duration::minutes(11));
        assert_eq!(later.decision, Decision::Deny);
    }

    #[test]
    fn a_sandbox_scoped_window_does_not_cover_another_sandbox() {
        let now = Utc::now();
        let window = OpenWindow {
            id: Uuid::new_v4(),
            sandbox_id: Some(Uuid::new_v4()),
            expires_at: now + Duration::minutes(10),
            reason: "x".into(),
        };
        let out = decide(&request("a.b", 1), &policy(vec![], vec![window]), now);
        assert_eq!(out.decision, Decision::Deny);
    }

    #[test]
    fn a_sandbox_scoped_rule_applies_only_to_that_sandbox() {
        let other = Uuid::new_v4();
        let mut r = rule("example.com", None, RuleAction::Allow, 1);
        r.sandbox_id = Some(other);
        let out = decide(&request("example.com", 443), &policy(vec![r], vec![]), Utc::now());
        assert_eq!(out.decision, Decision::Deny);
    }

    #[test]
    fn a_method_list_restricts_the_rule() {
        let mut r = rule("example.com", None, RuleAction::Allow, 1);
        r.methods = vec!["GET".into()];
        let mut post = request("example.com", 443);
        post.method = Some("POST".into());
        assert_eq!(decide(&post, &policy(vec![r.clone()], vec![]), Utc::now()).decision, Decision::Deny);
        assert_eq!(decide(&request("example.com", 443), &policy(vec![r], vec![]), Utc::now()).decision, Decision::Allow);
    }

    #[test]
    fn an_mcp_rule_matches_only_identified_mcp_traffic() {
        let mut r = rule("mcp.example.com", None, RuleAction::Allow, 1);
        r.protocol = RuleProtocol::Mcp;
        let https = request("mcp.example.com", 443);
        assert_eq!(decide(&https, &policy(vec![r.clone()], vec![]), Utc::now()).decision, Decision::Deny);
        let mut mcp = https.clone();
        mcp.protocol = Some("mcp".into());
        assert_eq!(decide(&mcp, &policy(vec![r], vec![]), Utc::now()).decision, Decision::Allow);
    }

    #[test]
    fn host_globs_behave_like_dns_operators_expect() {
        assert!(host_glob_matches("*", "anything"));
        assert!(host_glob_matches("example.com", "EXAMPLE.com"));
        assert!(host_glob_matches("*.example.com", "api.example.com"));
        assert!(!host_glob_matches("*.example.com", "example.com"));
        assert!(host_glob_matches("api.*.com", "api.example.com"));
        assert!(!host_glob_matches("example.com", "notexample.com"));
    }

    #[test]
    fn the_agent_hint_is_conservative() {
        assert_eq!(agent_hint(Some("hermes-agent/0.22.5")).as_deref(), Some("hermes"));
        assert_eq!(agent_hint(Some("claude-code/1.0")).as_deref(), Some("claude-code"));
        assert_eq!(agent_hint(Some("Mozilla/5.0")), None);
        assert_eq!(agent_hint(None), None);
    }
}
