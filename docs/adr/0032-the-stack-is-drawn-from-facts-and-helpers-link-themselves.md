# ADR 0032 — The stack diagram is arranged from facts, and a lone helper links itself

Status: Accepted
Date: 2026-10-29

## Context

A sandbox is rarely alone: it has an agent, tools it calls over MCP, a supervisor
that watches it, a gateway that carries it, a proxy it reaches the internet
through. The console showed each of those on a different screen, and linking a
helper sandbox to its agent (ADR 0030) was a manual step that was easy to forget.
An operator could not answer "what is this working with, and how is it wired?"
without opening six tabs.

## Decision

`GET /sandboxes/{id}/stack-topology` answers for the whole stack the sandbox
belongs to, in nine layers: you, the ports on the node, the sandboxes, their MCP
servers, the supervisor, the control plane and gateway, the egress proxy, the
destinations actually reached, the internet.

- **Arranged, not invented.** `domain::stack_topology::build` is pure. The service
  reads facts (ports with what the runner says is open, attachments, supervisors,
  gateways, the appliance's containers, the last minutes of egress traffic) and
  the builder arranges them. A fact nobody could read is `unknown`, never a guess.
- **Absence is drawn.** An agent with no supervisor gets an explicit "No
  supervisor" card; a missing proxy is the first problem listed. A helper tool
  without a supervisor is ordinary and is not flagged.
- **One status vocabulary** (`ok`, `warn`, `error`, `idle`, `unknown`, `absent`) so
  the console colours by meaning.
- **Every card has a log source** (the sandbox, a filtered view of it for the
  supervisor, or an appliance component through the runner's fixed list).

A helper links itself: a sandbox whose pack has role `tool` or `gateway`
(`agent_packs.role`, migration 0038) and which is created without a lead joins the
**only** agent on its node. With none there is nothing to join; with several, any
choice is a guess that puts the helper under the wrong agent, so it stays
standalone and the creator links it. The automatic choice goes through the same
rules as a named lead (one level deep, no self-lead).

## Alternatives rejected

**Link by name convention (`hermes-01-git`).** Works until someone names things
differently, and fails silently.

**Link to the most recent agent.** Wrong half the time, and wrong quietly.

**A client-side graph assembled from five endpoints.** Each endpoint is read at a
different moment, so the picture can show a port as open and its sandbox as stopped
at once. One read cannot.

## Consequences

The diagram is as live as its reads: the console polls every five seconds. The
runner is asked with a timeout, so a slow runner degrades cards to `unknown`
instead of failing the map.

## What would invalidate it

Stacks deeper than one level, or one sandbox in several stacks (ADR 0030 would
need to change first); or a second runner per node, where "the appliance's
containers" is no longer one answer.
