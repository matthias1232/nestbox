//! Startpunkt des Service: Konfiguration, Adapter, State, HTTP-Server.

use std::sync::Arc;

use nestbox_control_api::config::Config;
use nestbox_control_api::runtime::http_runner::HttpRunner;
use nestbox_control_api::{app, build};

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,tower_http=warn".into()),
        )
        .init();

    let config = Config::from_env();
    tracing::info!(
        state = %config.state_path.display(),
        runner = %config.runner_url,
        "Nestbox Control API startet"
    );

    // Heute: Docker über den Sandbox-Runner. Später: OpenShell-Adapter,
    // ohne dass sich an der API oder der GUI etwas ändert.
    let runtime = Arc::new(HttpRunner::new(
        config.runner_url.clone(),
        config.runtime_network.clone(),
    ));

    let state = match build(config.clone(), runtime).await {
        Ok(state) => state,
        Err(error) => {
            tracing::error!("Start fehlgeschlagen: {error}");
            std::process::exit(1);
        }
    };

    let listener = tokio::net::TcpListener::bind(&config.listen_addr)
        .await
        .expect("Listen-Adresse ist belegbar");
    tracing::info!("API lauscht auf http://{}", config.listen_addr);

    axum::serve(listener, app(state))
        .await
        .expect("Server läuft bis zum Shutdown");
}
