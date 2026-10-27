//! Application state.
//!
//! One struct, assembled at start and cloned per request. It holds handles, not
//! logic: a method on this type that decides something would be a rule living
//! outside `services/`.

use std::sync::Arc;

use crate::config::Config;
use crate::events::bus::{EventBus, LocalBus};
use crate::runtime::RuntimeAdapter;
use crate::services::ServiceContext;

/// Shared state for every request.
#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub services: ServiceContext,
}

impl AppState {
    /// Build the state from its parts.
    pub fn new(config: Config, runtime: Arc<dyn RuntimeAdapter>, pool: sqlx::PgPool) -> Self {
        let bus: Arc<dyn EventBus> = Arc::new(LocalBus::new());
        let secret_dir = config.secret_dir.clone();
        Self {
            services: ServiceContext::new(pool, runtime, bus, secret_dir),
            config: Arc::new(config),
        }
    }

    /// The database pool.
    pub fn pool(&self) -> &sqlx::PgPool {
        &self.services.pool
    }

    /// The event bus.
    pub fn bus(&self) -> &Arc<dyn EventBus> {
        &self.services.bus
    }
}
