# api/ — HTTP

## Purpose

Read the request, check what it claims, delegate to a service, format the answer.

Nothing here contains a rule or a query. That is not stylistic: a handler that
queries the database is a handler that can bypass the tenant scope and the
permission check, and it is the exact shape that dissolved the previous version
of this service.

## Dependencies

- May use: `services`, `domain`, `error`, `security`, `state`
- Must not use: `sqlx::*`, `store::*` directly, `runtime::*` directly

The architecture test enforces the last two.

## Security rules

1. Identity comes from the `Auth` extractor, which is the only place a credential
   is read. CSRF is enforced there too, for session requests only.
2. A handler never trusts a body to name its own environment or actor; both come
   from the principal.
3. A path parameter is parsed into a typed value (`Uuid`) by the extractor, so a
   malformed id is a 400 before any logic runs.
4. An id supplied in a create body is ignored where the database assigns one.

## Files

| File | Routes |
|---|---|
| `mod.rs` | Router composition and layers |
| `principal.rs` | The `Auth` extractor: session, token, CSRF |
| `auth.rs` | Login, logout, identity |
| `system.rs` | Health, readiness, runtime, OpenAPI, environments, audit |
| `sandboxes.rs` | Sandbox list, detail, create, actions, logs, exec |
| `policies.rs` | Policies, rules, drafts, traffic windows |
| `telemetry.rs` | History, facets, ingest |
| `events.rs` | SSE streams |
| `cluster.rs` | Gateways, supervisors |
| `providers.rs` | Provider profiles, probe |
| `models.rs` | Catalog per node with filters, per-node hardware, facets, requirements, installs with reservations, sync |
| `nodes.rs` | Nodes with budget and sandbox count, one node, its sandboxes, display rename, probe through the runner |
| `packs.rs` | Baselines, agent packs, and tool catalog as data, with `?baseline=` resolution |
| `recipes.rs` | Recipe CRUD, build, instantiate (`feat/recipes-build`) |
| `persistent_roots.rs` | Roots, mount, dismount, usage |
| `sandbox_agents.rs` | Agents inside a sandbox: list, add, remove with `keep_persistent` |
| `jobs.rs` | Jobs, schedules |
| `metrics.rs` | Resource history, node sample ingest, process table (`feat/sandbox-metrics`) |
| `inventory.rs` | Snapshot and refresh (`feat/sandbox-metrics`) |
| `access.rs` | SSH key, one-time download, rotation, ports, sessions (`feat/sandbox-access`) |
| `traffic.rs` | Traffic rows, one body, map edges, effective policies, MCP calls (`feat/live-traffic`) |
| `mcp.rs` | MCP registry and sandbox attachments (`feat/mcp-registry`) |
| `sandboxes_terminal.rs` | The terminal WebSocket (`feat/terminal`) |
| `topology.rs` | The cluster graph and its live edges (`feat/live-traffic`) |

`models::hardware_json` is the one shape for a machine: the catalog envelope,
`GET /models/hardware`, and `POST /nodes/{id}/probe` all use it, so it always
names the node, its status, and whether containers there can have a GPU next to
the measurement. A second shape would let those three drift apart.

## Work packages

A route may be registered and documented before its handler exists: the stub
answers `501 server.not_implemented`. This is how parallel packages share one
contract. A package fills the handler bodies in its file and touches nothing
in `mod.rs` or `openapi.rs`.

## Extending

Add the handler to the file of its resource, then add the route in `mod.rs`
**and** the operation in `openapi.rs`. The OpenAPI test fails if a route is
documented without existing, or the reverse.

## The terminal route

`GET /sandboxes/{id}/terminal` is the only WebSocket in this layer, and it is
still a handler and not a rule:

1. the `Origin` header is checked against `config.origins` and against the
   request's own host, because a WebSocket is not covered by the same-origin
   policy and a session cookie is. A request without an `Origin` is accepted only
   for a bearer token;
2. `terminal_service::open` runs **before** the upgrade, so a missing permission,
   a stopped sandbox, or a full quota is a 403, a 409, or a 429 with a message;
3. after `on_upgrade`, `BrowserSocket` adapts axum's messages to the service's
   `ClientFrame`/`ServerFrame` port, and `attach` runs the session.

The close frame the console shows comes from the service; the handler only adds
the WebSocket close.
