//! Business rules.

pub mod access_control;
pub mod access_service;
pub mod accounting_service;
pub mod agent_service;
pub mod audit_service;
pub mod backup_service;
pub mod auth_service;
pub mod builder_service;
pub mod catalog_sync;
pub mod catalogue_service;
pub mod cudax_service;
pub mod environment_service;
pub mod gateway_service;
pub mod geography_service;
pub mod image_sync_service;
pub mod inventory_service;
pub mod job_service;
pub mod mcp_service;
pub mod move_service;
pub mod node_join_service;
pub mod oidc_service;
pub mod media_service;
pub mod metrics_service;
pub mod model_service;
pub mod node_service;
pub mod pack_service;
pub mod persist_service;
pub mod policy_service;
pub mod privacy_service;
pub mod provider_service;
pub mod proxy_service;
pub mod sandbox_service;
pub mod selection;
pub mod session_service;
pub mod secret_service;
pub mod stack_topology_service;
pub mod telemetry_service;
pub mod tenancy_service;
pub mod template_service;
pub mod terminal_service;
pub mod user_service;
pub mod traffic_service;

pub use access_control::require;

use std::sync::Arc;

use crate::events::bus::EventBus;
use crate::runtime::RuntimeAdapter;

/// Everything a service needs, passed explicitly.
///
/// Deliberately not the HTTP state type: a service that can reach the router
/// will eventually use it, and then the layers are one layer.
#[derive(Clone)]
pub struct ServiceContext {
    pub pool: sqlx::PgPool,
    pub runtime: Arc<dyn RuntimeAdapter>,
    pub bus: Arc<dyn EventBus>,
    /// Where `file://` secret locators resolve. Passed explicitly so a service
    /// never reads a global configuration and a test can point it at a temp
    /// directory.
    pub secret_dir: std::path::PathBuf,
    /// The dispatcher in front of every node's runner, when this process has one. `runtime`
    /// is the same object seen through the trait; this handle is for the calls that must name
    /// a node ("measure that machine") instead of a sandbox.
    pub router: Option<Arc<crate::runtime::router::RuntimeRouter>>,
}

impl ServiceContext {
    /// Build a context from its parts.
    pub fn new(
        pool: sqlx::PgPool,
        runtime: Arc<dyn RuntimeAdapter>,
        bus: Arc<dyn EventBus>,
        secret_dir: std::path::PathBuf,
    ) -> Self {
        Self {
            pool,
            runtime,
            bus,
            secret_dir,
            router: None,
        }
    }

    /// The same context with a dispatcher; `runtime` becomes the dispatcher.
    pub fn with_router(mut self, router: Arc<crate::runtime::router::RuntimeRouter>) -> Self {
        self.runtime = router.clone();
        self.router = Some(router);
        self
    }

    /// The runner of one node. Without a dispatcher there is one runner and it is every
    /// node's, which is what a single-machine install and the unit tests are.
    pub fn runtime_for(&self, node: uuid::Uuid) -> Arc<dyn RuntimeAdapter> {
        match &self.router {
            Some(router) => router.for_node(Some(node)),
            None => self.runtime.clone(),
        }
    }
}
