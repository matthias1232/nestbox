//! The interactive terminal: a WebSocket from the browser to a PTY in the
//! sandbox, proxied through the control API.
//!
//! The route is a WebSocket, not a request/response pair, so it is documented
//! with `x-stream: ws`. The control API is the only place the session is
//! recorded: who opened it, when, for how long, and how many bytes crossed —
//! never what was typed or printed. The `feat/terminal` package fills the body.

use axum::extract::{Path, State, WebSocketUpgrade};
use axum::response::Response;
use uuid::Uuid;

use crate::api::principal::Auth;
use crate::error::ApiError;
use crate::state::AppState;

/// Attach a terminal to a sandbox.
///
/// Deliberately returns `501` until the runtime can open a PTY: a WebSocket that
/// upgrades and then says nothing is worse than a clear refusal.
pub async fn terminal(
    State(_state): State<AppState>,
    _auth: Auth,
    Path(_id): Path<Uuid>,
    _upgrade: WebSocketUpgrade,
) -> Result<Response, ApiError> {
    Err(ApiError::not_implemented("GET /sandboxes/{id}/terminal"))
}
