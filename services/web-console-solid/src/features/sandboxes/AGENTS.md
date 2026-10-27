# sandboxes/ — the inventory and the detail drawer

## Purpose

The screen an operator lives in: every sandbox, its live state, and the actions
on it.

## What the page must show

- The lifecycle status the control plane believes, and the runtime facts it
  observed, side by side. They can disagree, and that disagreement is the
  interesting information.
- Address, uptime, and resource limits, because "is it running" is not the same
  as "is it reachable".
- The log tail without leaving the page, because a failure is diagnosed from the
  log.

## Rules

- Paging is server-side. The page never asks for everything.
- A lifecycle action is disabled when the current status does not permit it, and
  the reason is the domain rule, not a guess in the component.
- A destructive action asks once, and says what will be destroyed.

## Files

| File | Responsibility |
|---|---|
| `useSandboxes.js` | Paging, filters, selection, actions; no JSX |
| `SandboxListPage.jsx` | Table, facets, bulk bar |
| `SandboxDetail.jsx` | Drawer with overview, logs, shell, rules, inference |

## The shell tab

A typed line runs as `bash -lc <line>` through `POST /sandboxes/{id}/exec`. It
is request/response, not a PTY: the output comes back when the command ends,
capped at 64 KiB, with the exit code. That is enough to ask Hermes a question
or inspect a file, and it keeps every run in the audit trail as an argument
vector. An interactive terminal is a later transport, not a change to this one.
