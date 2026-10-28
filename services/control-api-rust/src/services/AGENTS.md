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
| `sandbox_service.rs` | Lifecycle, placement, bulk actions, and the proxy environment every sandbox receives |
| `provider_service.rs` | Provider profiles with their probe status, the pending profile an install creates, the in-use check on delete, and the reachable-only inference environment a sandbox receives |
| `agent_service.rs` | Agents inside a sandbox: which packs may join, homes and their persistence, the supervisor command |
| `policy_service.rs` | Validation, diff, scope rules, global override, and the egress decider (`check`: windows, rules, one evidence row; `authenticate_node`; `deployment`) |
| `telemetry_service.rs` | Ingest, live publish, history queries |
| `traffic_service.rs` | Traffic rows with window counts, the map, one body, MCP calls, effective rules with the deployment comparison, live edges for the cluster graph (`feat/live-traffic`) |
| `model_service.rs` | Hardware probing through a node's runner (CPU, RAM, GPU, unified pool), fit against a node's free budget, facets, catalog filters, installs and their reservations |
| `node_service.rs` | The nodes list, one node with its budget, a node's sandboxes, name-or-id resolution, display rename |
| `pack_service.rs` | Baselines, agent packs, and tool catalog as data; pack defaults by row, baseline compatibility, package-name resolution |
| `builder_service.rs` | Recipes: validation against the catalogs, the Dockerfile generator, content hash, build job, instantiate |
| `gateway_service.rs` | Registry, settings, drift, sync |
| `job_service.rs` | Job creation and progress |
| `catalog_sync/` | The model catalogue is rebuilt from Hugging Face, Ollama, NGC, and NIM, never over a curated row |
| `schedule_service.rs` | Schedule evaluation and runs |
| `metrics_service.rs` | Sample ingest, ranged query, rollup, partition maintenance, the two metrics jobs |
| `inventory_service.rs` | Snapshot storage and age, the allow-list that keeps a value out, never an environment value |
| `access_service.rs` | SSH key pair as a file with a locator, port pool, sessions (`feat/sandbox-access`) |
| `mcp_service.rs` | Registry validation, attachments, discovery from inside the sandbox |
| `persist_service.rs` | Persistent roots: derived host paths, mounts, the one-read-write rule, honest usage |
| `terminal_service.rs` | The browser terminal: permission, the four-per-sandbox and twenty-per-user limits, the idle clock, the session row and its two audit events (`feat/terminal`) |

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

## The proxy environment

A sandbox has no route out: the runtime network is `internal`, so it has a route
to its own subnet and nothing else, not even a DNS answer for a public name. The
one address it can use is the node's egress proxy, so `create` writes that
address into the sandbox environment (`egress_environment`) and records it in the
sandbox spec, which is what makes "what was this sandbox told" answerable later.

Four decisions worth keeping:

1. **`HTTP_PROXY`, `HTTPS_PROXY`, `ALL_PROXY` are set in both spellings.** curl
   and wget read the lowercase ones, Go, Rust and npm read the uppercase ones.
   One spelling is the classic "works in the terminal, not in the agent" bug,
   and the failure looks like a network fault rather than a missing variable.
2. **`NO_PROXY` names the proxy itself**, otherwise a proxy-aware tool sends the
   request for the proxy to the proxy.
3. **The environment is a convenience, not a permission.** A request still needs
   a `network_rules` match; the control API decides per request (ADR 0013). A
   sandbox that ignores the variables is not less governed, it is simply unable
   to reach anything.
4. **The URL comes from `NESTBOX_EGRESS_PROXY_URL`, defaulting to
   `http://egress-proxy:3128`.** `nodes.egress_proxy_url` exists in the schema
   (migration 0022) but not yet on the `Node` domain type; one value a deploy can
   set beats a second source of truth, and when the field reaches the domain type
   only `egress_proxy_url()` changes. The default has to match the runner's
   `NESTBOX_EGRESS_PROXY_HOST`, because the runner is what attaches the proxy to
   the sandbox network under that alias.

`builder_service::instantiate` calls the same two functions. A recipe sandbox is
created through a different path, and a path that forgot the proxy would be a
sandbox that is isolated and therefore useless, with nothing in the UI saying
why.

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

## The MCP layer

`mcp_service.rs` keeps three decisions that look small and are not.

1. **Discovery runs inside the sandbox.** `tools/list` is an outbound request,
   so it is made by a probe executed through `ctx.runtime.exec`, not by the
   control plane. The control plane is on the host network: a request from here
   would bypass the sandbox's proxy, its rules, and the identity the policy is
   written against. A discovery path that skipped the rule check would be an
   egress hole with a friendly name (ADR 0026).
2. **A failed discovery is an attachment state, not a 5xx.** The row becomes
   `status=error` with `last_error`, and the response is the row. Request errors
   stay request errors: unknown sandbox 404, stopped sandbox 409, `stdio` server
   422. Returning 5xx would leave the row claiming the previous, optimistic
   state, which is the failure mode the column exists to prevent.
3. **Transport is validated as a pair with endpoint/command.** A `stdio` server
   with an endpoint, an `http` server without one, or a stdio server claiming
   `oauth` is a 422 naming the combination. The database CHECK constraints agree,
   but they cannot produce a sentence a user can act on.

A credential never enters this layer: `env_slots` holds variable **names**, the
probe reads the value inside the container, and `mcp_tool_calls` arguments and
results are redacted in the store on the way in.

The probe speaks the `2025-06-18` handshake (`initialize`, then `tools/list`).
A server that only offers the stateless `2026-07-28` discovery is recorded as an
error naming its refusal; it is not silently retried with a second protocol,
because two handshakes would make "discovery failed" ambiguous. Extend the probe
when there is a server that needs it, not before.

## Extending this module

Add the rule here first, as a pure function in `domain` if possible, and only
then wire it to the store. If the rule needs a permission, add the permission to
`domain::auth` before writing the check, so the check cannot reference a
permission that does not exist.

## The terminal

`terminal_service.rs` splits a session in two on purpose: `open` authorises and
reserves a row, `attach` runs the PTY. The split is what lets a refusal be an
HTTP status code — a WebSocket that upgrades and then closes explains nothing —
and it is why the transport is two small ports (`ClientSocket` for the browser,
`runtime::TerminalSocket` for the PTY) instead of an axum type in this layer.

The idle clock measures input, not output (ADR 0023). The limits are counted
from the open rows, so they survive a restart; the advisory lock in
`terminal_sessions::lock_user` makes the count and the insert one decision.
Nothing in this file may write a keystroke or a line of output anywhere.

The audit detail names the session `terminal_id`, not `session_id`: the central
redactor masks every key containing "session" (so a session cookie can never
leak), and a masked reference reads like a redacted secret instead of a pointer.
The row itself is in `sandbox_sessions`; `terminal_id` is the join key a reviewer
uses to find it.
