//! The live event bus.

use tokio::sync::broadcast;
use uuid::Uuid;

use crate::domain::telemetry::LiveEvent;

/// Capacity of the broadcast channel.
///
/// A slow subscriber that falls further behind than this is disconnected rather
/// than silently losing events or growing memory. 1024 events is a few seconds
/// of a busy firewall feed, which is a reasonable grace period for a browser
/// that hit a garbage collection pause.
const CHANNEL_CAPACITY: usize = 1024;

/// Why a subscription ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamEnd {
    /// The subscriber could not keep up and was dropped.
    Lagged,
    /// The bus shut down.
    Closed,
}

/// The bus interface.
///
/// Callers depend on this trait, never on the concrete type, so the transport
/// can change without touching a publisher or a handler.
pub trait EventBus: Send + Sync {
    /// Publish an event. Returns how many subscribers received it.
    fn publish(&self, event: LiveEvent) -> usize;

    /// Subscribe to the live feed.
    fn subscribe(&self) -> broadcast::Receiver<LiveEvent>;
}

/// The in-process implementation.
#[derive(Debug, Clone)]
pub struct LocalBus {
    sender: broadcast::Sender<LiveEvent>,
}

impl LocalBus {
    /// Create a bus.
    pub fn new() -> Self {
        let (sender, _) = broadcast::channel(CHANNEL_CAPACITY);
        Self { sender }
    }
}

impl Default for LocalBus {
    fn default() -> Self {
        Self::new()
    }
}

impl EventBus for LocalBus {
    fn publish(&self, event: LiveEvent) -> usize {
        // A send with no subscribers is not an error: it means nobody is
        // watching, which is the normal state between page loads.
        self.sender.send(event).unwrap_or(0)
    }

    fn subscribe(&self) -> broadcast::Receiver<LiveEvent> {
        self.sender.subscribe()
    }
}

/// Whether an event belongs to the environment a client is allowed to see.
///
/// This is the filter that makes a stream safe. It is applied on the way out,
/// not trusted from the publisher, because a publisher that reports events for
/// many environments is the normal case during gateway ingestion.
pub fn is_visible_to(event: &LiveEvent, environment_id: Uuid) -> bool {
    event.environment_id() == environment_id
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::telemetry::{EventKind, TelemetryEvent};
    use uuid::Uuid;

    fn sample(environment_id: Uuid) -> LiveEvent {
        LiveEvent::Telemetry(TelemetryEvent::Network(
            crate::domain::telemetry::NetworkEvent {
                id: Uuid::new_v4(),
                environment_id,
                sandbox_id: None,
                gateway_id: None,
                ts: chrono::Utc::now(),
                direction: crate::domain::telemetry::Direction::Outbound,
                protocol: "https".into(),
                source: "10.244.0.2".into(),
                destination: "api.example.com".into(),
                port: Some(443),
                binary: None,
                decision: crate::domain::telemetry::Decision::Allow,
                rule_id: None,
                rule_name: None,
                latency_ms: None,
                correlation_id: None,
                enforcement: Some("enforced".into()),
            },
        ))
    }

    #[test]
    fn a_published_event_reaches_a_subscriber() {
        let bus = LocalBus::new();
        let mut receiver = bus.subscribe();
        let env = Uuid::new_v4();
        assert_eq!(bus.publish(sample(env)), 1);
        let received = receiver.try_recv().expect("received");
        assert_eq!(received.environment_id(), env);
    }

    #[test]
    fn publishing_without_subscribers_is_not_an_error() {
        // Between page loads nobody is watching, which must not be a failure.
        let bus = LocalBus::new();
        assert_eq!(bus.publish(sample(Uuid::new_v4())), 0);
    }

    #[test]
    fn an_event_from_another_environment_is_not_visible() {
        // The filter that makes a stream tenant-safe.
        let mine = Uuid::new_v4();
        let theirs = Uuid::new_v4();
        assert!(is_visible_to(&sample(mine), mine));
        assert!(!is_visible_to(&sample(theirs), mine));
    }

    #[test]
    fn the_kind_of_a_telemetry_event_is_preserved() {
        let event = sample(Uuid::new_v4());
        let LiveEvent::Telemetry(inner) = &event else {
            panic!("expected telemetry");
        };
        assert_eq!(inner.kind(), EventKind::Network);
    }
}
