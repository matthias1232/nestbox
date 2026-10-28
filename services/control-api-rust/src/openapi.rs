//! The OpenAPI document.
//!
//! Generated from one registry rather than from annotations on every handler.
//! The reason is testability: a test compares this registry against the router,
//! so a route that exists without documentation — or documentation for a route
//! that was removed — fails the build. Annotations can drift silently; a
//! comparison cannot.

use serde_json::json;

/// How an operation streams, if it streams at all.
///
/// SSE and WebSocket are both "a stream" to a browser and very different to a
/// proxy: nginx needs an `Upgrade` header for one and buffering off for the
/// other. The document says which, so the gateway config and the client agree
/// instead of guessing from the path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamKind {
    None,
    Sse,
    Ws,
}

impl StreamKind {
    /// Whether this operation streams at all.
    pub fn is_stream(self) -> bool {
        !matches!(self, Self::None)
    }

    /// The `x-stream` value in the document.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Sse => "sse",
            Self::Ws => "ws",
        }
    }
}

/// One documented operation.
pub struct Operation {
    pub method: &'static str,
    pub path: &'static str,
    pub summary: &'static str,
    pub tag: &'static str,
    /// Minimum role, so a client can render or hide an action without guessing.
    pub role: &'static str,
    /// Whether a CSRF token is required (a session-authenticated mutation).
    pub requires_csrf: bool,
    /// Whether, and how, the operation streams.
    pub stream: StreamKind,
}

