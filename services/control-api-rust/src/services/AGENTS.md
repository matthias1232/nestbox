# services/ — business rules

## Purpose

The rules: what is allowed, what happens next, and in which order. A service takes
a principal, applies the permission check, calls the store, and returns a domain
value.

This layer exists so a rule has exactly one home. In the prototype a rule lived
partly in the handler and partly in the "state" module, and changing it meant
reading both.

## Dependencies

- May use: `domain`, `store` (through `TenantScope`), `runtime`, `events`,
  `security`, `error`
- Must not use: `axum::*`, HTTP extractors, `sqlx::*`

## Security rules

1. **The permission check lives here, not in the router.** A new route that
   forgets the check is the common failure, and a router-level check is the
   second line rather than the first.
2. Every function takes a `&Principal` and derives the environment from it. A
   caller cannot pass a different environment id.
3. A state change that can fail partway is a job, not a synchronous write.
4. Anything written to the audit trail goes through the store's `record`, which
   redacts.

## Conventions

- A function name is a verb phrase: `start_sandbox`, not `sandbox_start`.
- A service returns domain types or `ApiError`. It never returns a row.
- Rules that are pure (may this status transition?) live in `domain` so they can
  be unit-tested without a database. The service orchestrates.

## The egress decider

`policy_service::check` is the one place that answers whether an outbound request
from a sandbox may proceed. The pure rule (`domain::egress::decide`) stays in the
domain; the service loads what it needs and writes what it decided. Four rules
shape it:

1. **The caller is a node, not a user.** `authenticate_node` runs first, and
   `check` is never reachable without it. A node may only ask about a sandbox
   that runs on that node; the sandbox's `node_id` is compared to the asking
   node, and a mismatch is a deny.
2. **The tenant comes from the sandbox.** A node request carries no environment,
   so `store::postgres::egress::sandbox_scope` resolves it before a scope exists.
   That lookup walks the environments table and asks each candidate through a
   scoped transaction — no privileged read, so row-level security stays in force.
3. **One transaction, one truth.** Rules, windows, the compilation record, and
   the evidence row are read and written together. The row an operator sees later
   therefore describes the rule set that was actually applied.
4. **Fail closed, and say why.** A request without a sandbox, for an unknown
   sandbox, or from the wrong node is a deny with `default_deny`, an empty policy
   hash, and a cache TTL of zero; a database failure is a 5xx, which the proxy's
   `ext_authz` filter turns into a refusal on its own.

## Files

| File | Responsibility |
|---|---|
| `auth_service.rs` | Login, logout, session validation, principal building |
| `access_control.rs` | Permission evaluation and the refusal message |
| `sandbox_service.rs` | Lifecycle, placement, bulk actions |
| `provider_service.rs` | Provider profiles with their probe status, the pending profile an install creates, the in-use check on delete, and the reachable-only inference environment a sandbox receives |
| `agent_service.rs` | Agents inside a sandbox: which packs may join, homes and their persistence, the supervisor command |
| `policy_service.rs` | Validation, diff, scope rules, global override, and the egress decider (`check`: windows, rules, one evidence row; `authenticate_node`; `deployment`) |
| `telemetry_service.rs` | Ingest, live publish, history queries |
| `model_service.rs` | Hardware detection (CPU, RAM, GPU, unified pool), fit against a node's free budget, facets, catalog filters, installs and their reservations |
| `node_service.rs` | The nodes list, one node with its budget, a node's sandboxes, name-or-id resolution, display rename |
| `pack_service.rs` | Baselines, agent packs, and tool catalog as data; pack defaults by row, baseline compatibility, package-name resolution |
| `gateway_service.rs` | Registry, settings, drift, sync |
| `job_service.rs` | Job creation and progress |
| `schedule_service.rs` | Schedule evaluation and runs |
| `metrics_service.rs` | Sample ingest, ranged query, rollup, retention (`feat/sandbox-metrics`) |
| `inventory_service.rs` | Snapshot storage and age; never an environment value (`feat/sandbox-metrics`) |
| `access_service.rs` | SSH key pair as a file with a locator, port pool, sessions (`feat/sandbox-access`) |
| `mcp_service.rs` | Registry, attachments, discovery through the policy check (`feat/mcp-registry`) |
| `persist_service.rs` | Persistent roots: derived host paths, mounts, the one-read-write rule, honest usage |

## Persistence rules

1. A mount source is never taken from a request. `persist_service` derives the
   host path from a validated slug plus the caller's user id, and that derived
   path is the only source the runtime ever receives.
2. `sandbox_service::create` reads a sandbox's mounts from the database, inside
   the transaction that inserted the sandbox row, and passes them to the runtime.
3. A bind mount is fixed when the container is created. A mount or dismount
   changes the row, and therefore what the *next* container for that sandbox
   receives; it does not rewire a running container.
4. Archive, never delete, by default. Purging removes the record only: the
   directory lives on the node, which the control plane cannot reach.

## Extending this module

Add the rule here first, as a pure function in `domain` if possible, and only
then wire it to the store. If the rule needs a permission, add the permission to
`domain::auth` before writing the check, so the check cannot reference a
permission that does not exist.
