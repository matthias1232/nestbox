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

## Files

| File | Responsibility |
|---|---|
| `architecture.rs` | Layer boundaries, `AGENTS.md` presence, doc comments |
