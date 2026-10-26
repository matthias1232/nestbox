//! OpenAPI-Dokument – automatisch aus den Typen und Pfaden erzeugt.
//!
//! Neue Endpunkte werden in `paths(...)` ergänzt, neue Typen in
//! `components(schemas(...))`. Das Dokument ist damit immer im Gleichschritt
//! mit dem Code; es gibt keinen zweiten, handgepflegten Vertrag.

use utoipa::OpenApi;

use crate::audit::AuditEvent;
use crate::api;
use crate::domain::binding::{Binding, BindingInput};
use crate::domain::pack::Pack;
use crate::domain::provider::{ProviderInput, ProviderProfile};
use crate::domain::runtime::{NetworkInterface, RuntimeFacts, RuntimeResources};
use crate::domain::sandbox::{LifecycleAction, LifecycleRequest, Sandbox, SandboxInput, SandboxLogs};
use crate::domain::tool_profile::{ToolDefinition, ToolProfile, ToolProfileInput};
use crate::domain::topology::{Topology, TopologyEdge, TopologyNode};
use crate::error::{ErrorBody, ErrorEnvelope};

/// Der vollständige API-Vertrag von v1.
#[derive(OpenApi)]
#[openapi(
    info(
        title = "Nestbox Control API",
        version = "1.0.0",
        description = "API-first Control Plane der Nestbox-Appliance: Sandboxes, Provider, Tool-Profile, Bindings und Topologie. Jede Aktion der Web-GUI nutzt diesen Vertrag."
    ),
    paths(
        api::system::health,
        api::system::runtime_status,
        api::system::events,
        api::system::schema,
        api::packs::list,
        api::providers::list,
        api::providers::create,
        api::tools::list,
        api::tools::create,
        api::tools::catalog,
        api::sandboxes::list,
        api::sandboxes::get,
        api::sandboxes::create,
        api::sandboxes::lifecycle,
        api::sandboxes::logs,
        api::bindings::list,
        api::bindings::create,
        api::topology::get,
        api::reference::deploy_reference,
    ),
    components(schemas(
        Sandbox,
        SandboxInput,
        SandboxLogs,
        LifecycleAction,
        LifecycleRequest,
        ProviderProfile,
        ProviderInput,
        ToolProfile,
        ToolProfileInput,
        ToolDefinition,
        Pack,
        Binding,
        BindingInput,
        Topology,
        TopologyNode,
        TopologyEdge,
        RuntimeFacts,
        NetworkInterface,
        RuntimeResources,
        AuditEvent,
        api::system::HealthInfo,
        api::system::RuntimeStatus,
        ErrorEnvelope,
        crate::services::reference::DeploymentReport,
        crate::services::reference::DeploymentStep,
        ErrorBody,
    )),
    tags(
        (name = "system", description = "Health, Runtime-Status, Audit und Schema"),
        (name = "sandboxes", description = "Sandboxes, Packs und Lifecycle"),
        (name = "providers", description = "OpenAI-kompatible Provider-Profile"),
        (name = "tools", description = "Tool-Profile und Werkzeugkatalog"),
        (name = "topology", description = "Bindings und Netzwerkbild"),
    )
)]
pub struct ApiDoc;