/// Every endpoint the router exposes.
///
/// Kept in one place so the document is reviewable as a list rather than
/// reconstructed from attributes spread over ten files.
pub const OPERATIONS: &[Operation] = &[
    op("GET", "/api/v1/healthz", "Liveness probe", "system", "none", false, StreamKind::None),
    op("GET", "/api/v1/readyz", "Readiness probe, including the database", "system", "none", false, StreamKind::None),
    op("GET", "/api/v1/runtime", "Which runtime adapter is executing sandboxes", "system", "none", false, StreamKind::None),
    op("GET", "/api/v1/openapi.json", "This document", "system", "none", false, StreamKind::None),
    op("GET", "/api/schema/v1", "Alias of the OpenAPI document", "system", "none", false, StreamKind::None),
    op("GET", "/api/v1/docs", "Human-readable API entry page", "system", "none", false, StreamKind::None),
    op("POST", "/api/v1/auth/login", "Exchange credentials for a session", "auth", "none", false, StreamKind::None),
    op("POST", "/api/v1/auth/logout", "Revoke the current session", "auth", "viewer", true, StreamKind::None),
    op("GET", "/api/v1/auth/me", "The authenticated identity", "auth", "viewer", false, StreamKind::None),
    op("GET", "/api/v1/environments", "Environments this user may access", "environments", "viewer", false, StreamKind::None),
    op("GET", "/api/v1/environments/current", "Summary of the active environment", "environments", "viewer", false, StreamKind::None),
    op("GET", "/api/v1/sandboxes", "List sandboxes with cursor paging and facets", "sandboxes", "viewer", false, StreamKind::None),
    op("POST", "/api/v1/sandboxes", "Create a sandbox", "sandboxes", "operator", true, StreamKind::None),
    op("GET", "/api/v1/sandboxes/{id}", "Read a sandbox and its runtime facts", "sandboxes", "viewer", false, StreamKind::None),
    op("DELETE", "/api/v1/sandboxes/{id}", "Delete a sandbox and its container", "sandboxes", "operator", true, StreamKind::None),
    op("POST", "/api/v1/sandboxes/{id}/actions", "Start, stop, or restart a sandbox", "sandboxes", "operator", true, StreamKind::None),
    op("GET", "/api/v1/sandboxes/{id}/logs", "Recent log lines of a sandbox", "sandboxes", "viewer", false, StreamKind::None),
    op("POST", "/api/v1/sandboxes/{id}/exec", "Run one command inside a sandbox", "sandboxes", "operator", true, StreamKind::None),
    op("GET", "/api/v1/sandboxes/{id}/inference", "Model requests made by a sandbox", "telemetry", "viewer", false, StreamKind::None),
    op("GET", "/api/v1/sandboxes/{id}/rules", "Network rules attached to a sandbox", "policies", "viewer", false, StreamKind::None),
    op("GET", "/api/v1/policies", "List policies and the global override", "policies", "viewer", false, StreamKind::None),
    op("POST", "/api/v1/policies", "Create a policy", "policies", "operator", true, StreamKind::None),
    op("GET", "/api/v1/policies/global", "The active global override", "policies", "viewer", false, StreamKind::None),
    op("GET", "/api/v1/policies/{id}", "Read a policy", "policies", "viewer", false, StreamKind::None),
    op("PUT", "/api/v1/policies/{id}", "Replace a policy document", "policies", "operator", true, StreamKind::None),
    op("DELETE", "/api/v1/policies/{id}", "Delete a policy", "policies", "operator", true, StreamKind::None),
    op("GET", "/api/v1/policies/{id}/diff", "Diff against an earlier revision", "policies", "viewer", false, StreamKind::None),
    op("GET", "/api/v1/policy-templates", "List the preset library", "policies", "viewer", false, StreamKind::None),
    op("POST", "/api/v1/policy-templates", "Save a preset", "policies", "operator", true, StreamKind::None),
    op("POST", "/api/v1/policy-templates/{id}/instantiate", "Turn a preset into a policy", "policies", "operator", true, StreamKind::None),
    op("GET", "/api/v1/policy-schema", "The document schema the editor renders from", "policies", "viewer", false, StreamKind::None),
    op("GET", "/api/v1/sandboxes/{id}/policy", "The policy a sandbox carries", "policies", "viewer", false, StreamKind::None),
    op("PUT", "/api/v1/sandboxes/{id}/policy", "Assign a policy to a sandbox", "policies", "operator", true, StreamKind::None),
    op("DELETE", "/api/v1/sandboxes/{id}/policy", "Remove a sandbox's policy", "policies", "operator", true, StreamKind::None),
    op("GET", "/api/v1/network-rules", "List network rules", "policies", "viewer", false, StreamKind::None),
    op("POST", "/api/v1/network-rules", "Create a network rule", "policies", "operator", true, StreamKind::None),
    op("DELETE", "/api/v1/network-rules/{id}", "Delete a network rule", "policies", "operator", true, StreamKind::None),
    op("GET", "/api/v1/dns-rules", "List DNS rules", "policies", "viewer", false, StreamKind::None),
    op("POST", "/api/v1/dns-rules", "Create a DNS rule", "policies", "operator", true, StreamKind::None),
    op("DELETE", "/api/v1/dns-rules/{id}", "Delete a DNS rule", "policies", "operator", true, StreamKind::None),
    op("GET", "/api/v1/sandboxes/{id}/egress-mode", "One sandbox's egress mode", "policies", "viewer", false, StreamKind::None),
    op("PUT", "/api/v1/sandboxes/{id}/egress-mode", "Set one sandbox's egress mode", "policies", "operator", true, StreamKind::None),
    op("GET", "/api/v1/rule-drafts", "Pending rule drafts", "policies", "viewer", false, StreamKind::None),
    op("POST", "/api/v1/rule-drafts", "Draft a rule from an event", "policies", "operator", true, StreamKind::None),
    op("POST", "/api/v1/rule-drafts/{id}/approve", "Approve a draft and enforce it", "policies", "operator", true, StreamKind::None),
    op("POST", "/api/v1/rule-drafts/{id}/reject", "Reject a draft", "policies", "operator", true, StreamKind::None),
    op("POST", "/api/v1/traffic-windows", "Open a temporary traffic window", "policies", "operator", true, StreamKind::None),
    op("DELETE", "/api/v1/traffic-windows/{id}", "Close a traffic window early", "policies", "operator", true, StreamKind::None),
    op("GET", "/api/v1/telemetry/events", "Recent telemetry events", "telemetry", "viewer", false, StreamKind::None),
    op("GET", "/api/v1/telemetry/facets", "Event counts per kind", "telemetry", "viewer", false, StreamKind::None),
    op("GET", "/api/v1/telemetry/stream", "Live telemetry stream", "telemetry", "viewer", false, StreamKind::Sse),
    op("POST", "/api/v1/telemetry/ingest", "Record an event", "telemetry", "operator", true, StreamKind::None),
    op("POST", "/api/v1/telemetry/seed-demo", "Generate sample events for a fresh console", "telemetry", "operator", true, StreamKind::None),
    op("GET", "/api/v1/events/stream", "Live state-change stream", "telemetry", "viewer", false, StreamKind::Sse),
    op("GET", "/api/v1/gateways", "List gateways", "cluster", "viewer", false, StreamKind::None),
    op("GET", "/api/v1/gateways/{id}", "Read a gateway and its drift", "cluster", "viewer", false, StreamKind::None),
    op("PUT", "/api/v1/gateways/{id}/settings", "Replace gateway configuration", "cluster", "admin", true, StreamKind::None),
    op("POST", "/api/v1/gateways/{id}/sync", "Push configuration and clear drift", "cluster", "admin", true, StreamKind::None),
    op("GET", "/api/v1/supervisors", "List supervisors and their config dirtiness", "cluster", "viewer", false, StreamKind::None),
    op("POST", "/api/v1/supervisors", "Register a supervisor for a sandbox", "cluster", "operator", true, StreamKind::None),
    op("PATCH", "/api/v1/supervisors/{id}", "Bind a supervisor to a sandbox", "cluster", "operator", true, StreamKind::None),
    op("POST", "/api/v1/internal/supervisors/{id}/report", "A supervisor reports what it measured (supervisor token)", "cluster", "node", false, StreamKind::None),
    op("GET", "/api/v1/topology", "The cluster graph", "cluster", "viewer", false, StreamKind::None),
    op("GET", "/api/v1/providers", "List provider endpoint profiles", "providers", "viewer", false, StreamKind::None),
    op("POST", "/api/v1/providers", "Register a provider profile", "providers", "operator", true, StreamKind::None),
    op("GET", "/api/v1/providers/{id}", "Read a provider profile", "providers", "viewer", false, StreamKind::None),
    op("PUT", "/api/v1/providers/{id}", "Change a provider profile", "providers", "operator", true, StreamKind::None),
    op("DELETE", "/api/v1/providers/{id}", "Remove a provider profile", "providers", "operator", true, StreamKind::None),
    op("POST", "/api/v1/providers/{id}/probe", "Probe a provider and its credential", "providers", "operator", true, StreamKind::None),
    op("GET", "/api/v1/models", "Model catalog with per-machine fit", "models", "viewer", false, StreamKind::None),
    op("GET", "/api/v1/models/installed", "Installed models", "models", "viewer", false, StreamKind::None),
    op("GET", "/api/v1/models/hardware", "Detected hardware", "models", "viewer", false, StreamKind::None),
    op("GET", "/api/v1/models/facets", "Catalog counts per family, publisher, runtime, and verdict", "models", "viewer", false, StreamKind::None),
    op("GET", "/api/v1/models/requirements", "One model's requirements and verdict", "models", "viewer", false, StreamKind::None),
    op("POST", "/api/v1/models/{key}/install", "Install or update a model on a node", "models", "operator", true, StreamKind::None),
    op("POST", "/api/v1/models/{key}/uninstall", "Remove a model from a node", "models", "operator", true, StreamKind::None),
    op("GET", "/api/v1/models/installs", "Model installs per node with reservations", "models", "viewer", false, StreamKind::None),
    op("POST", "/api/v1/models/sync", "Refresh the catalog from Hugging Face, Ollama, and NVIDIA", "models", "operator", true, StreamKind::None),
    op("GET", "/api/v1/nodes", "Nodes with capacity, reservations, and sandbox counts", "nodes", "viewer", false, StreamKind::None),
    op("GET", "/api/v1/nodes/{id}", "One node with its budget", "nodes", "viewer", false, StreamKind::None),
    op("GET", "/api/v1/nodes/{id}/sandboxes", "Sandboxes on a node", "nodes", "viewer", false, StreamKind::None),
    op("PUT", "/api/v1/nodes/{id}/display", "Rename or describe a node", "nodes", "admin", true, StreamKind::None),
    op("POST", "/api/v1/nodes/{id}/probe", "Probe a node's hardware through its runner", "nodes", "operator", true, StreamKind::None),
    op("GET", "/api/v1/baselines", "Base operating systems a recipe can start from", "builder", "viewer", false, StreamKind::None),
    op("GET", "/api/v1/agent-packs", "Agent packs with baseline compatibility", "builder", "viewer", false, StreamKind::None),
    op("GET", "/api/v1/tool-catalog", "Tools with their apt and dnf names", "builder", "viewer", false, StreamKind::None),
    op("GET", "/api/v1/recipes", "List recipes", "builder", "viewer", false, StreamKind::None),
    op("POST", "/api/v1/recipes", "Create a draft recipe", "builder", "operator", true, StreamKind::None),
    op("GET", "/api/v1/recipes/{id}", "Read a recipe", "builder", "viewer", false, StreamKind::None),
    op("PATCH", "/api/v1/recipes/{id}", "Edit a recipe; bumps the revision", "builder", "operator", true, StreamKind::None),
    op("DELETE", "/api/v1/recipes/{id}", "Delete a recipe", "builder", "operator", true, StreamKind::None),
    op("POST", "/api/v1/recipes/{id}/build", "Build the recipe's image as a job", "builder", "operator", true, StreamKind::None),
    op("POST", "/api/v1/recipes/{id}/instantiate", "Create a sandbox from a built recipe", "builder", "operator", true, StreamKind::None),
    op("GET", "/api/v1/persistent-roots", "Persistent roots and where they are mounted", "persistence", "viewer", false, StreamKind::None),
    op("POST", "/api/v1/persistent-roots", "Create a persistent root on a node", "persistence", "operator", true, StreamKind::None),
    op("GET", "/api/v1/persistent-roots/{id}", "One root with its mounts", "persistence", "viewer", false, StreamKind::None),
    op("DELETE", "/api/v1/persistent-roots/{id}", "Archive or delete a root", "persistence", "operator", true, StreamKind::None),
    op("POST", "/api/v1/persistent-roots/{id}/mount", "Mount a root into a sandbox", "persistence", "operator", true, StreamKind::None),
    op("POST", "/api/v1/persistent-roots/{id}/dismount", "Dismount a root from a sandbox", "persistence", "operator", true, StreamKind::None),
    op("GET", "/api/v1/persistent-roots/{id}/usage", "Measured disk usage of a root", "persistence", "viewer", false, StreamKind::None),
    op("GET", "/api/v1/sandboxes/{id}/agents", "Agents inside a sandbox", "sandboxes", "viewer", false, StreamKind::None),
    op("POST", "/api/v1/sandboxes/{id}/agents", "Add an agent to a sandbox", "sandboxes", "operator", true, StreamKind::None),
    op("DELETE", "/api/v1/sandboxes/{id}/agents/{instance}", "Remove an agent, keeping or dropping its data", "sandboxes", "operator", true, StreamKind::None),
    op("GET", "/api/v1/jobs", "List jobs", "jobs", "viewer", false, StreamKind::None),
    op("GET", "/api/v1/jobs/{id}", "Read a job and its progress", "jobs", "viewer", false, StreamKind::None),
    op("GET", "/api/v1/jobs/{id}/logs", "A job's log", "jobs", "viewer", false, StreamKind::None),
    op("GET", "/api/v1/schedules", "List schedules", "jobs", "viewer", false, StreamKind::None),
    op("POST", "/api/v1/schedules/{id}/run", "Run a schedule now", "jobs", "operator", true, StreamKind::None),
    op("GET", "/api/v1/audit-events", "Recent audit events", "audit", "viewer", false, StreamKind::None),
    // Admin Center: resource history, processes, inventory
    op("GET", "/api/v1/sandboxes/{id}/metrics", "CPU, memory, network and disk over a window", "sandboxes", "viewer", false, StreamKind::None),
    op("GET", "/api/v1/sandboxes/{id}/processes", "The sandbox's process table, read live", "sandboxes", "viewer", false, StreamKind::None),
    op("GET", "/api/v1/sandboxes/{id}/inventory", "The last inventory snapshot and its age", "sandboxes", "viewer", false, StreamKind::None),
    op("POST", "/api/v1/sandboxes/{id}/inventory/refresh", "Take a new inventory snapshot", "sandboxes", "operator", true, StreamKind::None),
    op("POST", "/api/v1/internal/metrics/ingest", "Record a batch of resource samples (node token)", "sandboxes", "node", false, StreamKind::None),
    // Admin Center: terminal, SSH, ports, sessions
    op("GET", "/api/v1/sandboxes/{id}/terminal", "Attach an interactive terminal over WebSocket", "sandboxes", "operator", false, StreamKind::Ws),
    op("GET", "/api/v1/sandboxes/{id}/ssh-key", "The SSH key record, never the private key", "sandboxes", "viewer", false, StreamKind::None),
    op("GET", "/api/v1/sandboxes/{id}/ssh-key/download", "Download the private key, once", "sandboxes", "operator", false, StreamKind::None),
    op("POST", "/api/v1/sandboxes/{id}/ssh-key/rotate", "Replace the SSH key pair", "sandboxes", "operator", true, StreamKind::None),
    op("GET", "/api/v1/sandboxes/{id}/ports", "Ports published on the node for this sandbox", "sandboxes", "viewer", false, StreamKind::None),
    op("POST", "/api/v1/sandboxes/{id}/ports", "Publish another sandbox port from the node pool", "sandboxes", "operator", true, StreamKind::None),
    op("DELETE", "/api/v1/sandboxes/{id}/ports/{forward_id}", "Withdraw a published port", "sandboxes", "operator", true, StreamKind::None),
    op("GET", "/api/v1/sandboxes/{id}/sessions", "Terminal and SSH sessions, without content", "sandboxes", "viewer", false, StreamKind::None),
    // Admin Center: traffic, map, effective policies, MCP calls
    op("GET", "/api/v1/sandboxes/{id}/traffic", "Egress traffic rows for a window", "telemetry", "viewer", false, StreamKind::None),
    op("GET", "/api/v1/sandboxes/{id}/traffic/{event_id}/body", "The redacted request and response body", "telemetry", "viewer", false, StreamKind::None),
    op("GET", "/api/v1/sandboxes/{id}/map", "Aggregated egress edges for the live map", "telemetry", "viewer", false, StreamKind::None),
    op("GET", "/api/v1/sandboxes/{id}/mcp/calls", "MCP calls the proxy saw", "mcp", "viewer", false, StreamKind::None),
    op("GET", "/api/v1/sandboxes/{id}/policies/effective", "Rules that apply to this sandbox, with deployment state", "policies", "viewer", false, StreamKind::None),
    // MCP registry and sandbox attachments
    op("GET", "/api/v1/mcp/servers", "MCP servers in the registry", "mcp", "viewer", false, StreamKind::None),
    op("POST", "/api/v1/mcp/servers", "Register an MCP server", "mcp", "operator", true, StreamKind::None),
    op("GET", "/api/v1/mcp/servers/{key}", "Read an MCP server definition", "mcp", "viewer", false, StreamKind::None),
    op("PUT", "/api/v1/mcp/servers/{key}", "Replace an MCP server definition", "mcp", "operator", true, StreamKind::None),
    op("DELETE", "/api/v1/mcp/servers/{key}", "Remove an MCP server definition", "mcp", "operator", true, StreamKind::None),
    op("GET", "/api/v1/sandboxes/{id}/mcp/servers", "MCP servers attached to a sandbox", "mcp", "viewer", false, StreamKind::None),
    op("POST", "/api/v1/sandboxes/{id}/mcp/servers", "Attach a registry server to a sandbox", "mcp", "operator", true, StreamKind::None),
    op("DELETE", "/api/v1/sandboxes/{id}/mcp/servers/{key}", "Detach a server from a sandbox", "mcp", "operator", true, StreamKind::None),
    op("POST", "/api/v1/sandboxes/{id}/mcp/servers/{key}/discover", "Ask a server for its tool list, through the policy check", "mcp", "operator", true, StreamKind::None),
    // Internal: node-token endpoints for the proxy and the runner
    op("POST", "/api/v1/internal/policy/check", "Decide one egress request (node token)", "policies", "node", false, StreamKind::None),
    op("POST", "/api/v1/internal/telemetry/batch", "Ingest a proxy access-log batch (node token)", "telemetry", "node", false, StreamKind::None),
];

