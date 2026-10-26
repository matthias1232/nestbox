//! Fachliche Typen der Control Plane.
//!
//! Diese Module kennen kein HTTP und keinen Docker-Socket. Sie beschreiben,
//! was eine Sandbox, ein Provider, ein Pack oder ein Binding ist.

pub mod binding;
pub mod pack;
pub mod provider;
pub mod runtime;
pub mod sandbox;
pub mod tool_profile;
pub mod topology;

pub use binding::{Binding, BindingInput};
pub use pack::Pack;
pub use provider::{ProviderInput, ProviderProfile};
pub use runtime::{RuntimeFacts, SandboxSpec};
pub use sandbox::{LifecycleAction, Sandbox, SandboxInput, SandboxLogs};
pub use tool_profile::{ToolDefinition, ToolProfile, ToolProfileInput};
pub use topology::{Topology, TopologyEdge, TopologyNode};
