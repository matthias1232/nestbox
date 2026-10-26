# Gateways and supervisors

## What each layer does

| Layer | Responsibility |
|---|---|
| **Gateway** | Provisions sandboxes, stores provider credentials, delivers policy, resolves providers, provides the SSH tunnel, relays between peers |
| **Supervisor** | Runs inside the sandbox boundary, applies the delivered configuration, enforces policy |
| **Sandbox** | The workload and its own policy document |

The gateway is the control plane for OpenShell. All control traffic between a
client and a running sandbox flows through it.

## Why configuration is not policy

They have different lifecycles, so they are different objects (ADR 0012).

A sandbox policy change is a hot reload. A gateway configuration change — the
compute driver, peer routing, interceptors, middleware registration — is static
and often needs a restart that affects every sandbox on that gateway.

Merging them would make the safe operation wait for the dangerous one.

## Drift and sync

Each gateway carries two versions:

- `settings_version` — what the control plane wants
- `settings_applied_version` — what the gateway runs

When they differ, the gateway is **drifted**. The console shows a badge, a diff,
and a "Sync now" button.

The scheduled `gateway-config-sync` job does the same thing every 20 minutes for
any drifted gateway, so a drift that nobody noticed still converges. Every sync
is a job with a log and an audit record, so "did it land" is a fact.

A drifted or unhealthy gateway is not a placement target. Placing a sandbox on a
misconfigured gateway is worse than waiting for the sync.

## Supervisors are observed, not configured

Nestbox reports what a supervisor is doing: health, configuration dirtiness, and
the result of the last policy apply. The runtime exposes these directly, so the
console reads them rather than inferring them.

There is no "supervisor policy", because the runtime has no such object. Creating
one would be a UI for a state that does not exist.

## Multiple gateways

The schema is multi-gateway from the start, even though M1 runs one.

- A cluster contains gateways; a gateway belongs to a node.
- Each gateway has its own public address and overlay address.
- Gateways relay to each other through peer routing.
- Sandbox placement chooses a gateway by capacity, zone, and drift.

The operations that matter with several gateways are the repair paths, and they
are modelled now: restart, re-sync, drain, migrate a sandbox off, and re-register
a gateway that lost its credentials.

## What the console shows

| Screen | Content |
|---|---|
| Gateways | List with status, version, drift, capacity, last sync |
| Gateway detail | Settings editor, diff against applied, sync button, logs, metrics |
| Supervisors | List with health, config dirtiness, last policy apply |
| Cluster | Topology graph: node → gateway → supervisor → sandbox |

Live logs come from the gateway's OCSF JSONL output and its metrics endpoint,
not from a log file on disk. That is what makes them work for a remote gateway
as well as a local one.

## Where this lives in the code

| Concern | File |
|---|---|
| Types | `services/control-api-rust/src/domain/gateway.rs`, `supervisor.rs` |
| Registry and health | `services/control-api-rust/src/services/gateway_registry.rs` |
| Settings and drift | `services/control-api-rust/src/services/gateway_settings.rs` |
| Sync | `services/control-api-rust/src/services/gateway_sync.rs` |
| Console | `services/web-console-solid/src/features/gateways/`, `supervisors/` |
