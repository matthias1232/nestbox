# ADR 0001 — PostgreSQL as the only control store

Status: Accepted
Date: 2026-10-26

## Context

The first prototype kept its entire control state in a single `state.json`
file, loaded into memory behind a `RwLock`, with an optional migration path
from an older format. It worked for three sandboxes. It cannot work for
thousands, and it cannot work for more than one API instance.

The concrete problems were not stylistic:

- Two API instances would each hold their own copy and silently diverge.
- There was no transaction boundary, so a half-applied bulk action left
  inconsistent state.
- Versioning, audit, and "who changed what" had nowhere to live.
- Every list operation loaded the whole dataset, so filtering happened in Rust
  after the fact.
- A crash during a write could truncate the file.

## Decision

PostgreSQL is the single control store. The database is the source of truth.

- No `state.json` in the production path.
- No in-memory store implementation.
- No import tooling. The development environment may be rebuilt from scratch.
- Schema changes happen only through versioned migrations under `migrations/`.
- Multi-step writes run inside a transaction.

## Alternatives rejected

**Keep the JSON file and add a write-ahead log.** Rejected: reimplements
transactions, indexes, and concurrency badly. The effort exceeds adopting a
database, and the result is still single-writer.

**SQLite.** Rejected: solves durability and transactions, but not concurrent
writers across instances, and not row-level security. It would need replacing
in the same phase the product is supposed to scale in.

**An embedded key-value store (sled, RocksDB).** Rejected: no relational
queries, no constraints, so every invariant moves into application code. The
filtering and faceting requirements make this a poor fit.

**Document store (MongoDB).** Rejected: the domain is relational (organization
→ environment → cluster → gateway → sandbox). Document storage would push
joins into the application and lose referential integrity.

## Consequences

- The system needs a database process, a connection pool, and migrations.
  Deployment gets one more container.
- Tests need a real database, so the test suite is slower than an in-memory
  fake would be. That is accepted: testing what runs in production is worth it.
- Every read path must be designed as a query, including paging and filtering.
  This is the intended pressure: it forces server-side paging early.
- Backup and restore become a documented operational concern rather than
  "copy the file".

## What would invalidate this

- A requirement for offline operation of a single node with no database
  process. The answer would be an embedded Postgres, not a return to JSON.
- A read load so high that Postgres cannot serve it, which would justify a
  cache or a read replica. It would not change the source of truth.
