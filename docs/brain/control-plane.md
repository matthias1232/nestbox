# The control plane

## The shape of a request

```
Browser
  │  session cookie + CSRF token
  ▼
api/            parse, validate, check permission, delegate
  │
  ▼
services/       the rule: what is allowed, what happens next
  │
  ├──► store/       read and write, always tenant-scoped
  │      │
  │      ▼
  │    PostgreSQL
  │
  └──► runtime/     execute against the gateway or container
         │
         ▼
       OpenShell gateway ──► sandbox
```

A request never skips a layer. The API does not query the database. The service
does not know about HTTP. The runtime does not know about tenants.

## Why the layers are separate

Because the prototype merged them, and the result was a file where a rule, a
SQL statement, and an HTTP status code lived together. Changing the rule meant
reading all three.

The separation has a cost: a value may pass through three layers unchanged. It
has a benefit that dominates: to change a rule you open one file, and to know
whether something is a rule or a query you look at the folder name.

`tests/architecture.rs` enforces the boundaries by reading the source. A
violation fails the build.

## The tenant scope

Every repository call takes an `environment_id`. It is not optional and it is
not defaulted. The reason is that the default is where leaks come from: a
forgotten filter returns every tenant's data, and the bug is invisible in a
single-tenant test.

The database enforces the same rule independently via row-level security, so a
bug in the application filter still cannot return another tenant's rows. Two
independent controls, because this is the failure that matters most.

## The request identity

Every request gets a `request_id`. It appears in the response, in the log, and
in any error envelope. When a user reports "the save failed", the id names the
exact log line. Without it, support means searching by timestamp and guessing.

## Long-running work

Anything that touches more than a handful of resources is a **job**, not a
synchronous request.

- A bulk action over selected sandboxes creates a job with a target set.
- A model install creates a job with stages.
- A policy rollout creates a job per wave.
- A gateway sync creates a job.

A job has progress, partial failures, a result, and a log. The console shows it
in a list, and it is retryable. This is what makes "select all 10,000 and stop
them" a safe operation rather than a timeout.

## Recurring work

Recurring work is a **schedule**, which creates jobs.

| Schedule | Cadence | Why |
|---|---|---|
| `gateway-config-sync` | 20 minutes | Converge drifted gateways |
| `policy-reconciliation` | 5 minutes | Detect policy drift on supervisors |
| `certificate-expiry-check` | daily | Warn before expiry |
| `address-pool-reconcile` | hourly | Release stale assignments |
| `telemetry-partition-maintenance` | daily | Create next partition, drop expired |
| `provider-refresh` | 15 minutes | Renew expiring credentials |
| `model-update-check` | weekly | Detect newer model versions |
| `session-cleanup` | hourly | Expire sessions and traffic windows |

Every schedule shows its next run, last run, and last result. Every run is a
job with a log, so "did the sync happen" is a fact rather than a guess.
