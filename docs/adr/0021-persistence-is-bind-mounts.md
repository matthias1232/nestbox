# ADR 0021 — Persistent data lives in bind mounts, one read-write mount per root

Status: Accepted
Date: 2026-10-27

## Context

A sandbox image is immutable: rebuilding it must not lose an agent's home, a
project checkout, or a conversation history. The options were a named volume per
sandbox, a bind mount from a host directory, or a sync process that copies data
in and out.

Copying in and out is the worst of the three: it makes "where is my data" a
question with two answers, and a crash during the copy loses the newer one. That
leaves volumes and bind mounts, and the choice depends on who has to see the
data.

An operator has to see it: back it up with the tools they already use, inspect
it when an agent does something strange, and delete it when a tenant leaves.
That points at a directory on the node.

## Decision

**A persistent root is a directory on a node; a mount binds it into a
sandbox.** Roots are rows (`persistent_roots`), mounts are rows (`mounts`). The
runtime receives bind mounts; the runner validates every source against
`NESTBOX_PERSISTENT_BASE` again, so a compromised control plane cannot mount
`/etc`.

**The host path is derived, never supplied.** `host_path(base, owner, kind,
slug)` produces `<base>/<owner>/<kind>--<slug>`. A request names a slug, not a
path, so a mount source cannot be influenced from outside.

**One read-write mount per root at a time.** Two containers writing the same
files is not a conflict the platform can resolve, so it refuses it: a partial
unique index `mounts_one_rw_per_root` and a `409 mount.conflict` with the
sandbox that holds the mount. Read-only mounts of the same root are unlimited —
that is how one project is shared with several sandboxes.

**A root belongs to a node.** A sandbox can only mount a root that lives on its
own node; the check is in the service, and the error says which node the root is
on. Mounting across nodes would silently depend on a shared filesystem that does
not exist yet.

**The image never carries the data.** A recipe produces an image tagged by its
content hash; the agents' homes and the project mounts are mounts. This is what
makes "delete the sandbox, create it again, the file is still there" true rather
than aspirational.

**Deleting is two operations.** Archive keeps the data and hides the root;
purge deletes the row. A root that is mounted cannot be deleted (`409
root.in_use`), so data loss is never a side effect of a delete.

**Usage is reported as measured, or as unmeasured.** The control plane cannot
`du` a remote node's disk, so it stores a measurement when the runner supplies
one and otherwise says it has none. A fabricated number would be worse than an
empty field.

## Alternatives rejected

**A named volume per sandbox.** Rejected: Docker owns the location, so an
operator cannot back it up with the tools they have, and the data disappears
with the volume name unless something else tracks it.

**A shared filesystem across nodes (NFS, Ceph) in this phase.** Rejected as
premature: it introduces a second source of truth for "is the file there" before
one node is proven. The mount model does not change when it arrives.

**Let the request name the source path.** Rejected: that is arbitrary host
access through an API. The path is derived from validated parts, and the runner
checks it again.

**Allow two read-write mounts and let the agents coordinate.** Rejected: agents
do not coordinate, and the failure is silent file corruption.

## Consequences

- `sandbox_service::create` loads mounts from the database and passes them to
  the runtime; nothing in a create request can add one.
- Rebuilding a sandbox keeps its data; a new sandbox from the same recipe sees
  an empty root until it is mounted.
- Backups are `rsync` over the persistent base; the platform does not need a
  backup API to make that possible.
- A root's `host_path` is stable for its lifetime. Renaming means a new root.
- The `agent_home` kind and the `project` kind differ only in prefix and
  ownership; both are the same mechanism, which is why the mount rules apply to
  both.

## What would invalidate this

A node that cannot give the container a host directory — a managed Kubernetes
cluster without `hostPath` — would need a volume abstraction behind
`RuntimeAdapter`. The rows and the one-writer rule would stay; only the mount
mechanism would change.
