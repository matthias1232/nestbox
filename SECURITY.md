# Security policy

## Reporting

Report vulnerabilities privately: security@mmbits.com. You will get an acknowledgement within
72 hours. Do not open a public issue.

## Threat model

Nestbox runs agents that you give credentials to. The design assumes:

- **The agent may be hostile.** Prompt injection, malicious tool output and compromised packages
  are expected. That is why every nest is isolated and why policy is enforced at the runtime
  boundary, not in the agent's code.
- **The gateway identity is the controller's.** Users never hold gateway credentials. A user
  compromise is a controller compromise and is treated as such.
- **Secrets are never baked into images in plaintext.** They live in encrypted envelopes and are
  attached as provider profiles at runtime.
- **The host is trusted.** Root on the appliance can read everything. This is an appliance, not a
  multi-tenant platform.

## What we guarantee in code

- every mutating action writes an `AuditEvent`
- API tokens are per-client and revocable
- login attempts are rate-limited (`django-axes`) and lockouts are per username + IP
- the API schema is frozen in CI, so an accidental permission or field change is visible in review
- model weights are never redistributed

## What is explicitly out of scope

- hostile root on the host
- side channels in the GPU driver
- supply-chain attacks against the base image (mitigated by digest pinning and SBOM, not solved)
