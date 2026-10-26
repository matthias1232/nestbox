# ADR 0005 — Secrets as locators only

Status: Accepted
Date: 2026-10-26

## Context

Nestbox manages API keys for model providers, TLS private keys, and per-sandbox
gateway tokens. The prototype stored provider configuration in a JSON file that
also held the key value, and served that file's content back through the API.

Three failure modes follow from storing secret values in the control store:

1. A database dump or backup becomes a credential leak.
2. Any response that serialises the row leaks the key.
3. Any log line that prints the struct leaks the key.

## Decision

The database stores a **locator**, never a value.

```
secret_references(id, environment_id, name, locator, kind)
   locator: file:///run/secrets/omniroute
   locator: file:///etc/nestbox/secrets/provider-nvidia
   locator: vault://kv/data/nestbox/omniroute     (future resolver)
   locator: k8s://namespace/name/key              (future resolver)
```

- Values live in files with mode `0600` outside the repository, under
  `/etc/nestbox/secrets/`.
- A resolver turns a locator into a value at the moment of use, in memory only.
- A central `redact()` filter masks known-sensitive keys in all logs, errors,
  and telemetry. The list of sensitive keys is one constant, not scattered.
- API responses expose `api_key_configured: bool` and never a value or a
  prefix of one.
- A test asserts that no response body, log record, or event contains a
  canary secret.

## Alternatives rejected

**Encrypt values in the database with a master key.** Rejected as the primary
mechanism: it moves the problem to key custody and still puts ciphertext in
every backup. It is a valid second layer, not a substitute for keeping values
out.

**Environment variables injected into the API container.** Rejected: visible in
`docker inspect`, in the compose file, and in any crash dump. Fine for
non-secret configuration, not for credentials.

**A dedicated secret manager in this phase.** Deferred rather than rejected.
Vault or Kubernetes secrets would be added as another resolver behind the same
locator format, with no schema change. That is precisely why the locator is a
URI and not a column named `path`.

**HashiCorp Vault immediately.** Rejected for M1: it adds an operational
dependency and a failure mode (Vault down means no provider credentials) before
there is a deployment that needs its guarantees. The locator design makes the
migration additive.

## Consequences

- The control plane needs read access to the secret directory, which is a
  privileged mount and must be scoped to exactly that path.
- Rotation is a file replacement plus a version bump in the reference, not an
  `UPDATE` of a value. Rotation is therefore auditable and reversible.
- A secret cannot be displayed in the UI, ever. The UI shows whether it is
  configured and when it was last rotated.
- The resolver interface is asynchronous and fallible, so callers must handle
  "secret unavailable" as a real state rather than assuming a value exists.

## What would invalidate this

- A requirement to display a secret to an operator, which would be a product
  decision to change, not an engineering one.
- A deployment platform where file-based secrets are unavailable, which would
  make the Kubernetes resolver mandatory rather than optional.
