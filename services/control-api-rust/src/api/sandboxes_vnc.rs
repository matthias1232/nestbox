//! The sandbox desktop: a WebSocket from the browser (noVNC) to the VNC server
//! inside the sandbox, carried through the control API and the runner.
//!
//! Like the terminal route it checks the origin, then the permission and the
//! sandbox state *before* the upgrade, and then only moves frames. It reads no
//! pixel and records no screen; the audit line says who attached to which
//! sandbox and for how long.

use axum::extract::ws::{Message as ClientMessage, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::response::Response;
use futures::{SinkExt, StreamExt};
use tokio_tungstenite::tungstenite::Message as RunnerMessage;
use uuid::Uuid;

use crate::api::principal::Auth;
use crate::api::sandboxes_terminal::refuse_foreign_origin;
use crate::error::ApiError;
use crate::services::metrics_service;
use crate::state::AppState;

/// noVNC offers `binary`; answering with it keeps the handshake legal.
const SUBPROTOCOL: &str = "binary";

/// Attach the browser to the sandbox's desktop.
pub async fn desktop(
    State(state): State<AppState>,
    auth: Auth,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
    upgrade: WebSocketUpgrade,
) -> Result<Response, ApiError> {
    refuse_foreign_origin(&state, &auth, &headers)?;
    let name = metrics_service::desktop_target(&state.services, &auth.principal, id).await?;
    let runner = state
        .services
        .runtime
        .open_vnc(&name)
        .await
        .map_err(crate::services::metrics_service::runner_error)?;
    let who = auth.principal.display_name.clone();
    Ok(upgrade.protocols([SUBPROTOCOL]).on_upgrade(move |socket| async move {
        tracing::info!(sandbox = %name, user = %who, "desktop session opened");
        let started = std::time::Instant::now();
        pump(socket, runner).await;
        tracing::info!(sandbox = %name, user = %who, seconds = started.elapsed().as_secs(), "desktop session closed");
    }))
}

/// Move frames both ways until either side ends.
async fn pump(browser: WebSocket, runner: crate::runtime::VncStream) {
    let (mut to_browser, mut from_browser) = browser.split();
    let (mut to_runner, mut from_runner) = runner.split();
    let down = async {
        while let Some(Ok(message)) = from_runner.next().await {
            let forwarded = match message {
                RunnerMessage::Binary(bytes) => ClientMessage::Binary(bytes.to_vec().into()),
                RunnerMessage::Close(_) => break,
                _ => continue,
            };
            if to_browser.send(forwarded).await.is_err() {
                break;
            }
        }
    };
    let up = async {
        while let Some(Ok(message)) = from_browser.next().await {
            let forwarded = match message {
                ClientMessage::Binary(bytes) => RunnerMessage::Binary(bytes.to_vec().into()),
                ClientMessage::Close(_) => break,
                _ => continue,
            };
            if to_runner.send(forwarded).await.is_err() {
                break;
            }
        }
    };
    tokio::select! { _ = down => {}, _ = up => {} }
}
