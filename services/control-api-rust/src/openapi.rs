//! The OpenAPI document.
//!
//! Generated from one registry rather than from annotations on every handler.
//! The reason is testability: a test compares this registry against the router,
//! so a route that exists without documentation — or documentation for a route
//! that was removed — fails the build. Annotations can drift silently; a
//! comparison cannot.

use serde_json::json;

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
    /// Whether the operation streams (SSE).
    pub streams: bool,
}

/// Every endpoint the router exposes.
///
/// Kept in one place so the document is reviewable as a list rather than
/// reconstructed from attributes spread over ten files.
pub const OPERATIONS: &[Operation] = &[
    op("GET", "/api/v1/healthz", "Liveness probe", "system", "none", false, false),
    op("GET", "/api/v1/readyz", "Readiness probe, including the database", "system", "none", false, false),
    op("GET", "/api/v1/runtime", "Which runtime adapter is executing sandboxes", "system", "none", false, false),
    op("GET", "/api/v1/openapi.json", "This document", "system", "none", false, false),
    op("GET", "/api/schema/v1", "Alias of the OpenAPI document", "system", "none", false, false),
    op("GET", "/api/v1/docs", "Human-readable API entry page", "system", "none", false, false),
    op("POST", "/api/v1/auth/login", "Exchange credentials for a session", "auth", "none", false, false),
    op("POST", "/api/v1/auth/logout", "Revoke the current session", "auth", "viewer", true, false),
    op("GET", "/api/v1/auth/me", "The authenticated identity", "auth", "viewer", false, false),
    op("GET", "/api/v1/environments", "Environments this user may access", "environments", "viewer", false, false),
    op("GET", "/api/v1/environments/current", "Summary of the active environment", "environments", "viewer", false, false),
    op("GET", "/api/v1/sandboxes", "List sandboxes with cursor paging and facets", "sandboxes", "viewer", false, false),
    op("POST", "/api/v1/sandboxes", "Create a sandbox", "sandboxes", "operator", true, false),
    op("GET", "/api/v1/sandboxes/{id}", "Read a sandbox and its runtime facts", "sandboxes", "viewer", false, false),
    op("DELETE", "/api/v1/sandboxes/{id}", "Delete a sandbox and its container", "sandboxes", "operator", true, false),
    op("POST", "/api/v1/sandboxes/{id}/actions", "Start, stop, or restart a sandbox", "sandboxes", "operator", true, false),
    op("GET", "/api/v1/sandboxes/{id}/logs", "Recent log lines of a sandbox", "sandboxes", "viewer", false, false),
    op("GET", "/api/v1/sandboxes/{id}/inference", "Model requests made by a sandbox", "telemetry", "viewer", false, false),
    op("GET", "/api/v1/sandboxes/{id}/rules", "Network rules attached to a sandbox", "policies", "viewer", false, false),
    op("GET", "/api/v1/policies", "List policies and the global override", "policies", "viewer", false, false),
    op("POST", "/api/v1/policies", "Create a policy", "policies", "operator", true, false),
    op("GET", "/api/v1/policies/global", "The active global override", "policies", "viewer", false, false),
    op("GET", "/api/v1/policies/{id}", "Read a policy", "policies", "viewer", false, false),
    op("PUT", "/api/v1/policies/{id}", "Replace a policy document", "policies", "operator", true, false),
    op("DELETE", "/api/v1/policies/{id}", "Delete a policy", "policies", "operator", true, false),
    op("GET", "/api/v1/policies/{id}/diff", "Diff against an earlier revision", "policies", "viewer", false, false),
    op("GET", "/api/v1/network-rules", "List network rules", "policies", "viewer", false, false),
    op("POST", "/api/v1/network-rules", "Create a network rule", "policies", "operator", true, false),
    op("DELETE", "/api/v1/network-rules/{id}", "Delete a network rule", "policies", "operator", true, false),
    op("GET", "/api/v1/rule-drafts", "Pending rule drafts", "policies", "viewer", false, false),
    op("POST", "/api/v1/rule-drafts", "Draft a rule from an event", "policies", "operator", true, false),
    op("POST", "/api/v1/rule-drafts/{id}/approve", "Approve a draft and enforce it", "policies", "operator", true, false),
    op("POST", "/api/v1/rule-drafts/{id}/reject", "Reject a draft", "policies", "operator", true, false),
    op("POST", "/api/v1/traffic-windows", "Open a temporary traffic window", "policies", "operator", true, false),
    op("DELETE", "/api/v1/traffic-windows/{id}", "Close a traffic window early", "policies", "operator", true, false),
    op("GET", "/api/v1/telemetry/events", "Recent telemetry events", "telemetry", "viewer", false, false),
    op("GET", "/api/v1/telemetry/facets", "Event counts per kind", "telemetry", "viewer", false, false),
    op("GET", "/api/v1/telemetry/stream", "Live telemetry stream", "telemetry", "viewer", false, true),
    op("POST", "/api/v1/telemetry/ingest", "Record an event", "telemetry", "operator", true, false),
    op("POST", "/api/v1/telemetry/seed-demo", "Generate sample events for a fresh console", "telemetry", "operator", true, false),
    op("GET", "/api/v1/events/stream", "Live state-change stream", "telemetry", "viewer", false, true),
    op("GET", "/api/v1/gateways", "List gateways", "cluster", "viewer", false, false),
    op("GET", "/api/v1/gateways/{id}", "Read a gateway and its drift", "cluster", "viewer", false, false),
    op("PUT", "/api/v1/gateways/{id}/settings", "Replace gateway configuration", "cluster", "admin", true, false),
    op("POST", "/api/v1/gateways/{id}/sync", "Push configuration and clear drift", "cluster", "admin", true, false),
    op("GET", "/api/v1/supervisors", "List supervisors and their config dirtiness", "cluster", "viewer", false, false),
    op("GET", "/api/v1/topology", "The cluster graph", "cluster", "viewer", false, false),
    op("GET", "/api/v1/providers", "List provider endpoint profiles", "providers", "viewer", false, false),
    op("POST", "/api/v1/providers", "Register a provider profile", "providers", "operator", true, false),
    op("GET", "/api/v1/providers/{id}", "Read a provider profile", "providers", "viewer", false, false),
    op("PUT", "/api/v1/providers/{id}", "Change a provider profile", "providers", "operator", true, false),
    op("DELETE", "/api/v1/providers/{id}", "Remove a provider profile", "providers", "operator", true, false),
    op("POST", "/api/v1/providers/{id}/probe", "Probe a provider and its credential", "providers", "operator", true, false),
    op("GET", "/api/v1/models", "Model catalog with per-machine fit", "models", "viewer", false, false),
    op("GET", "/api/v1/models/installed", "Installed models", "models", "viewer", false, false),
    op("GET", "/api/v1/models/hardware", "Detected hardware", "models", "viewer", false, false),
    op("POST", "/api/v1/models/{key}/install", "Install or update a model", "models", "operator", true, false),
    op("POST", "/api/v1/models/{key}/uninstall", "Remove a model", "models", "operator", true, false),
    op("GET", "/api/v1/jobs", "List jobs", "jobs", "viewer", false, false),
    op("GET", "/api/v1/jobs/{id}", "Read a job and its progress", "jobs", "viewer", false, false),
    op("GET", "/api/v1/jobs/{id}/logs", "A job's log", "jobs", "viewer", false, false),
    op("GET", "/api/v1/schedules", "List schedules", "jobs", "viewer", false, false),
    op("POST", "/api/v1/schedules/{id}/run", "Run a schedule now", "jobs", "operator", true, false),
    op("GET", "/api/v1/audit-events", "Recent audit events", "audit", "viewer", false, false),
];

const fn op(
    method: &'static str,
    path: &'static str,
    summary: &'static str,
    tag: &'static str,
    role: &'static str,
    requires_csrf: bool,
    streams: bool,
) -> Operation {
    Operation {
        method,
        path,
        summary,
        tag,
        role,
        requires_csrf,
        streams,
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

        if operation.streams {
            fields.insert("x-stream".into(), json!("sse"));
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
        for operation in OPERATIONS {
            if matches!(operation.method, "POST" | "PUT" | "PATCH" | "DELETE") {
                if operation.path == "/api/v1/auth/login" {
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
    }
}
