# ADR 0037 — Keys typed into the console are stored encrypted, write-only

Status: Accepted (supersedes the "no value in the database" half of ADR 0005)
Date: 2026-10-29

## Context

ADR 0005 kept every secret value out of the API and the database: a provider held a
`file://` locator and an installer put the file on the host. That is right for a
credential placed by an installer and wrong for the everyday case, an operator with a
provider key and a browser. The console had no way to add a key, so providers and their
keys could not be managed in the GUI, which is what an admin of several gateways needs.

## Decision

1. **A table of encrypted values** (`stored_secrets`), per environment, row-level
   secured like every tenant table. A value is sealed with AES-256-GCM, a random 96-bit
   nonce per value, and associated data that binds the ciphertext to its environment and
   name, so a copied row does not open elsewhere.
2. **The master key is a file, not a column.** `vault_key` (32 random bytes as hex) is
   created once by `init-data` next to the other secrets and read by the control API. A
   copy of the database is not a copy of the keys, and losing the key file loses the
   keys (by design; there is no recovery path to build a backdoor into).
3. **Write-only API.** `POST /api/v1/secrets` takes a value once and answers with its
   last four characters. No endpoint returns a value; the list shows the name, kind, last
   four, the locator, usage and timestamps. The audit trail records the name and kind,
   never the value.
4. **A provider points at a key by locator** `db://<name>`, next to the existing
   `file://<path>`. Rotation is putting a new value under the same name, so the providers
   that use it keep working. A key a provider still uses cannot be deleted.
5. **The clear value exists only in memory**, in the code that calls a provider on the
   operator's behalf (probe) or builds an agent's environment.

## Consequences

* ADR 0005's rule that a value never reaches the API still holds for reading. Writing
  one is now allowed, once, for the roles that may change providers.
* A key handed to an agent as an environment value is visible to that agent. The
  intended end state on OpenShell is a gateway-side provider that injects the credential
  only on approved requests (see ADR 0036, "Not yet").
* Key rotation of the master key is not implemented: it needs a re-encryption pass.
