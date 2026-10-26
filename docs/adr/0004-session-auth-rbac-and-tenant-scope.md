# ADR 0004 — Session auth, RBAC, and tenant scope

Status: Accepted
Date: 2026-10-26

## Context

The prototype protected the console with a single HTTP Basic Auth user and
allowed every request with `CorsLayer::permissive()`. That is acceptable for
one operator on a laptop. It is not acceptable for a multi-tenant control
plane that manages credentials, executes shell commands, and enforces network
policy.

Two distinct problems were conflated: *who is this* (authentication) and
*what may they touch* (authorization). Multi-tenancy makes the second one
load-bearing, because the data model already separates environments.

## Decision

**Authentication** is a session cookie, plus bearer tokens for automation.

- `HttpOnly`, `SameSite=Lax`, `Secure` when TLS is active.
- Server-side session record, so logout revokes immediately.
- CSRF token required on every mutating request (double submit).
- Idle timeout and absolute expiry; token rotation on privilege change.

**Authorization** is role-based with tenant scope.

| Role | Scope |
|---|---|
| `admin` | Full control within the environment, including gateway settings and global policy |
| `operator` | Sandbox lifecycle, policy edits, exec, model install |
| `viewer` | Read-only, including telemetry |

- Permissions are checked **inside the service**, not only in the router,
  because a new route that forgets the check is the common failure.
- Every user has membership records per environment (`user_scopes`).

**Tenant isolation** is belt and braces.

- `environment_id` is an explicit parameter on every repository call.
- PostgreSQL row-level security enforces the same filter in the database via
  `SET LOCAL app.environment_id`.
- A test proves a tampered `environment_id` is rejected in the application
  layer *before* the database, and that the database would reject it anyway.

## Alternatives rejected

**JWT in localStorage.** Rejected: readable by any script on the page, so an
XSS becomes full account takeover. It also cannot be revoked server-side
without a denylist, which reintroduces server state without the benefits.

**Keep Basic Auth and add a header for the tenant.** Rejected: no logout, no
expiry, no roles, credentials sent on every request, and the tenant becomes a
client-supplied value with no verification.

**Application-level filtering only, no RLS.** Rejected: one missed `WHERE`
clause is a cross-tenant data leak. RLS makes the database refuse rather than
trust. The cost is that every transaction must set the scope, which is a
constraint worth paying.

**RLS only, no application filter.** Rejected: relies on the database session
being correct, and produces worse error messages. The application filter is
the primary control; RLS is the backstop.

## Consequences

- Every request needs a session lookup, which is one indexed query.
- Every transaction must set the tenant scope. A helper guarantees it, and a
  test asserts that a repository call without scope fails.
- Automation uses tokens rather than the session cookie, so the CLI and CI
  never handle a browser session.
- The role model is coarse (three roles). Fine-grained permissions would need a
  new table; the three roles cover the real cases and can be extended without
  breaking the contract.

## What would invalidate this

- A requirement for delegated administration inside a single environment,
  which would need resource-level permissions rather than a role.
- Federation with an external identity provider, which would replace the
  password store with OIDC while keeping the session and scope model intact.
