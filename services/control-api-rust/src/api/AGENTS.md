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
| `cluster.rs` | Gateways, supervisors, topology |
| `providers.rs` | Provider profiles, probe |
| `models.rs` | Catalog with filters, hardware, facets, requirements, installs, sync |
<<<<<<< HEAD
| `nodes.rs` | Nodes, budget, probe (handlers: `feat/nodes-api`) |
| `packs.rs` | Baselines, agent packs, tool catalog (with `?baseline=`) |
=======
| `nodes.rs` | Nodes with budget and sandbox count, one node, its sandboxes, display rename (probe: `feat/node-hardware-probe`) |
| `packs.rs` | Baselines, agent packs, tool catalog (`feat/packs-as-data`) |
>>>>>>> origin/feat/m1-rebuild
| `recipes.rs` | Recipe CRUD, build, instantiate (`feat/recipes-build`) |
| `persistent_roots.rs` | Roots, mount, dismount, usage |
| `sandbox_agents.rs` | Agents inside a sandbox (`feat/sandbox-agents`) |
| `jobs.rs` | Jobs, schedules |

## Work packages

A route may be registered and documented before its handler exists: the stub
answers `501 server.not_implemented`. This is how parallel packages share one
contract. A package fills the handler bodies in its file and touches nothing
in `mod.rs` or `openapi.rs`.

## Extending

Add the handler to the file of its resource, then add the route in `mod.rs`
**and** the operation in `openapi.rs`. The OpenAPI test fails if a route is
documented without existing, or the reverse.
