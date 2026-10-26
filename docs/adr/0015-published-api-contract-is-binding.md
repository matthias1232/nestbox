# ADR 0015 — The published API contract is binding

Status: Accepted
Date: 2026-10-26

## Context

The public documentation already promises a specific API shape:

- Two authentication schemes: `Authorization: Token <key>` and a session cookie.
- Three roles: `admin`, `operator`, `viewer`.
- One error envelope with a stable `code`, a developer-facing English
  `message`, `details`, and `request_id`.
- SSE event names are part of the contract.
- `/api/v1/` is frozen; breaking changes get a new prefix.
- `/api/schema/v1/` is the machine-readable contract, and CI fails when the
  committed contract changes without intent.

These are published. They are therefore constraints on the implementation, not
aspirations.

## Decision

The published contract is implemented as written, and drift is a build failure.

- The error envelope is produced by one function, so every error looks the same
  including on streaming endpoints.
- `code` is a stable, non-translated identifier. The UI translates the code, not
  the message. The message is for developers.
- `request_id` is generated per request, returned to the client, and logged, so
  a user report maps to a log line.
- OpenAPI is generated from the Rust types with `utoipa`, and a checked-in copy
  of the generated document is compared in CI. A change that is not intentional
  fails.
- SSE event names are constants in one module and referenced from the contract.

## Alternatives rejected

**Design the API fresh, update the docs afterwards.** Rejected: the docs are
published and third parties may build against them. Changing them silently is
the failure mode the frozen prefix exists to prevent.

**Free-form error strings.** Rejected: they cannot be branched on, cannot be
translated reliably, and change whenever someone edits a message.

**Document errors only in OpenAPI, not in behaviour tests.** Rejected: the
document and the implementation drift independently. A test that asserts the
envelope shape is what keeps them together.

**Skip `request_id` until there is a logging stack.** Rejected: it is the one
field that makes a user report actionable, and it is cheap.

## Consequences

- The API shape is decided partly by the documentation, including naming. Where
  the docs say `nest` in prose but the resource is a sandbox, the resource name
  wins and the prose is clarified; the wire format follows the resource.
- The generated OpenAPI document is committed, so a type change shows up in
  review as a contract change.
- Adding a field is safe; removing or renaming one needs a new prefix. This is
  the intended friction.
- Some internal error detail cannot reach the client. It goes to the log with
  the `request_id` instead.

## What would invalidate this

- A v2 API, which would be added alongside v1 rather than replacing it, with its
  own published contract.
