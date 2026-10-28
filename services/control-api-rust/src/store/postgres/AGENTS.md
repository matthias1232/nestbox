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

## Extending this folder

Add the statement to the file of its aggregate. Take `&mut TenantScope`. Name
the columns. Map to a domain type. If the query needs a new index, add it in a
migration in the same change, and say in the migration comment which query it
serves.
