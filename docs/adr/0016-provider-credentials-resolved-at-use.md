# 0016 — Provider credentials are resolved at use, never stored or returned

## Status

Accepted

## Context

An agent sandbox needs an inference endpoint and a credential. The credential
has to reach the container, and the endpoint has to be editable without a
redeploy. ADR 0005 already rules out storing a credential value in the database.
The open question was *when* the value is read and *where* it goes.

Three options were on the table:

1. Bake the key into the sandbox image at build time.
2. Store the value encrypted in PostgreSQL and decrypt it on create.
3. Store only a locator; resolve it to a value at the moment a sandbox is
   created, and hand it to the container as environment.

## Decision

Option 3. The database holds `provider_profiles.secret_ref` as a `file://`
locator. `security::secrets::resolve` turns it into a value exactly when the
sandbox service builds the runtime spec, and the value goes into the container's
environment. It is never written back, never returned by an endpoint, and never
logged. `GET /providers` reports `credential_configured: true|false` and nothing
else.

Two guards make this safe rather than merely convenient:

- A locator must resolve inside the configured secret directory after
  canonicalisation. `file:///etc/passwd` is refused with a named error.
- A secret file that group or world can read is refused. Copying a key in with
  default permissions is the common mistake, and the moment it is caught is the
  moment the window is widest.

Only the `file://` scheme is implemented. `vault://` and `k8s://` are refused
with a message that says so, rather than half-implemented.

The probe (`POST /providers/{id}/probe`) uses the same resolver and reports
reachability and whether the credential was accepted, so an operator learns
"the key is wrong" from the console rather than from a failing agent.

## Consequences

- Rotating a key is: write the new value to the same file, recreate the sandbox.
  No rebuild, no migration, no edit in the console.
- A sandbox created from `curl` and one created from the console get the same
  environment, because the rule lives in the service.
- A running sandbox keeps the environment it started with. That is deliberate:
  a container's environment is immutable, and pretending otherwise would hide
  the recreate that is actually needed.
- The appliance's default provider is seeded as data (migration `0011`). The
  key file itself is placed by the operator or the deploy script
  (`deploy/scripts/set-inference-key.sh`), never by a migration, because a key
  in a migration is a key in the repository.

## Rejected

- **Option 1 (bake into image):** a rotated key would need an image rebuild
  and every sandbox recreated; a leaked image would leak the key.
- **Option 2 (encrypted in Postgres):** the decryption key has to live
  somewhere, and that somewhere is the same host. It adds a ciphertext column
  and a key-management problem without removing the file.
