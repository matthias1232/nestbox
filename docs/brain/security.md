# Security

## What this system protects

It manages containers that run AI agents, API keys that cost money, and network
paths that a compromised workload would want. The assets are:

1. **Credentials** — provider keys, TLS private keys, per-sandbox tokens.
2. **Tenant data** — one environment must never see another's sandboxes,
   policies, or telemetry.
3. **The host** — a sandbox escape is a full compromise.
4. **The audit trail** — if it can be edited, it proves nothing.

## The controls, by asset

### Credentials

The database stores a **locator**, never a value (ADR 0005). Values live in
`0600` files outside the repository. A resolver reads them at the moment of use.

A central redaction filter masks sensitive keys in every log line, error body,
and telemetry record. The list of sensitive names is one constant, so a new
credential cannot be forgotten in one place and remembered in another.

No response contains a secret. The API returns `configured: bool` and a rotation
timestamp. A test asserts a canary secret never appears in any output.

### Tenant data

Two independent controls:

- The repository requires `environment_id` on every call. It is not optional.
- PostgreSQL row-level security enforces the same filter via
  `SET LOCAL app.environment_id`.

The reason for both is that the failure mode is silent. A missing filter returns
another tenant's rows and looks like success. Two controls mean one bug is not a
breach.

### The host

Containers run without privileges: capabilities dropped, `no-new-privileges`,
no privileged mode, no host network, no host mounts, resource limits set. Images
are pinned by digest and drawn from an allowlist; `latest` is rejected.

`exec` and `terminal` are the most powerful features in the product. They run
**inside** the sandbox, not on the host, they require a dedicated permission
(`sandbox.exec`), and every invocation is audited with the exact command.

### The audit trail

Append-only. There is no update and no delete path in the API. An audit record
that can be changed is not an audit record.

## The application-level controls

| Threat | Control |
|---|---|
| SQL injection | Bound parameters only; no string building; CI grep |
| Command injection | Argument vectors, never a shell string |
| CSRF | Token on every mutating request, double submit |
| Session theft | `HttpOnly`, `Secure` under TLS, server-side revocation, idle timeout |
| IDOR | Permission check in the service, not only in the router |
| SSRF | Scheme allowlist, internal and metadata addresses blocked, DNS checked |
| Path traversal | Normalised paths, containment check |
| XSS | Framework escaping, no `innerHTML` with server data, CSP |
| Clickjacking | `frame-ancestors 'none'`, `X-Frame-Options` |
| Self-DoS | Rate limits, job quotas, mandatory pagination |
| Log injection | Structured logging, control characters stripped |
| Race conditions | Optimistic locking with a version column |

## The published contract is attack surface too

The error envelope must not leak internal detail. A stack trace, a database
error, or a filesystem path in a response is a finding, not a debugging aid. That
detail goes to the log under the `request_id`, which the client receives.

## Surviving an assessment

The threat model is a table of properties a tester probes, each with a control
and an automated test. See
`services/control-api-rust/tests/` for the suites, and section 11.2 of the
rebuild plan for the full mapping.

The posture is: an assessment should find **documented, tested behaviour**, and
where it finds a gap, the fix is recorded as an ADR so the reasoning is visible
to the next person.

## Supply chain

`cargo audit`, `cargo deny`, `npm audit`, digest-pinned images, a secret scanner
over the repository, and `clippy -D warnings` — all blocking in CI. A dependency
is added only with a note explaining why, because every dependency is also
attack surface.

## Where this lives in the code

| Concern | File |
|---|---|
| Redaction | `services/control-api-rust/src/security/redaction.rs` |
| Passwords | `services/control-api-rust/src/security/password.rs` |
| Sessions and CSRF | `services/control-api-rust/src/security/session.rs` |
| Headers and CORS | `services/control-api-rust/src/security/headers.rs` |
| Permission checks | `services/control-api-rust/src/services/access_control.rs` |
