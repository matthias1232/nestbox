# ADR 0009 — The event bus sits behind a trait

Status: Accepted
Date: 2026-10-26

## Context

The live console must show firewall, DNS, and model requests immediately, not
after a polling interval. Server-Sent Events over a broadcast channel give that
with one HTTP connection per client.

The constraint is that a broadcast bus is in-process. With more than one API
instance, a client connected to instance A never receives an event published on
instance B. M1 runs a single instance, so this is not a defect today, but it is
a wall the architecture would hit on the way to scale.

## Decision

The bus is an interface with a local implementation.

```
trait EventBus {
    fn publish(&self, event: LiveEvent);
    fn subscribe(&self) -> BroadcastStream;
}
```

- M1 ships `LocalBus`, backed by `tokio::sync::broadcast`.
- Callers depend on the trait, never on the concrete type.
- The SSE handler subscribes through the trait, so the transport is replaceable
  without touching any handler or any publisher.
- Backpressure is explicit: a slow subscriber is dropped with a typed error
  rather than silently losing events or growing memory without bound.

## Alternatives rejected

**PostgreSQL `LISTEN/NOTIFY` immediately.** Rejected for M1: it adds a
dedicated connection per subscriber, caps payload size, and introduces a
delivery latency and a failure mode that a single-instance deployment does not
need. It is the natural M2 implementation behind the same trait.

**Redis pub/sub immediately.** Rejected: adds a dependency and an operational
component for a property (multi-instance fan-out) that M1 does not yet have.

**Polling from the browser.** Rejected: the requirement is explicitly
real-time, not one-second-delayed. Polling also multiplies load with client
count.

**Write events only to the database and tail the table.** Rejected as the live
path: it makes the database the message broker, adds write amplification on a
hot path, and the latency target is not reachable without polling.

## Consequences

- A single API instance is a stated M1 limit, documented rather than implied.
- Telemetry still lands in the database (partitioned, per ADR 0010), so history
  survives a restart even though the live stream does not.
- Moving to multi-instance is a new `EventBus` implementation, not a refactor
  of callers. That is the entire point of the trait.
- A subscriber that cannot keep up is disconnected and reconnects with a
  cursor, so a slow client degrades itself rather than the server.

## What would invalidate this

- A requirement for exactly-once delivery of live events, which pub/sub does
  not give and which would need a durable log with offsets. The database
  already holds the durable record, so live delivery stays best-effort by
  design.
