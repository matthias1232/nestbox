//! MCP servers a sandbox can talk to, and what the proxy saw them do.
//!
//! Transport is the fact that matters for visibility: an `http` or `sse`
//! server is reached through the egress proxy and every call shows up with
//! its tool name; a `stdio` server is a child process inside the container and
//! is, by definition, not egress. The console says which is which rather than
//! showing an empty column.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum McpTransport {
    Stdio,
    Http,
    Sse,
}

impl McpTransport {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Stdio => "stdio",
            Self::Http => "http",
            Self::Sse => "sse",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "stdio" => Self::Stdio,
            "http" => Self::Http,
            "sse" => Self::Sse,
            _ => return None,
        })
    }

    /// Whether calls to this server cross the proxy and are visible as traffic.
    pub fn visible_to_proxy(self) -> bool {
        !matches!(self, Self::Stdio)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum McpAuth {
    None,
    Token,
    Oauth,
}

impl McpAuth {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Token => "token",
            Self::Oauth => "oauth",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "none" => Self::None,
            "token" => Self::Token,
            "oauth" => Self::Oauth,
            _ => return None,
        })
    }
}

/// A server definition from the registry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct McpServer {
    pub key: String,
    pub name: String,
    pub description: Option<String>,
    pub publisher: Option<String>,
    pub homepage: Option<String>,
    pub transport: McpTransport,
    pub endpoint: Option<String>,
    pub command: Option<String>,
    pub args: Vec<String>,
    pub auth_type: McpAuth,
    /// Environment variable names the server needs. Names only.
    pub env_slots: Vec<String>,
    pub packs: Vec<String>,
    pub enabled: bool,
}

impl McpServer {
    /// The host the proxy will see, for an `http`/`sse` server.
    pub fn host(&self) -> Option<String> {
        let endpoint = self.endpoint.as_deref()?;
        let rest = endpoint.split("://").nth(1)?;
        let authority = rest.split('/').next()?;
        Some(authority.split(':').next().unwrap_or(authority).to_string())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum McpStatus {
    Connected,
    Disconnected,
    Error,
    Unknown,
}

impl McpStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Connected => "connected",
            Self::Disconnected => "disconnected",
            Self::Error => "error",
            Self::Unknown => "unknown",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "connected" => Self::Connected,
            "disconnected" => Self::Disconnected,
            "error" => Self::Error,
            "unknown" => Self::Unknown,
            _ => return None,
        })
    }
}

/// A sandbox's attachment to a server, with what discovery found.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct SandboxMcpServer {
    pub id: Uuid,
    pub sandbox_id: Uuid,
    pub server_key: String,
    pub enabled: bool,
    pub status: McpStatus,
    /// `[{name, description, input_schema}]` as last discovered.
    pub tools: serde_json::Value,
    pub last_session_id: Option<String>,
    pub last_seen_at: Option<DateTime<Utc>>,
    pub last_error: Option<String>,
}

/// One MCP call as the proxy saw it on the wire.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct McpToolCall {
    pub id: Uuid,
    pub sandbox_id: Option<Uuid>,
    pub ts: DateTime<Utc>,
    pub server_host: String,
    pub server_key: Option<String>,
    pub session_id: Option<String>,
    pub method: String,
    pub tool_name: Option<String>,
    /// Redacted before storage.
    pub arguments: Option<serde_json::Value>,
    /// Redacted and cut at 16 KiB before storage.
    pub result: Option<serde_json::Value>,
    pub is_error: bool,
    pub duration_ms: Option<i32>,
    pub request_id: Option<String>,
}

/// What the proxy extracted from a JSON-RPC body, if it was MCP.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, ToSchema)]
pub struct McpFrame {
    pub method: Option<String>,
    pub tool_name: Option<String>,
    pub request_id: Option<String>,
    pub arguments: Option<serde_json::Value>,
}

