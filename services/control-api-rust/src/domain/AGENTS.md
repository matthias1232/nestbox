# domain/ — pure types

## Purpose

The vocabulary of the system: sandboxes, policies, gateways, telemetry events,
and the validation that belongs to them. A type here is what a business rule is
written against.

This layer exists so that a rule can be expressed without a database and tested
without a network. It is the cheapest layer to change and the one everything
else depends on.

## Dependencies

- May use: the standard library, `serde`, `chrono`, `uuid`, `utoipa`
- Must not use: `sqlx`, `axum`, `reqwest`, `tokio`, `async fn`

The absence of `async` is deliberate. A domain type that awaits something has
acquired an execution model, and the reason this layer is testable in
microseconds is that it has none.

## Security rules

1. Validation lives here, next to the type. A `SandboxName` that validates in
   the API layer can be constructed invalid from the service layer.
2. No secret values. A type may hold a `secret_ref` (a locator); it may never
   hold a key, token, or password.
3. String length limits are enforced in the constructor, so an unbounded string
   cannot reach a query or a log.

## Conventions

- A constructor validates. `SandboxName::new(...)` returns `Result`, and the
  inner field is private.
- Enums, not stringly-typed states. A status is a Rust enum with an explicit
  database representation.
- No invariant is left to the caller when the type can carry it.

## Files

| File | Responsibility |
|---|---|
| `sandbox.rs` | Sandbox, status, health, spec, version |
| `gateway.rs` | Gateway, status, settings drift |
| `supervisor.rs` | Supervisor and its reported state |
| `policy.rs` | The four-section document and its validation |
| `network_rule.rs` | A network rule and its enforcement mode |
| `telemetry.rs` | Network, DNS, and inference events |
| `job.rs` | Job, status, progress |
| `model.rs` | Model, hardware profile, fit classification, resource bars |
| `model_install.rs` | One model on one node; reservation rule |
| `node.rs` | Node, kind, status, budget (total / reserved / free) |
| `pack.rs` | Baseline, agent pack, tool, package-manager resolution |
| `builder.rs` | Recipe, agent spec, home mode, persistence, resource limits |
| `persist.rs` | Persistent root, mount, mount spec, path derivation |
| `pagination.rs` | Cursor, filter, sort, facet types |
| `auth.rs` | User, role, permission |
| `metrics.rs` | `docker stats` sample, one-minute bucket, window with its point budget |
| `access.rs` | SSH key record (locator, never material), published port, inventory, session |
| `egress.rs` | The pure `decide` function, compiled rule set, traffic event, map edge |
| `mcp.rs` | MCP server definition, sandbox attachment, tool call, JSON-RPC recognition |

## Extending this module

Add the type with a validating constructor, then the error variants it can
produce, then a unit test that constructs it valid *and* invalid. If the type
needs a new state, add the enum variant and the database representation in the
same change, and update the `CHECK` constraint in the migration.
