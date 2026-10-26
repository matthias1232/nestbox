//! Bindings und die daraus abgeleitete Topologie.

use chrono::Utc;
use uuid::Uuid;

use crate::domain::binding::{Binding, BindingInput};
use crate::domain::topology::{Topology, TopologyEdge, TopologyNode};
use crate::error::ApiError;
use crate::services::sandboxes;
use crate::state::AppState;

/// Alle deklarierten Bindings.
pub async fn list(state: &AppState) -> Vec<Binding> {
    state.data.read().await.bindings.values().cloned().collect()
}

/// Legt ein Binding an; beide Enden müssen existieren.
pub async fn create(state: &AppState, input: BindingInput) -> Result<Binding, ApiError> {
    {
        let data = state.data.read().await;
        let exists = |name: &str| data.sandboxes.values().any(|item| item.name == name);
        if !exists(&input.source) || !exists(&input.target) {
            return Err(ApiError::Invalid(
                "source und target müssen existierende Sandboxes sein.".to_string(),
            ));
        }
        if data.bindings.contains_key(&input.name) {
            return Err(ApiError::Conflict {
                resource: "binding",
                name: input.name,
            });
        }
    }

    let binding = Binding {
        id: Uuid::new_v4().to_string(),
        name: input.name.clone(),
        source: input.source,
        target: input.target,
        protocol: input.protocol,
        port: input.port,
        purpose: input.purpose,
        created_at: Utc::now().to_rfc3339(),
    };

    state
        .data
        .write()
        .await
        .bindings
        .insert(binding.name.clone(), binding.clone());

    state
        .audit(
            "binding.created",
            &binding.id,
            serde_json::json!({ "source": binding.source, "target": binding.target }),
        )
        .await;

    Ok(binding)
}

/// Baut das Netzwerkbild für die GUI.
pub async fn topology(state: &AppState) -> Topology {
    let items = sandboxes::list(state).await;
    let bindings = list(state).await;

    let mut nodes = vec![TopologyNode {
        id: "runtime".to_string(),
        label: format!("{} runtime", state.runtime.name()),
        kind: "runtime".to_string(),
        status: "ready".to_string(),
        ip: String::new(),
    }];

    let mut edges = Vec::new();

    for sandbox in &items {
        nodes.push(TopologyNode {
            id: sandbox.name.clone(),
            label: sandbox.name.clone(),
            kind: sandbox.pack_key.clone(),
            status: sandbox.status.clone(),
            ip: sandbox
                .runtime
                .as_ref()
                .map(|facts| facts.ip.clone())
                .unwrap_or_default(),
        });
        edges.push(TopologyEdge {
            source: "runtime".to_string(),
            target: sandbox.name.clone(),
            label: "runs".to_string(),
        });
    }

    for binding in &bindings {
        edges.push(TopologyEdge {
            source: binding.source.clone(),
            target: binding.target.clone(),
            label: format!("{}:{} · {}", binding.protocol, binding.port, binding.purpose),
        });
    }

    Topology { nodes, edges }
}
