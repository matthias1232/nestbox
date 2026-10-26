# control-api-rust — the Nestbox control plane

The HTTP API behind the console. Rust, Axum, PostgreSQL.

## Start here

Read `src/AGENTS.md` for the layer model, then the `AGENTS.md` of the layer you
are about to change. If you are about to change a design decision rather than a
file, read the relevant ADR in `../../docs/adr/` first.

## Layout

| Path | What |
|---|---|
| `src/domain/` | Pure types and validation. No I/O. |
| `src/store/` | SQL and row mapping. No business rules. |
| `src/services/` | Business rules. No HTTP, no SQL. |
| `src/api/` | HTTP parsing and responses. No SQL. |
| `src/runtime/` | Execution against gateways and containers. |
| `src/events/` | The live event bus and SSE transport. |
| `src/security/` | Sessions, passwords, redaction, headers. |
| `migrations/` | Versioned SQL. Forward-only. |
| `tests/` | Integration and architecture tests. |

## Run locally

```bash
export DATABASE_URL=postgres://nestbox:test@localhost:55432/nestbox_test
export NESTBOX_BOOTSTRAP_ADMIN=true
cargo run -- migrate     # apply migrations, seed, bootstrap the admin
cargo run                # start the server on :8000
```

## Test

```bash
docker compose -f ../../deploy/docker-compose.test.yml up -d
export TEST_DATABASE_URL=postgres://nestbox:test@localhost:55432/nestbox_test
cargo test
```

The suite needs a real PostgreSQL, deliberately (ADR 0001): the migration path
and the row-level-security policy are part of what is tested, and an in-memory
fake would not exercise either.

## Why runtime queries instead of `query!` macros

`sqlx::query_as` with bound parameters is used rather than the compile-time
`query!` macros. The reason is operational: the macros need a live database at
build time, which means the Docker build needs database access. The security
property that matters — every value is a bound parameter, never concatenated —
is identical. The trade-off is that a schema mismatch is caught by a test
instead of by the compiler, which the integration suite covers.

See `src/store/AGENTS.md` for the rules that replace the compile-time check.

## Endpoints

The OpenAPI document is served at `/api/v1/openapi.json`, Swagger UI at
`/docs`, and a human-readable Redoc page at `/redoc.html`. The generated
document is committed and compared in CI (ADR 0015).