const fn op(
    method: &'static str,
    path: &'static str,
    summary: &'static str,
    tag: &'static str,
    role: &'static str,
    requires_csrf: bool,
    stream: StreamKind,
) -> Operation {
    Operation {
        method,
        path,
        summary,
        tag,
        role,
        requires_csrf,
        stream,
    }
}

/// Build the OpenAPI document.
pub fn document() -> serde_json::Value {
    let mut paths = serde_json::Map::new();

    for operation in OPERATIONS {
        let entry = paths
            .entry(operation.path.to_string())
            .or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()));

        // Built as a map up front rather than patched through a fallible
        // accessor, so there is no unwrap on a value this function constructs.
        let mut fields = serde_json::Map::new();
        fields.insert("summary".into(), json!(operation.summary));
        fields.insert("tags".into(), json!([operation.tag]));
        fields.insert("x-roles".into(), json!([operation.role]));
        fields.insert("x-requires-csrf".into(), json!(operation.requires_csrf));

        if operation.stream.is_stream() {
            fields.insert("x-stream".into(), json!(operation.stream.as_str()));
            fields.insert(
                "responses".into(),
                json!({ "200": { "description": "A stream of server-sent events" } }),
            );
        } else {
            fields.insert(
                "responses".into(),
                json!({ "200": { "description": "Success" } }),
            );
        }

        let body = serde_json::Value::Object(fields);

        if let Some(object) = entry.as_object_mut() {
            object.insert(operation.method.to_lowercase(), body);
        }
    }

    json!({
        "openapi": "3.1.0",
        "info": {
            "title": "Nestbox Control API",
            "version": "2.0.0",
            "description": "API-first control plane for governed AI-agent sandboxes. \
                            The /api/v1/ prefix is frozen; a breaking change gets a new prefix."
        },
        "servers": [{ "url": "/" }],
        "components": {
            "securitySchemes": {
                "session": { "type": "apiKey", "in": "cookie", "name": "nestbox_session" },
                "token": { "type": "http", "scheme": "bearer" }
            },
            "schemas": {
                "Error": {
                    "type": "object",
                    "properties": {
                        "error": {
                            "type": "object",
                            "properties": {
                                "code": { "type": "string" },
                                "message": { "type": "string" },
                                "details": { "type": "object" },
                                "request_id": { "type": "string" }
                            },
                            "required": ["code", "message"]
                        }
                    }
                }
            }
        },
        "security": [{ "session": [] }, { "token": [] }],
        "tags": [
            { "name": "system" }, { "name": "auth" }, { "name": "environments" },
            { "name": "sandboxes" }, { "name": "policies" }, { "name": "telemetry" },
            { "name": "cluster" }, { "name": "providers" }, { "name": "models" }, { "name": "jobs" },
            { "name": "nodes" }, { "name": "builder" }, { "name": "persistence" }, { "name": "mcp" },
            { "name": "audit" }
        ],
        "paths": paths,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_operation_has_a_unique_method_and_path() {
        let mut seen = std::collections::BTreeSet::new();
        for operation in OPERATIONS {
            let key = format!("{} {}", operation.method, operation.path);
            assert!(seen.insert(key.clone()), "duplicate operation: {key}");
        }
    }

    #[test]
    fn paths_are_versioned_except_the_schema_alias() {
        for operation in OPERATIONS {
            assert!(
                operation.path.starts_with("/api/v1/")
                    || operation.path.starts_with("/api/schema/"),
                "{} is outside the versioned prefix",
                operation.path
            );
        }
    }

    #[test]
    fn every_mutation_declares_csrf_requirements() {
        // A mutation that claims no CSRF need is either token-authenticated or a
        // mistake, and the list is small enough to enumerate deliberately.
        //
        // The internal routes are authenticated by a node token, not a session
        // cookie, so there is no browser to protect and no CSRF token to send.
        // Listing them here means a fourth one cannot appear without a decision.
        const NODE_TOKEN_MUTATIONS: &[&str] = &[
            "/api/v1/internal/metrics/ingest",
            "/api/v1/internal/policy/check",
            "/api/v1/internal/supervisors/{id}/report",
            "/api/v1/internal/telemetry/batch",
        ];
        for operation in OPERATIONS {
            if matches!(operation.method, "POST" | "PUT" | "PATCH" | "DELETE") {
                if operation.path == "/api/v1/auth/login" {
                    continue;
                }
                if NODE_TOKEN_MUTATIONS.contains(&operation.path) {
                    continue;
                }
                assert!(
                    operation.requires_csrf,
                    "{} {} must declare CSRF requirements",
                    operation.method, operation.path
                );
            }
        }
    }

    #[test]
    fn the_node_token_routes_are_the_ones_that_skip_csrf() {
        const NODE_TOKEN_MUTATIONS: &[&str] = &[
            "/api/v1/internal/metrics/ingest",
            "/api/v1/internal/policy/check",
            "/api/v1/internal/supervisors/{id}/report",
            "/api/v1/internal/telemetry/batch",
        ];
        for path in NODE_TOKEN_MUTATIONS {
            let operation = OPERATIONS
                .iter()
                .find(|operation| operation.method == "POST" && operation.path == *path)
                .unwrap_or_else(|| panic!("{path} is not a documented POST"));
            assert!(
                !operation.requires_csrf,
                "{path} is node-token authenticated and needs no CSRF token"
            );
            assert_eq!(operation.role, "node", "{path} runs as a node, not a user");
        }
    }

    #[test]
    fn the_document_contains_every_registered_path() {
        let document = document();
        let paths = document["paths"].as_object().expect("paths");
        for operation in OPERATIONS {
            let entry = paths
                .get(operation.path)
                .unwrap_or_else(|| panic!("missing path {} in document", operation.path));
            assert!(
                entry.get(operation.method.to_lowercase()).is_some(),
                "missing {} {} in document",
                operation.method,
                operation.path
            );
        }
    }

    #[test]
    fn streams_are_marked() {
        let document = document();
        let stream = &document["paths"]["/api/v1/telemetry/stream"]["get"];
        assert_eq!(stream["x-stream"], "sse");

        // The terminal is a WebSocket, not SSE: a gateway needs different
        // settings for it, and the document has to say which.
        let terminal = &document["paths"]["/api/v1/sandboxes/{id}/terminal"]["get"];
        assert_eq!(terminal["x-stream"], "ws");

        // A plain request must not claim to stream at all.
        let plain = &document["paths"]["/api/v1/sandboxes/{id}/metrics"]["get"];
        assert!(plain.get("x-stream").is_none());
    }
}
