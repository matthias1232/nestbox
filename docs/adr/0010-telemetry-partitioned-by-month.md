# ADR 0010 — Telemetry is partitioned by month

Status: Accepted
Date: 2026-10-26

## Context

A live firewall console produces a lot of rows: every connection, every DNS
query, every model request. Retention is a product requirement (an environment
has `retention_days`). The naive design is one big table plus
`DELETE FROM telemetry_events WHERE ts < cutoff`.

That delete is a problem at scale: it takes a long time, it bloats the table
with dead tuples until vacuum runs, it holds locks, and it can be interrupted
halfway.

## Decision

`telemetry_events` is range-partitioned by month on `ts`.

- A maintenance job pre-creates next month's partition.
- Retention is `DROP PARTITION`, which is metadata-only and instant, instead of
  a row delete.
- Queries always carry a time bound, so the planner prunes to the relevant
  partitions.
- Indexes are per-partition: `(environment_id, ts)`, `(sandbox_id, ts)`,
  `(kind, ts)`.

## Alternatives rejected

**A single table with periodic `DELETE`.** Rejected: the vacuum debt and the
lock duration grow with the table, so the operation gets slower exactly when
the system is largest.

**TimescaleDB.** Rejected for M1: an extension adds an install dependency and a
version coupling to the appliance image. Native declarative partitioning gives
the retention property, which is the actual requirement. Timescale remains an
option if compression and continuous aggregates become necessary.

**ClickHouse from the start.** Rejected for M1: a second datastore, a second
backup story, and a second failure mode, for a workload that Postgres handles
at this size. The schema keeps telemetry in its own table precisely so that
exporting to ClickHouse later is additive. ADR 0001's principle applies:
Postgres is the control store, and a telemetry export target is a separate
decision that can be made when the volume justifies it.

**No persistence, live stream only.** Rejected: the audit and the "what
happened an hour ago" question both need history, and the website promises a
timeline.

## Consequences

- A row cannot be updated across partitions; telemetry is append-only, which is
  correct for an event log.
- A migration that changes the telemetry schema must handle existing
  partitions, so those migrations are more involved than a plain `ALTER`.
- The partition maintenance job is required, or inserts fail when the current
  month has no partition. It is one of the seeded schedules, not an optional
  extra.
- Queries without a time bound scan every partition, so the repository
  interface requires a time range rather than allowing an unbounded list.

## What would invalidate this

- A retention policy shorter than a day, which would justify daily partitions.
- A volume where Postgres cannot keep up, which would justify the ClickHouse
  export path described above.
