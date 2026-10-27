//! Server-Sent Events.
//!
//! The stream is the reason the console feels live. It is also a place where a
//! tenant filter is easy to forget, so the filter is applied here, at the last
//! point before the socket, regardless of what the publisher believed.

use std::convert::Infallible;

use axum::extract::State;
use axum::response::Sse;
use axum::response::sse::{Event, KeepAlive};
use futures::stream::Stream;
use tokio::sync::broadcast::error::RecvError;

use crate::api::principal::Auth;
use crate::domain::telemetry::LiveEvent;
use crate::error::ApiError;
use crate::events::bus::is_visible_to;
use crate::state::AppState;

/// Whether a stream should carry telemetry rows or only state changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StreamFilter {
    Telemetry,
    State,
}

/// The telemetry stream: network, DNS, and inference rows.
pub async fn telemetry_stream(
    State(state): State<AppState>,
    auth: Auth,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, ApiError> {
    crate::services::require(
        &auth.principal,
        crate::domain::auth::Permission::TelemetryRead,
    )?;
    Ok(build_stream(
        state.bus().clone(),
        auth.principal.environment_id,
        StreamFilter::Telemetry,
    ))
}

/// The state stream: sandbox, gateway, job, and rollout changes.
pub async fn state_stream(
    State(state): State<AppState>,
    auth: Auth,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, ApiError> {
    crate::services::require(
        &auth.principal,
        crate::domain::auth::Permission::SandboxRead,
    )?;
    Ok(build_stream(
        state.bus().clone(),
        auth.principal.environment_id,
        StreamFilter::State,
    ))
}

fn build_stream(
    bus: std::sync::Arc<dyn crate::events::EventBus>,
    environment_id: uuid::Uuid,
    filter: StreamFilter,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let receiver = bus.subscribe();

    // `unfold` keeps the receiver as the stream's state, so each poll resumes
    // where the last one stopped. A lagged subscriber skips ahead rather than
    // being disconnected, because a browser that missed a burst is better served
    // by the next event than by a reconnect loop.
    let stream = futures::stream::unfold(receiver, move |mut receiver| async move {
        loop {
            match receiver.recv().await {
                Ok(event) => {
                    if !is_visible_to(&event, environment_id) {
                        continue;
                    }
                    if !matches_filter(&event, filter) {
                        continue;
                    }
                    let name = event_name(&event);
                    match serde_json::to_string(&event) {
                        Ok(data) => {
                            return Some((Ok(Event::default().event(name).data(data)), receiver));
                        }
                        Err(_) => continue,
                    }
                }
                Err(RecvError::Lagged(skipped)) => {
                    tracing::warn!(skipped, "sse subscriber lagged; skipping ahead");
                    continue;
                }
                Err(RecvError::Closed) => return None,
            }
        }
    });

    Sse::new(stream).keep_alive(
        // A comment frame every 15 s keeps an idle connection alive through
        // proxies that drop silent sockets.
        KeepAlive::new().interval(std::time::Duration::from_secs(15)),
    )
}

/// The event name, which is part of the published contract (ADR 0015).
fn event_name(event: &LiveEvent) -> &'static str {
    match event {
        LiveEvent::Telemetry(inner) => inner.kind().stream_name(),
        LiveEvent::SandboxStateChanged { .. } => "sandbox",
        LiveEvent::GatewayHealthChanged { .. } => "gateway",
        LiveEvent::JobProgress { .. } => "job",
        LiveEvent::PolicyRollout { .. } => "policy_rollout",
    }
}

fn matches_filter(event: &LiveEvent, filter: StreamFilter) -> bool {
    match filter {
        StreamFilter::Telemetry => matches!(event, LiveEvent::Telemetry(_)),
        StreamFilter::State => !matches!(event, LiveEvent::Telemetry(_)),
    }
}

/// A helper for the console: read one batch of events and close.
///
/// Useful behind a proxy that buffers, and for a smoke test that must prove the
/// stream produces anything at all.
pub async fn drain_once(
    state: &AppState,
    environment_id: uuid::Uuid,
    limit: usize,
) -> Vec<LiveEvent> {
    let mut receiver = state.bus().subscribe();
    let mut out = Vec::new();
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(2);
    while out.len() < limit {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            break;
        }
        match tokio::time::timeout(remaining, receiver.recv()).await {
            Ok(Ok(event)) if is_visible_to(&event, environment_id) => out.push(event),
            Ok(Ok(_)) => continue,
            Ok(Err(_)) => break,
            Err(_) => break,
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn state_event(environment_id: Uuid) -> LiveEvent {
        LiveEvent::SandboxStateChanged {
            sandbox_id: Uuid::new_v4(),
            environment_id,
            status: "running".into(),
            version: 2,
        }
    }

    #[test]
    fn the_state_filter_excludes_telemetry() {
        let event = state_event(Uuid::nil());
        assert!(matches_filter(&event, StreamFilter::State));
        assert!(!matches_filter(&event, StreamFilter::Telemetry));
    }

    #[test]
    fn event_names_match_the_published_contract() {
        assert_eq!(event_name(&state_event(Uuid::nil())), "sandbox");
        let gateway = LiveEvent::GatewayHealthChanged {
            gateway_id: Uuid::nil(),
            environment_id: Uuid::nil(),
            status: "healthy".into(),
            drifted: false,
        };
        assert_eq!(event_name(&gateway), "gateway");
    }
}
