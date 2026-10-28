//! Process start: configuration, pool, migrations, runtime adapter, serve.
//!
//! This file contains no rules. If it grows an `if` that decides something about
//! a sandbox, that decision belongs in `services/`.

use std::sync::Arc;

use nestbox_control_api::config::Config;
use nestbox_control_api::runtime::RuntimeAdapter;
use nestbox_control_api::runtime::http_runner::HttpRunner;
use nestbox_control_api::security::password;
use nestbox_control_api::state::AppState;
use nestbox_control_api::{app, store};

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,tower_http=warn".into()),
        )
        .init();

    let config = match Config::from_env() {
        Ok(config) => config,
        Err(message) => {
            // A configuration error must name the missing variable, not fail
            // later with a 500 on the first request.
            tracing::error!("configuration error: {message}");
            std::process::exit(2);
        }
    };
    tracing::info!(summary = %config.summary(), "starting nestbox control api");

    let pool = match store::postgres::connect(&config.database_url).await {
        Ok(pool) => pool,
        Err(error) => {
            tracing::error!(error = %error, "cannot connect to the database");
            std::process::exit(2);
        }
    };

    // `migrate` applies the schema and exits, so the API never races a migration.
    if std::env::args().any(|arg| arg == "migrate") {
        if let Err(error) = migrate(&pool, &config).await {
            tracing::error!(error = %error, "migration failed");
            std::process::exit(1);
        }
        tracing::info!("migrations complete");
        return;
    }

    let runtime: Arc<dyn RuntimeAdapter> = Arc::new(HttpRunner::new(
        config.runner_url.clone(),
        config.runtime_network.clone(),
    ));

    let state = AppState::new(config.clone(), runtime, pool);

    // A build that was running when the process died leaves its recipe
    // `building` and its job `running`. Neither will ever finish, so they are
    // settled before the first request can see them.
    match nestbox_control_api::services::builder_service::settle_interrupted(&state.services).await {
        Ok(0) => {}
        Ok(settled) => tracing::info!(settled, "settled builds interrupted by a restart"),
        Err(error) => tracing::warn!(code = error.code.0, "could not settle interrupted builds"),
    }

    let listener = match tokio::net::TcpListener::bind(&config.listen_addr).await {
        Ok(listener) => listener,
        Err(error) => {
            tracing::error!(address = %config.listen_addr, error = %error, "cannot bind");
            std::process::exit(2);
        }
    };
    tracing::info!(address = %config.listen_addr, "listening");

    if let Err(error) = axum::serve(listener, app(state))
        .with_graceful_shutdown(shutdown_signal())
        .await
    {
        tracing::error!(error = %error, "server stopped");
    }
}

/// Apply migrations, then bootstrap the administrator if the sentinel is still
/// in place.
async fn migrate(pool: &sqlx::PgPool, config: &Config) -> Result<(), String> {
    sqlx::migrate!("./migrations")
        .run(pool)
        .await
        .map_err(|error| error.to_string())?;

    if config.bootstrap_admin {
        bootstrap_admin(pool).await.map_err(|error| error.to_string())?;
    }
    Ok(())
}

/// Replace the `!bootstrap` sentinel with a real password, printed once.
///
/// Printing a credential is normally wrong. It is acceptable exactly here: the
/// value is generated now, shown once on the operator's own terminal, and never
/// retrievable again. The alternative — a default password in a file — is worse.
async fn bootstrap_admin(pool: &sqlx::PgPool) -> Result<(), String> {
    const EMAIL: &str = "admin@nestbox.local";

    let generated = generate_password();
    let hash = password::hash_password(generated.clone()).await?;

    let changed = store::postgres::auth::set_password_if_bootstrap(pool, EMAIL, &hash)
        .await
        .map_err(|error| error.to_string())?;

    if changed {
        tracing::warn!("======================================================");
        tracing::warn!("bootstrap administrator created");
        tracing::warn!("email:    {EMAIL}");
        tracing::warn!("password: {generated}");
        tracing::warn!("this password is shown once and is not recoverable");
        tracing::warn!("======================================================");
    }
    Ok(())
}

/// A readable random password.
///
/// Hex rather than base64 so it survives a copy-paste and a phone keyboard
/// without ambiguity, at the cost of four extra characters for the same entropy.
fn generate_password() -> String {
    use rand::RngCore;
    let mut bytes = [0u8; 18];
    rand::thread_rng().fill_bytes(&mut bytes);
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// Wait for a termination signal so a restart does not drop in-flight requests.
async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };

    #[cfg(unix)]
    let terminate = async {
        if let Ok(mut signal) =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        {
            signal.recv().await;
        }
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
    tracing::info!("shutdown signal received");
}
