//! The cluster graph, and the live traffic edges the Admin Center adds to it.
//!
//! The existing graph is static: nodes, gateways, sandboxes, and what belongs to
//! what. `?live=5m` adds edges from recent egress traffic (sandbox →
//! destination, counted and marked when anything was blocked). The graph itself
//! is computed on the server so the console never aggregates a week of rows in a
//! browser.
//!
//! Two properties the extension must keep:
//!
//! 1. **Without `live`, the answer is unchanged.** The static graph is what four
//!    other screens already read; adding a parameter must not alter it.
//! 2. **Live edges are additive.** They are appended to the same `nodes` and
//!    `edges` arrays with `kind: "egress"`, so a renderer that only knows the
//!    static kinds draws what it drew before and ignores the rest.

use axum::Json;
use axum::extract::{Query, State};
use serde::Deserialize;

use crate::api::principal::Auth;
use crate::error::ApiError;
use crate::services::gateway_service;
use crate::services::traffic_service::{self, LiveEdges};
use crate::state::AppState;

/// The live parameter of the cluster graph.
#[derive(Debug, Deserialize)]
pub struct TopologyQuery {
    /// A window such as `5m`. Absent means the static graph only.
    pub live: Option<String>,
}

/// The cluster graph, optionally with live egress edges.
pub async fn topology(
    State(state): State<AppState>,
    auth: Auth,
    Query(query): Query<TopologyQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let mut graph = gateway_service::topology(&state.services, &auth.principal).await?;

    // An empty `?live=` is the parameter with nothing asked for; treating it as
    // "off" is kinder than a 422 for a query string a UI may build naively.
    let live = query
        .live
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());

    let Some(live) = live else {
        return Ok(Json(graph));
    };

    let edges = traffic_service::live_edges(&state.services, &auth.principal, Some(live)).await?;
    attach_live(&mut graph, &edges);
    Ok(Json(graph))
}

/// The identifier a destination has in the graph.
///
/// Prefixed so a destination can never collide with a uuid-shaped node id.
fn destination_id(host: &str, port: i32) -> String {
    format!("dest:{host}:{port}")
}

/// Append live edges and their destination nodes to a graph.
///
/// Kept as a plain function over `serde_json::Value` because that is the shape
/// `gateway_service::topology` returns and because the merge has no rule in it:
/// it is the same edge, spelled the way the graph spells edges.
fn attach_live(graph: &mut serde_json::Value, live: &LiveEdges) {
    let Some(object) = graph.as_object_mut() else {
        return;
    };

    // Read both arrays out first: the map is borrowed for writing at the end, and
    // two simultaneous `get_mut` calls on one map do not borrow-check.
    let mut nodes: Vec<serde_json::Value> = object
        .get("nodes")
        .and_then(serde_json::Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut edges: Vec<serde_json::Value> = object
        .get("edges")
        .and_then(serde_json::Value::as_array)
        .cloned()
        .unwrap_or_default();

    let known: std::collections::HashSet<String> = nodes
        .iter()
        .filter_map(|node| node.get("id").and_then(|id| id.as_str()))
        .map(str::to_string)
        .collect();

    for edge in &live.edges {
        let id = destination_id(&edge.host, edge.port);
        if known.contains(&id) || nodes.iter().any(|node| node["id"] == id.as_str()) {
            continue;
        }
        nodes.push(serde_json::json!({
            "id": id,
            "kind": "destination",
            "name": edge.host,
            "port": edge.port,
            "protocol": edge.protocol,
        }));
    }

    for edge in &live.edges {
        edges.push(serde_json::json!({
            "from": edge.sandbox_id,
            "to": destination_id(&edge.host, edge.port),
            "kind": "egress",
            "protocol": edge.protocol,
            "port": edge.port,
            "requests": edge.requests,
            "allowed": edge.allowed,
            "denied": edge.denied,
            // The map's one piece of advice: red when anything was stopped.
            "blocked": edge.denied > 0,
            "bytes": edge.bytes,
            "last_seen": edge.last_seen,
            "tools": edge.tools,
        }));
    }

    object.insert("nodes".to_string(), serde_json::Value::Array(nodes));
    object.insert("edges".to_string(), serde_json::Value::Array(edges));
    object.insert(
        "live".to_string(),
        serde_json::json!({
            "window": live.window,
            "window_seconds": live.window_seconds,
            "from": live.from,
            "to": live.to,
            "edges": live.edges.len(),
            // True when the cap cut the list, so the console says "top
            // connections" rather than implying completeness.
            "truncated": live.truncated,
            "sandboxes": live.sandboxes,
        }),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use uuid::Uuid;

    fn live_edges() -> LiveEdges {
        let sandbox_id = Uuid::nil();
        LiveEdges {
            window: "5m".into(),
            window_seconds: 300,
            from: Utc::now(),
            to: Utc::now(),
            edges: vec![crate::domain::egress::TrafficEdge {
                sandbox_id,
                host: "example.com".into(),
                port: 443,
                protocol: "https".into(),
                requests: 4,
                allowed: 3,
                denied: 1,
                bytes: 2048,
                last_seen: Utc::now(),
                tools: vec!["read_file".into()],
            }],
            sandboxes: vec![traffic_service::LiveSandbox {
                id: sandbox_id,
                name: "hermes-01".into(),
                node_id: None,
            }],
            truncated: false,
        }
    }

    #[test]
    fn live_edges_are_appended_without_touching_the_static_ones() {
        let mut graph = serde_json::json!({
            "nodes": [{ "id": "gateway-1", "kind": "gateway" }],
            "edges": [{ "from": "cluster-1", "to": "gateway-1", "kind": "hosts" }],
        });
        attach_live(&mut graph, &live_edges());

        let nodes = graph["nodes"].as_array().expect("nodes");
        assert_eq!(nodes.len(), 2);
        assert_eq!(nodes[0]["kind"], "gateway");
        assert_eq!(nodes[1]["kind"], "destination");
        assert_eq!(nodes[1]["id"], "dest:example.com:443");

        let edges = graph["edges"].as_array().expect("edges");
        assert_eq!(edges.len(), 2);
        assert_eq!(edges[0]["kind"], "hosts");
        assert_eq!(edges[1]["kind"], "egress");
        assert_eq!(edges[1]["blocked"], true);
        assert_eq!(edges[1]["tools"][0], "read_file");
        assert_eq!(graph["live"]["edges"], 1);
    }

    #[test]
    fn a_destination_node_is_added_once() {
        let mut graph = serde_json::json!({ "nodes": [], "edges": [] });
        let mut live = live_edges();
        let mut second = live.edges[0].clone();
        second.port = 443;
        live.edges.push(second);
        attach_live(&mut graph, &live);
        assert_eq!(graph["nodes"].as_array().map(Vec::len), Some(1));
        assert_eq!(graph["edges"].as_array().map(Vec::len), Some(2));
    }

    #[test]
    fn a_destination_id_cannot_collide_with_a_uuid() {
        assert_eq!(destination_id("example.com", 443), "dest:example.com:443");
    }

    #[test]
    fn a_graph_that_is_not_an_object_is_left_alone() {
        // Defensive: the merge must never panic on a shape it did not expect.
        let mut graph = serde_json::json!("not a graph");
        attach_live(&mut graph, &live_edges());
        assert_eq!(graph, serde_json::json!("not a graph"));
    }
}
