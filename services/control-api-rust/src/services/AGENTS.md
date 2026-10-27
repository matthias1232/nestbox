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

## Files

| File | Responsibility |
|---|---|
| `auth_service.rs` | Login, logout, session validation, principal building |
| `access_control.rs` | Permission evaluation and the refusal message |
| `sandbox_service.rs` | Lifecycle, placement, bulk actions |
| `provider_service.rs` | Provider profiles, probe, and the inference environment a sandbox receives |
| `policy_service.rs` | Validation, diff, scope rules, global override |
| `telemetry_service.rs` | Ingest, live publish, history queries |
| `model_service.rs` | Node hardware probing through the runner (CPU, RAM, GPU, unified pool), fit, facets, catalog filters, install jobs |
| `gateway_service.rs` | Registry, settings, drift, sync |
| `job_service.rs` | Job creation and progress |
| `schedule_service.rs` | Schedule evaluation and runs |

## Extending this module

Add the rule here first, as a pure function in `domain` if possible, and only
then wire it to the store. If the rule needs a permission, add the permission to
`domain::auth` before writing the check, so the check cannot reference a
permission that does not exist.

## Probing a node

The hardware probe runs on the node, through its runner: `ctx.runtime.hardware()`
returns raw `/proc` and `nvidia-smi` text, and the parsers in `model_service.rs`
turn it into a `HardwareProfile` for that node's id. Measuring from the control
plane would describe the control plane's container and call it the machine.

Two rules follow from that, and both are in `model_service.rs`:

1. **A failed probe invents nothing.** No profile is written, the node is marked
   offline (runner unreachable) or unknown (runner answered with an error), and
   the previous measurement, if any, is returned unchanged. With no earlier
   measurement the answer is `503 node.runtime_unavailable`.
2. **A card is not capacity until a container can have it.** `container_gpu` is
   measured by the runner, stored on the node, and reported next to the GPUs; a
   node with devices but without the NVIDIA container runtime says so in its
   detection note.

The measurement happens outside the tenant transaction, so a runner that never
answers cannot hold a transaction open for its timeout.

Phase 1 has one runtime adapter, so only the local node can be probed honestly.
A remote node needs its own adapter before a probe of it can mean anything, which
is why no route creates one yet.
