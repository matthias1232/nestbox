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
| `provider_service.rs` | Provider profiles with their probe status, the pending profile an install creates, the in-use check on delete, and the reachable-only inference environment a sandbox receives |
| `agent_service.rs` | Agents inside a sandbox: which packs may join, homes and their persistence, the supervisor command |
| `policy_service.rs` | Validation, diff, scope rules, global override |
| `telemetry_service.rs` | Ingest, live publish, history queries |
| `model_service.rs` | Hardware probing through a node's runner (CPU, RAM, GPU, unified pool), fit against a node's free budget, facets, catalog filters, installs and their reservations |
| `node_service.rs` | The nodes list, one node with its budget, a node's sandboxes, name-or-id resolution, display rename |
| `pack_service.rs` | Baselines, agent packs, and tool catalog as data; pack defaults by row, baseline compatibility, package-name resolution |
| `builder_service.rs` | Recipes: validation against the catalogs, the Dockerfile generator, content hash, build job, instantiate |
| `gateway_service.rs` | Registry, settings, drift, sync |
| `job_service.rs` | Job creation and progress |
| `schedule_service.rs` | Schedule evaluation and runs |
| `metrics_service.rs` | Sample ingest, ranged query, rollup, retention (`feat/sandbox-metrics`) |
| `inventory_service.rs` | Snapshot storage and age; never an environment value (`feat/sandbox-metrics`) |
| `access_service.rs` | SSH key pair as a file with a locator, port pool, sessions |
| `mcp_service.rs` | Registry, attachments, discovery through the policy check (`feat/mcp-registry`) |
| `persist_service.rs` | Persistent roots: derived host paths, mounts, the one-read-write rule, honest usage |

## Probing a node

The hardware probe runs on the node, through its runner: `ctx.runtime.hardware()`
returns raw `/proc` and `nvidia-smi` text, and the parsers in `model_service.rs`
turn it into a `HardwareProfile` for that node's id. Measuring from the control
plane would describe the control plane's container and call it the machine.

Three rules follow from that, and all are in `model_service.rs`:

1. **A failed probe invents nothing.** No profile is written, the node is marked
   offline (runner unreachable) or unknown (runner answered with an error), and
   the previous measurement, if any, is returned unchanged. With no earlier
   measurement the answer is `503 node.runtime_unavailable`.
2. **A card is not capacity until a container can have it.** `container_gpu` is
   measured by the runner, stored on the node, and `usable_hardware` clears the
   GPU fields for the fit when it is false.
3. **The runner call happens with no transaction open.** `resolve_node_for` and
   `load_hardware` each open a short scope, so a runner that never answers cannot
   hold a transaction for its timeout. The scope that reads installs and the
   catalog begins afterwards.

Phase 1 has one runtime adapter, so only the local node is measured this way. A
remote node without an adapter gets `unmeasured_hardware` — zeroes and a note,
never a guess about a machine the control plane cannot see.

## Access: keys, ports, sessions

`access_service.rs` is the only place a private key exists as a value, and only
for the length of one response (ADR 0024).

- **The file and the row are written in one order.** Key material is generated,
  written with `create_new` and mode 0600, and only then recorded. A failure
  writing the row removes the file again, so a crashed create leaves nothing
  usable. A file without a row is a leftover and is deleted on the next attempt;
  a row without a file is a broken state the operator repairs by rotating.
- **The download is stamped by a conditional update.** `download` reads the
  material first and then runs `UPDATE ... WHERE downloaded_at IS NULL`; the
  loser of a race gets 410 `key.already_downloaded`. Reading first means an
  unreadable file does not burn the single chance.
- **Rotation deletes before it writes.** A crash between the two steps must
  leave no usable key rather than two.
- **A port is claimed by the database.** `claim_port` is an insert with
  `ON CONFLICT DO NOTHING`; the prescan of occupied ports only shortens the
  search. Withdrawing a forward deletes the pool row first, because
  `node_port_pool.forward_id` is `ON DELETE SET NULL` and the link would
  otherwise be lost.
- **Provisioning is called from the create path** (`api/sandboxes.rs`), so every
  new sandbox has a key and its ports. It is idempotent: an existing key and
  existing forwards are returned, not duplicated. A provisioning failure is
  logged rather than turned into a failed create, because the sandbox row and
  container already exist by then.
- **A forward starts `pending`.** It is claimed and visible; `active` means the
  node's runtime has the `-p` mapping, which is the runner's step.

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

## The builder

`builder_service.rs` deserves a note because its decisions are not obvious from
the signatures.

- **The Dockerfile is generated, never stored.** The recipe holds the choices;
  the catalogs hold what those choices mean; the generator is the only place the
  two meet. A corrected pack row changes the next build with no migration.
- **A build is a job that runs detached.** The endpoint records a `recipe_build`
  job, marks the recipe `building`, and returns. The task is spawned in process
  because there is no queue worker yet; a restart during a build leaves the row
  `building`, and a PATCH recovers it. The job body is `builder_service::run_build`,
  public so a test can run it without a detached task.
- **`instantiate` writes the sandbox row itself** rather than calling
  `sandbox_service::create`, because a recipe carries an agent set and a
  content-addressed image tag that `CreateSandbox` cannot express, and its
  mounts have to be in the spec the container is created with. It reuses the
  same pieces (`SandboxName`, the placement rules, `pack_needs_inference`, the
  status settlement), so the two paths send the same shape to the runtime. Once
  `CreateSandbox` can carry agents and a recipe image, this should collapse into
  a call to it.
- **The catalogs are read through the store, not `pack_service`.** Both read the
  same rows; going through `pack_service` is the cleaner layering and changes
  nothing about the recipes.

## Extending this module

Add the rule here first, as a pure function in `domain` if possible, and only
then wire it to the store. If the rule needs a permission, add the permission to
`domain::auth` before writing the check, so the check cannot reference a
permission that does not exist.
