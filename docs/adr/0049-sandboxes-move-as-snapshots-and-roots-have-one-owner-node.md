# ADR 0049 — Sandboxes move as snapshots, and a root lives on one node at a time

Status: Accepted
Date: 2026-10-31

## Context

Work should move to a rented GPU machine and back: project files, persistent data and the agent's login (`.agent-state`), without keeping the machine.

## Decision

1. **What moves**: the sandbox definition (image digest, policy, mounts, MCP, packs), its persistent roots, and the agent home with the login. Images are not moved; they are synchronised (ADR 0048).
2. **A restic repository per data center** (S3 or a rest-server on a home node), encrypted and deduplicated, replaces the tar.gz root backups. Backup and migration are the same mechanism.
3. **A lease** in PostgreSQL says which node holds a root. Only the holder runs it, so two live copies of one login cannot exist.
4. **Move job**: live snapshot, short stop, delta snapshot, restore on the target, recreate with the same identity and policy, verify checksums and health, switch the lease, keep the source as a cold copy.
5. **Deleting a node is refused** while a root on it has changes newer than its last snapshot.
6. **Control-plane backup**: pgBackRest to S3; the vault key is backed up separately, because without it every stored key is unreadable.

## Alternatives rejected

* **rsync between nodes.** No history, and the node must be alive for the way back.
* **Shared network filesystem.** Latency makes it unusable across the internet.

## Invalidation conditions

Revisited when roots reach sizes where snapshots are too slow, or when a root must be live on two nodes.
