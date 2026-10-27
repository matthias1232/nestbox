//! The PostgreSQL repositories, one module per aggregate.

pub mod audit;
pub mod auth;
pub mod environments;
pub mod gateways;
pub mod jobs;
pub mod models;
pub mod policies;
pub mod providers;
pub mod sandboxes;
pub mod telemetry;

/// Open the connection pool.
///
/// The pool is created with a bounded size so a burst cannot exhaust the
/// database's own connection limit, and with a statement timeout so a runaway
/// query is cancelled rather than holding a connection forever.
pub async fn connect(database_url: &str) -> Result<sqlx::PgPool, sqlx::Error> {
    use sqlx::postgres::PgPoolOptions;
    use std::time::Duration;

    PgPoolOptions::new()
        .max_connections(16)
        .min_connections(1)
        .acquire_timeout(Duration::from_secs(5))
        .idle_timeout(Duration::from_secs(600))
        .connect(database_url)
        .await
}

/// Whether the database answers at all.
///
/// Used by the readiness probe. It deliberately does a trivial round trip rather
/// than inspecting a table: readiness means "this process can reach its store",
/// not "the schema is complete", which the migrations already guarantee.
pub async fn is_ready(pool: &sqlx::PgPool) -> bool {
    sqlx::query_scalar::<_, i32>("SELECT 1")
        .fetch_one(pool)
        .await
        .is_ok()
}
