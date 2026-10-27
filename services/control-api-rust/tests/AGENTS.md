# tests/ — architecture and integration checks

## Purpose

Two different jobs:

- `architecture.rs` enforces the layer boundaries by reading the source tree.
  A violation fails the build, which is the only way the rule survives.
- The integration suites in the API exercise real behaviour against a real
  PostgreSQL, because the migration path, row-level security, and cursor paging
  are exactly the things an in-memory fake would not test.

## Rules

- An architecture test asserts a property that a reviewer would otherwise have
  to remember.
- An integration test names the behaviour, not the function: "a viewer cannot
  delete a sandbox" rather than "test_delete_403".
- A test that needs a database skips cleanly when `TEST_DATABASE_URL` is unset,
  so `cargo test` works on a laptop without one.
- A suite that needs a *migrated* database creates its own database from
  `TEST_DATABASE_URL` rather than dropping `public` in the shared one, so two
  suites can run in the same `cargo test` without deleting each other's schema.
  `recipes.rs` is the pattern.

## Files

| File | Responsibility |
|---|---|
| `architecture.rs` | Layer boundaries, `AGENTS.md` presence, doc comments |
| `database.rs` | Schema, row-level security, seed, and the constraints the services rely on (skips without `TEST_DATABASE_URL`) |
| `recipes.rs` | The builder against a database: drafts, revisions, build outcome, instantiate, capacity |
| `telemetry_ingest.rs` | The proxy batch against a database: node token fail-closed, network/dns rows, redacted bodies, MCP calls, the batch cap, tenant scope (`feat/proxy-telemetry`) |
