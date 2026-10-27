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
| `model_service.rs` | Hardware detection (CPU, RAM, GPU, unified pool), fit, facets, catalog filters, install jobs |
| `builder_service.rs` | Recipes: validation against the catalogs, the Dockerfile generator, content hash, build job, instantiate |
| `gateway_service.rs` | Registry, settings, drift, sync |
| `job_service.rs` | Job creation and progress |
| `schedule_service.rs` | Schedule evaluation and runs |

## The builder

`builder_service.rs` deserves a note because two of its decisions are not
obvious from the signature.

- **The Dockerfile is generated, never stored.** The recipe holds the choices;
  the catalogs hold what those choices mean; the generator is the only place the
  two meet. A corrected pack row changes the next build with no migration.
- **A build is a job that runs detached.** The endpoint records a `recipe_build`
  job, marks the recipe `building`, and returns. The task is spawned in process
  because there is no queue worker yet; a restart during a build leaves the row
  `building`, and a PATCH recovers it. The job body is `builder_service::run_build`,
  public so a test can run it without a detached task.
- **`instantiate` writes the sandbox row itself** rather than calling
  `sandbox_service::create`, because a recipe carries mounts, agents, and an
  image tag that `CreateSandbox` cannot express. It reuses the same pieces
  (`SandboxName`, the placement rules, `pack_needs_inference`, the status
  settlement) so the two paths send the same shape to the runtime.

## Extending this module

Add the rule here first, as a pure function in `domain` if possible, and only
then wire it to the store. If the rule needs a permission, add the permission to
`domain::auth` before writing the check, so the check cannot reference a
permission that does not exist.
