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
pub mod inventory_service;
pub mod job_service;
pub mod mcp_service;
pub mod media_service;
pub mod metrics_service;
pub mod model_service;
pub mod node_service;
pub mod pack_service;
pub mod persist_service;
pub mod policy_service;
pub mod privacy_service;
pub mod provider_service;
pub mod sandbox_service;
pub mod selection;
pub mod session_service;
pub mod secret_service;
pub mod stack_topology_service;
pub mod telemetry_service;
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
        }
    }
}
