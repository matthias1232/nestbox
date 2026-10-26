//! Topologie: Knoten und Kanten für die grafische Übersicht.

use serde::Serialize;
use utoipa::ToSchema;

/// Ein Knoten im Netzwerkbild – Runtime oder Sandbox.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct TopologyNode {
    pub id: String,
    pub label: String,
    /// `runtime`, `ubuntu-dev`, `hermes-agent`, …
    pub kind: String,
    pub status: String,
    #[serde(default)]
    pub ip: String,
}

/// Eine gerichtete Kante – „läuft auf“ oder „spricht mit“.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct TopologyEdge {
    pub source: String,
    pub target: String,
    pub label: String,
}

/// Das vollständige Bild für die GUI.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct Topology {
    pub nodes: Vec<TopologyNode>,
    pub edges: Vec<TopologyEdge>,
}
