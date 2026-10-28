# store/postgres/ — the PostgreSQL implementation

## Purpose

One file per aggregate, holding the SQL for it. This folder is the only place in
the codebase where SQL appears.

## Security rules specific to this folder

1. Bind every value. The only interpolated text is a column name from
   `SortField::column()`, which is a closed Rust enum.
2. Never `SELECT *` in a read used by the API; name the columns so the mapping
   is checked against the schema by the integration test.
3. An `UPDATE` carries `AND version = $n` and reports `VersionConflict` when it
   matches nothing. Optimistic locking is what makes two operators editing the
   same policy a 409 instead of a lost update.
4. The audit table accepts inserts only. There is no update or delete statement
   for it here, and the application role's grants refuse one anyway.

## Files

| File | Aggregate |
|---|---|
| `mod.rs` | Re-exports and shared row helpers |
| `sandboxes.rs` | Sandbox list, read, insert, status change, delete |
| `auth.rs` | Users, sessions, API tokens, scopes |
| `policies.rs` | Policy documents, versions, network rules, drafts, the compiled rule-set hash |
| `egress.rs` | Compilation hash record, decisions, open windows, node token locator, deployment comparison |
| `telemetry.rs` | Event insert, query, partition maintenance |
| `gateways.rs` | Gateways, supervisors, sync state |
| `models.rs` | Model catalog, hardware profiles, inference rows, the catalog-sync upsert |
| `model_installs.rs` | One model on one node: reservation, state, provider link, its job |
| `nodes.rs` | Nodes, the local node, seed marks, display fields, sandbox counts per node |
| `jobs.rs` | Jobs, targets, schedules |
| `environments.rs` | Environments, clusters, nodes, overview counts |
| `providers.rs` | Provider profiles, secret references |
| `agent_packs.rs` | Baselines, agent packs, and the tool catalog (global data, no tenant filter) |
| `persistent_roots.rs` | Persistent roots, mounts, and the agents inside a sandbox |
| `recipes.rs` | Recipes, the build outcome, and the link from a sandbox to its recipe |
| `audit.rs` | Append-only audit writes |
| `traffic.rs` | Traffic rows, window counts, map and live edges, one body read, MCP calls, effective rules, the two hashes behind "deployed" |

## Reading the traffic log

`traffic.rs` reads the proxy's access log out of `telemetry_events.detail` instead
of a column per field, because the fields are the proxy's and they gain one at a
time. Three consequences, all deliberate:

1. **A malformed value is a `NULL`, not a failed page.** Every numeric field is
   guarded by a regex in SQL before its cast, so one bad line cannot blank a
   list of two hundred.
2. **Two spellings are accepted for a destination** (`host` from the access log,
   `destination` from the older telemetry type). A row written by either ingest
   path appears; a reader that knew only one would show a gap where there is none.
3. **A row without a decision is not traffic.** `decide` answers in three words,
   and the first column of the table is that answer; inventing one would make the
   table an opinion.

Counts and edges are grouped in SQL, never assembled from returned rows: a
five-minute window at ten thousand events a minute is a number the browser must
not have to produce. Every statement is bounded by `ts`, `environment_id`,
`sandbox_id`, and `LIMIT`.

One carve-out: the **body read** lives here rather than in `traffic_bodies.rs`,
because the body is shown from the traffic row and the read must be scoped by the
same sandbox. `traffic_bodies.rs` keeps the write side and the retention sweep.

| `terminal_sessions.rs` | Terminal sessions in `sandbox_sessions`: open, count for the limits, close. Never content (ADR 0023) |

## Extending this folder

Add the statement to the file of its aggregate. Take `&mut TenantScope`. Name
the columns. Map to a domain type. If the query needs a new index, add it in a
migration in the same change, and say in the migration comment which query it
serves.

## Counting a limit from rows

The terminal limits are counted from `sandbox_sessions`, not from a counter in
memory, so they hold across a restart and across API replicas. Two consequences
are deliberate:

* `lock_user` takes a transaction-scoped advisory lock before the count, because
  "count, then insert" is only one decision if nothing else can insert for that
  user in between;
* an unclosed row is ignored once it is older than a session can live, so a
  process that died mid-session cannot consume the quota forever;
* `open` therefore calls `close_stale` first: the ghost is closed (reason
  `error`, `closed_at` = the moment it was noticed) instead of being skipped
  silently, which keeps the audit row honest about the fact that it ended.

`sandbox_sessions` gets its tenant policy in `0025`, not in `0009`: 0009 runs
once and could not see a table that arrived in 0024. Any table added later with
an `environment_id` needs its own `ALTER TABLE ... ENABLE ROW LEVEL SECURITY`
plus the `tenant_isolation` policy; the catalogue walk in 0009 will not repeat.
