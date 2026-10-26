# ADR 0002 — Layered architecture with enforced boundaries

Status: Accepted
Date: 2026-10-26

## Context

The prototype had one `services/sandboxes.rs` of 286 lines that read HTTP
types, decided business rules, and wrote state, plus an `api/sandboxes.rs`
that also contained rules. A reader could not tell where a rule lived, and
changing a rule meant reading three files.

The requirement from the maintainer is explicit: a human must be able to open
one file and know what it is for, and the API must be modular so a feature can
be added without touching unrelated code.

## Decision

Five layers with hard, tested boundaries:

| Layer | Owns | May use | Must not use |
|---|---|---|---|
| `domain/` | Data types, validation, pure functions | std | `sqlx`, `axum`, `reqwest`, `async fn` |
| `store/` | SQL, mapping to domain types | `sqlx`, `domain` | `axum`, business rules |
| `services/` | Business rules, orchestration | `domain`, store traits, runtime traits | `axum::*`, `sqlx::*` |
| `api/` | HTTP: parse, validate, delegate, respond | `axum`, `services`, `domain` | `sqlx::`, Docker calls |
| `runtime/` | Execution against gateways and containers | `reqwest`, `domain` | `sqlx`, `axum` routing |

The boundaries are checked by `tests/architecture.rs`, which reads the source
tree and fails on a violation. The rules are not documentation that people are
trusted to follow; they are a build gate.

## Alternatives rejected

**Convention only, no test.** Rejected: the prototype is the evidence that
conventions erode. The test costs a few dozen lines and catches the failure
mode that actually occurred.

**A single crate per layer with separate Cargo workspaces.** Rejected for now:
workspaces enforce boundaries structurally and more strongly, but they
multiply build time, complicate the Docker build, and make small changes
heavier. The source-level test gives most of the benefit at a fraction of the
cost. This is revisitable if the team grows.

**Hexagonal ports and adapters with a trait per repository.** Partially
adopted: the store exposes traits, and the runtime is a trait. A full
port-per-aggregate design was rejected as ceremony that hides a simple call
graph behind indirection.

**Framework-driven structure (Axum extractors that call the database).**
Rejected: that is exactly the shape that dissolved the prototype's structure.

## Consequences

- Adding a feature means touching the layers in order, which is more files but
  each file is small and unambiguous.
- Some operations need a value passed through three layers unchanged. That is
  the cost of the separation and is accepted.
- The architecture test needs occasional updating as legitimate patterns
  appear, for example a new allowed dependency. That update is a deliberate,
  reviewable change rather than silent drift.

## What would invalidate this

- A performance requirement that can only be met by collapsing layers, for
  example a hot path that must go from HTTP to SQL directly. The answer would
  be a documented exception with a benchmark, not the removal of the rule.
