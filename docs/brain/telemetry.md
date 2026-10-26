# Telemetry

## Three streams, one shape

The console shows three live feeds, because they answer three different
questions:

| Stream | Question it answers | Source |
|---|---|---|
| Network | What connections is this sandbox making, and what was decided | Gateway OCSF export |
| DNS | What names is it resolving, and were they allowed | Gateway DNS log |
| Inference | Which model requests, to which provider, with what result | Provider resolution at the gateway |

All three share a shape: time, sandbox, decision, and a correlation id that ties
related events together across streams.

## Why real-time matters

The requirement is explicit: no one-second delay. Polling would mean the console
shows a firewall decision after the operator already acted on stale information.

The path is:

```
gateway OCSF log
  → ingest (batched)
      ├── PostgreSQL (partitioned, history)
      └── event bus (immediate)
              → SSE → browser
```

The database write and the live publish are independent. A slow database does
not delay the stream, and a disconnected browser does not lose history.

## Why the history exists

The live stream is ephemeral by design (ADR 0009). History is what lets an
operator answer "what happened at 14:02" and "which destinations did this
sandbox attempt last week". That is why telemetry also lands in a partitioned
table (ADR 0010), and why retention is a partition drop rather than a delete.

## Columns an operator expects

The console mirrors what a firewall console shows, because the mental model
should be familiar:

time · direction · protocol · source · destination · port · decision · rule id ·
latency · correlation id

Plus two that matter for this product specifically: the **binary** that made the
attempt (OpenShell attributes connections to an executable, and a script calling
`curl` is not `curl`), and the **enforcement mode** (enforced or audit), because
an audit-mode block is a warning, not a wall.

## Acting on an event

From a row, an operator can:

- create a rule draft (never an immediate rule, per ADR 0011),
- open the sandbox that produced it,
- filter the stream to that sandbox, destination, or binary,
- pause the stream to read without losing the buffer.

Bulk selection creates drafts in bulk, and the combined diff is shown before
approval.

## The temporary window

For an agent that needs to work right now, an operator can open traffic for a
bounded number of minutes. The window has an expiry, a reason, and the operator
who opened it. When it expires, the previous policy applies again. The countdown
is visible, and the expiry is enforced server-side, so closing the browser does
not leave traffic open.

This is deliberate: it is the safe version of "just turn the firewall off for a
minute", because it ends on its own and it is recorded.

## Where this lives in the code

| Concern | File |
|---|---|
| Event types | `services/control-api-rust/src/domain/telemetry.rs` |
| Ingest and batching | `services/control-api-rust/src/services/telemetry_ingest.rs` |
| Live bus | `services/control-api-rust/src/events/bus.rs` |
| SSE transport | `services/control-api-rust/src/events/stream.rs` |
| SQL | `services/control-api-rust/src/store/postgres/telemetry_events.rs` |
| Console | `services/web-console-solid/src/features/traffic/` |
