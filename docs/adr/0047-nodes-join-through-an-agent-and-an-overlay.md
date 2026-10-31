# ADR 0047 — Nodes join through an agent and a WireGuard overlay; this replaces ADR 0029

Status: Accepted
Date: 2026-10-31

## Context

ADR 0029 kept one runtime adapter per process and refused to create nodes. The goal is now to attach a machine in minutes, behind NAT, at any cheap host.

## Decision

1. **A node runs the node package**: runner, egress proxy, optionally an OpenShell gateway, and a small **node agent** (join, heartbeat, image sync, snapshots).
2. **Overlay: WireGuard, hub and spoke.** The control plane is the hub; each data center has its own /24. A node needs no open port. The runner API, forwarded ports and data transfer ride on overlay IPs unchanged.
3. **Join by one-time token** (15 minutes, bound to a data center and a site). The node exchanges it for its identity: a WireGuard key and a node token, both stored as vault locators.
4. **Four ways to install**: a one-line script, cloud-init text for a host's user-data field, a bootable ISO, and a provider driver (ADR 0050).
5. **The control API holds an adapter registry keyed by `node_id`** instead of one adapter. Every service call that used `ctx.runtime` dispatches by node. An unreachable node is a typed error. Reservations stay serialised per node in PostgreSQL.
6. **Placement** is explicit at first (the caller names a node); a scheduler that reads budgets comes later.

## Alternatives rejected

* **A node URL read per request.** Moves credential lookup into the hot path.
* **Kubernetes.** Replaces the runner contract and bind-mount persistence (ADR 0021).
* **A public port on every node.** Does not work behind NAT and enlarges the attack surface.

## Invalidation conditions

Revisited when overlay throughput becomes the bottleneck, or when a customer forbids a provider-operated hub.