/// Recognise an MCP request body.
///
/// MCP is JSON-RPC 2.0 with a known method set. A body that parses as JSON-RPC
/// with `"method"` starting with `initialize`, `tools/`, `resources/`,
/// `prompts/`, `notifications/`, `ping`, `logging/`, `completion/`, or
/// `sampling/` is MCP; anything else is "some JSON" and reported as such. The
/// tool name comes from `params.name` on `tools/call`.
pub fn parse_mcp_frame(body: &[u8]) -> Option<McpFrame> {
    let value: serde_json::Value = serde_json::from_slice(body).ok()?;
    let object = match &value {
        serde_json::Value::Object(map) => map,
        // A batch: take the first request that is MCP.
        serde_json::Value::Array(items) => items.iter().find_map(|item| item.as_object())?,
        _ => return None,
    };
    if object.get("jsonrpc").and_then(|v| v.as_str()) != Some("2.0") {
        return None;
    }
    let method = object.get("method")?.as_str()?.to_string();
    const PREFIXES: &[&str] = &[
        "initialize",
        "tools/",
        "resources/",
        "prompts/",
        "notifications/",
        "ping",
        "logging/",
        "completion/",
        "sampling/",
        "roots/",
        "elicitation/",
    ];
    if !PREFIXES.iter().any(|prefix| method.starts_with(prefix)) {
        return None;
    }
    let params = object.get("params").and_then(|p| p.as_object());
    let tool_name = if method == "tools/call" {
        params
            .and_then(|p| p.get("name"))
            .and_then(|n| n.as_str())
            .map(str::to_string)
    } else {
        None
    };
    let arguments = if method == "tools/call" {
        params.and_then(|p| p.get("arguments")).cloned()
    } else {
        None
    };
    let request_id = object.get("id").map(|id| match id {
        serde_json::Value::String(s) => s.clone(),
        other => other.to_string(),
    });
    Some(McpFrame {
        method: Some(method),
        tool_name,
        request_id,
        arguments,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tools_call_yields_the_tool_and_its_arguments() {
        let body = br#"{"jsonrpc":"2.0","id":7,"method":"tools/call","params":{"name":"read_file","arguments":{"path":"/etc/hostname"}}}"#;
        let frame = parse_mcp_frame(body).expect("mcp");
        assert_eq!(frame.method.as_deref(), Some("tools/call"));
        assert_eq!(frame.tool_name.as_deref(), Some("read_file"));
        assert_eq!(frame.request_id.as_deref(), Some("7"));
        assert_eq!(frame.arguments.and_then(|a| a.get("path").cloned()), Some(serde_json::json!("/etc/hostname")));
    }

    #[test]
    fn a_tools_list_has_no_tool_name() {
        let body = br#"{"jsonrpc":"2.0","id":"a","method":"tools/list"}"#;
        let frame = parse_mcp_frame(body).expect("mcp");
        assert_eq!(frame.method.as_deref(), Some("tools/list"));
        assert_eq!(frame.tool_name, None);
    }

    #[test]
    fn plain_json_rpc_that_is_not_mcp_is_not_mcp() {
        assert!(parse_mcp_frame(br#"{"jsonrpc":"2.0","id":1,"method":"eth_blockNumber"}"#).is_none());
        assert!(parse_mcp_frame(br#"{"model":"x","messages":[]}"#).is_none());
        assert!(parse_mcp_frame(b"not json").is_none());
    }

    #[test]
    fn a_batch_is_recognised_by_its_first_mcp_member() {
        let body = br#"[{"jsonrpc":"2.0","method":"notifications/initialized"},{"jsonrpc":"2.0","id":1,"method":"tools/list"}]"#;
        let frame = parse_mcp_frame(body).expect("mcp");
        assert_eq!(frame.method.as_deref(), Some("notifications/initialized"));
    }

    #[test]
    fn only_http_transports_are_visible_to_the_proxy() {
        assert!(McpTransport::Http.visible_to_proxy());
        assert!(McpTransport::Sse.visible_to_proxy());
        assert!(!McpTransport::Stdio.visible_to_proxy());
    }

    #[test]
    fn the_host_is_taken_from_the_endpoint() {
        let server = McpServer {
            key: "x".into(),
            name: "x".into(),
            description: None,
            publisher: None,
            homepage: None,
            transport: McpTransport::Http,
            endpoint: Some("https://mcp.example.com:8443/mcp".into()),
            command: None,
            args: vec![],
            auth_type: McpAuth::None,
            env_slots: vec![],
            packs: vec![],
            enabled: true,
        };
        assert_eq!(server.host().as_deref(), Some("mcp.example.com"));
    }
}
