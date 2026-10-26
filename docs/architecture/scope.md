# Scope of the current phase (M1)

This document is deliberately explicit about what M1 does **not** do. An honest
limit is more useful than a silent gap, because a limit can be planned around
and a gap cannot.

## Built in M1

| Area | What is built |
|---|---|
| Control store | PostgreSQL, migrations, row-level security, seed environment `default` |
| API | Typed domain, layered architecture, session auth, RBAC, tenant scope, cursor paging, facets, SSE |
| Console | Sandbox list with server paging and bulk jobs, live traffic console (network, DNS, inference), policy editor with diff and rollback, rule drafts, gateways with drift and sync, models with live fit, jobs and schedules, audit |
| Policy | Four-section model, two scopes, templates, rollout, approval workflow, temporary traffic windows |
| Telemetry | Three live streams, partitioned history, retention by partition drop |
| Models | Catalog as data, hardware detection, fit classification, install and update jobs, local and cloud |
| Ingress | Rules, certificates inventory, address pools, forwards, service exposures — data model and API |
| Operations | Jobs, schedules, audit, live logs per component |

## Modelled but not executed in M1

These have a complete schema and API surface. The execution path is M2.

| Capability | Why not yet |
|---|---|
| Multi-gateway overlay routing | Needs a second real gateway to test against |
| Ingress renderer (generate nginx config per gateway and reload atomically) | Needs a real gateway to validate the generated config |
| ACME certificate issuance and renewal | Needs a public domain or a PKI decision |
| Internal CA issuance | Same |
| Sandbox migration between gateways | The data model is ready; the live move needs two gateways |
| Transparent proxy interception | The mode and model exist; issuing the trust CA is M2 |
| mTLS between gateway and control plane | Needs a certificate authority, which is M2 |

## Not in scope at all

| Capability | Reason |
|---|---|
| Kubernetes or Helm deployment | The target is an appliance on one machine; Kubernetes would contradict that |
| Multi-region control planes | Postgres is the single store; regions would need replication, which is a different architecture |
| A marketplace of third-party packs | Out of the current product statement |
| Billing or metering | Not part of the product |

## Known environment limits

The development machine has no GPU. That means:

- Hardware detection is verifiable and correctly reports no GPU.
- The model fit classification is fully verifiable.
- The download and server-start path cannot be exercised end to end on that
  machine. It is built in full, and verified against the classification, the
  job stages, and the error paths.

On a machine with a GPU, the same code runs unchanged. This is stated rather
than hidden, because a claim of "tested" that excludes the one path that needs
hardware would not be honest.

## What M1 proves

The structure. If the sandbox list arrives with server paging, facets,
virtualisation, and bulk jobs without modifying an unrelated module, the layering
holds and every later feature is cheaper. If it required touching everything, the
structure was wrong and was corrected before continuing.
