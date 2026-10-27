# ADR 0027 — Bodies are logged fully, and the redactor is the boundary

Status: Accepted
Date: 2026-10-27

## Context

"Show me what the agent actually sent" is the request that makes a traffic log
useful, and the request that makes it dangerous. A request body can contain an API
key, a password, a customer record, or a prompt with personal data in it. Logging
bodies is therefore never a question of whether to redact, only of where.

There is also a size problem: a body can be a gigabyte, and a traffic table that
accepts one is a table nobody can query.

## Decision

**Bodies are captured up to 64 KiB per side, and only through the redactor.**
The proxy buffers at most 64 KiB; a larger body is logged as metadata with
`truncated = true`. The control API runs `security::redaction` on every body
before insert — the same function that redacts logs and error messages. There is
exactly one redaction implementation, and the storage layer is not allowed to
decide for itself.

**Bodies live in their own table.** `traffic_bodies` is partitioned by day and kept
for 24 h; `telemetry_events` stays small and lives longer. Retention is a dropped
partition, not a delete of a million rows.

**The promise is written down where it can be read.** The console's traffic detail
says "redacted before storage; 24 h". It does not say "encrypted" (it is not) and
it does not say "never leaves the appliance" (the traffic log is part of a backup).

**A body that was never decrypted is never logged.** Without the node CA, an HTTPS
request is a CONNECT tunnel: there is no body to store, and the row says
`inspected = false` rather than showing an empty body as if the request had none.

## Alternatives rejected

**Log metadata only.** Safe, and it makes "the agent sent a 4xx with this body"
unanswerable — the most common reason to open the traffic tab.

**Encrypt bodies at rest with a key in the database.** The key and the ciphertext
in the same store is decoration, not protection.

**Redact in the proxy.** A second redaction implementation in a second language,
in a process the agent can influence. One implementation, in Rust, at the boundary
of storage.

## Consequences

- Patterns the redactor knows are caught; a secret in an unknown format is not.
  This is a real limitation and it is why retention is 24 h rather than 30 days.
- A 64 KiB cap means a large JSON request is stored as a prefix; the row says so.
- `traffic_bodies` is the table an operator is most likely to want deleted early,
  which is why its retention is independent of the event table's.

## What would invalidate this

A structured, per-tenant field-level policy ("this environment may never store
bodies") would layer on top of this, not replace it. The redactor at the boundary
stays either way.
