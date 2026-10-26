# ADR 0008 — Only the network section is hot-reloadable

Status: Accepted
Date: 2026-10-26

## Context

A policy document has four sections: `filesystem_policy`, `landlock`,
`process`, and `network_policies`. It is tempting to present the whole document
as editable with an "Apply" button, as if every change takes effect on a
running sandbox.

OpenShell does not work that way. The network section reloads into a running
sandbox. Filesystem, Landlock, and process settings are fixed at workload
start: OpenShell **rejects** the change after the workload starts, and the
sandbox must be recreated.

A GUI that hides this produces a user who believes a change applied when it did
not.

## Decision

The GUI states the difference, in the editor, at the point of edit.

- `network_policies` edits apply to a running sandbox. Existing connections
  open under the old rules are closed at the reload boundary, and the GUI says
  so, because a keep-alive or WebSocket will drop.
- `filesystem_policy`, `landlock`, and `process` edits require recreating the
  sandbox. The GUI marks the section as *requires restart*, shows which
  sandboxes are affected, and offers "recreate affected sandboxes" as an
  explicit action with a diff preview.
- Adding a filesystem path is accepted by validation but does not affect the
  running workload; the GUI says "saved, applies after recreate".
- Removing a path, or changing `include_workdir`, Landlock, or process
  settings, is rejected after start. The API returns a typed error naming the
  section.

## Alternatives rejected

**Present the whole document as hot-applicable.** Rejected: it is false, and
the failure is silent from the user's perspective — the save succeeds, the
runtime keeps the old behaviour.

**Forbid editing the startup sections entirely.** Rejected: a user must be able
to change them, they just need a restart. Hiding the capability forces them to
recreate the sandbox through some other path.

**Automatically recreate the sandbox when a startup section changes.** Rejected
as the default: recreation destroys the workload's live state. It must be a
deliberate action with a visible consequence, offered as a one-click follow-up
rather than applied silently.

## Consequences

- The editor is not uniform. Three sections carry a restart warning and one
  does not, which is slightly more complex and considerably more honest.
- A policy save has two possible outcomes: applied now, or applied on next
  start. The UI must show which, and the API must return that distinction
  (`applies_now`, `requires_recreate`, `rejected`).
- The audit record distinguishes the two, so an operator can see that a change
  is pending a restart rather than believing it is live.

## What would invalidate this

- OpenShell gaining live reconfiguration of filesystem or process boundaries,
  which is unlikely because it contradicts the isolation model. Then the
  distinction collapses and the editor becomes uniform.
