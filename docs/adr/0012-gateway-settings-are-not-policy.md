# ADR 0012 — Gateway settings are not policy

Status: Accepted
Date: 2026-10-26

## Context

The question was raised directly: should a gateway and a supervisor be able to
carry policies and settings of their own, and what should a "sync" button do
across multiple gateways?

Reading the OpenShell architecture answers the first part precisely. The
gateway is a control plane that provisions sandboxes, stores provider
credentials, delivers policies, resolves provider environments, and provides
the SSH tunnel. Its own configuration is an operator config file:
compute driver, peer routing, interceptors, supervisor middleware, resource
admission, OCSF and OTLP export, TLS SANs, service forwarding.

That configuration is **not** policy. It is static, it mostly needs a gateway
restart, and it has no per-sandbox scope. Supervisor middleware registration is
explicitly operator-owned and requires a restart.

## Decision

Gateway configuration and sandbox policy are separate concepts with separate
lifecycles.

| Concern | Where it lives | Change semantics |
|---|---|---|
| Sandbox policy | `policies`, `network_rules` | Hot for the network section |
| Gateway configuration | `gateways.settings` (jsonb) | Versioned, applied by sync, may need a restart |
| Supervisor state | observed, not authored | Reported as drift |

**Sync and drift:**

- `gateways.settings_version` is desired; `settings_applied_version` is what the
  gateway runs.
- Desired ≠ applied is **drift**, shown as a badge with a diff.
- "Sync now" pushes the desired config; the scheduled `gateway-config-sync` job
  does the same every 20 minutes for any drifted gateway.
- A drifted or unhealthy gateway is not a placement target for new sandboxes.

**Supervisors are observed, not configured.** Nestbox reports their health,
their config dirtiness, and their policy-apply results, and surfaces drift. It
does not invent a supervisor policy, because the runtime has no such object.

## Alternatives rejected

**One settings blob covering gateway and sandbox.** Rejected: the lifecycles
differ. A sandbox policy change is a hot reload; a gateway change may restart
the control plane for every sandbox on it. Merging them makes the safe case
wait for the dangerous one.

**A gateway-scoped policy.** Rejected per ADR 0007: OpenShell has no such scope.

**Manual per-gateway configuration through the CLI only.** Rejected: it does
not scale past two gateways and it makes drift invisible. The whole point of a
control plane is that the desired state lives in one place and the components
converge to it.

**Automatic sync with no visibility.** Rejected: an operator must be able to see
what changed and when it landed. Every sync is a job with a log and an audit
record.

## Consequences

- Two version counters per gateway, and a drift indicator derived from them.
- Sync is idempotent and safe to retry, because the desired config is a full
  document rather than a diff.
- A gateway that requires a restart to apply a change makes that explicit
  rather than pretending the change is live.
- Placement must consult drift, which couples scheduling to configuration state.
  That coupling is correct: placing a sandbox on a misconfigured gateway is
  worse than waiting.

## What would invalidate this

- OpenShell making gateway configuration hot-reloadable and per-workspace, which
  would move some settings into a scoped model and require revisiting ADR 0007.
