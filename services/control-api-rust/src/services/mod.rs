//! Business rules.

pub mod access_control;
pub mod audit_service;
pub mod auth_service;
pub mod environment_service;
pub mod gateway_service;
pub mod job_service;
pub mod model_service;
pub mod policy_service;
pub mod sandbox_service;
pub mod telemetry_service;

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
}

impl ServiceContext {
    /// Build a context from its parts.
    pub fn new(
        pool: sqlx::PgPool,
        runtime: Arc<dyn RuntimeAdapter>,
        bus: Arc<dyn EventBus>,
    ) -> Self {
        Self { pool, runtime, bus }
    }
}
