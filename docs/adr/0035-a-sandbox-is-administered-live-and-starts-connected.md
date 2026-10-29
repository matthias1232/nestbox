# ADR 0035 — A sandbox is administered live, and its agent starts connected

Status: Accepted
Date: 2026-10-29

## Context

Opening a sandbox should be enough to start working: connect with the client you
have, see what it is doing, give it more room, move a folder to another sandbox, and
find the agent already pointed at a model through the proxy. Checked against the
running appliance, most of that was missing or inert: the deck's Dashboard,
Traffic, MCP, Terminal, Processes and Inventory tabs were placeholders, the metrics
sampler reported nothing, a mount was a row the container never saw, DNS rules were
stored and never consulted, and an agent had no model endpoint.

## Decision

- **SSH access is administered per sandbox** (migration 0040): public keys an
  operator adds, and temporary user name/password logins (15 minutes to a day,
  revocable). Only `sha256(salt || password)` is stored and sent to the runner,
  which checks it and the expiry on every attempt and slows a wrong guess. The
  password is in one response and in no log or audit record.
- **Resources change live.** `PUT /sandboxes/{id}/resources` remembers the figures
  in the spec and applies memory, CPU and the process limit to the running container
  (`docker update`, called directly because the SDK's `update()` lacks two of them).
  The answer says applied, saved for the next start, or saved but not applied.
- **Host load is the host's.** The runner reads the host's `/proc`; the console shows
  CPU, memory, disk and, separately, what running sandboxes have been *promised*.
- **Mounts apply by replacing the container.** Docker cannot change binds on a running
  container, so `POST /sandboxes/{id}/recreate` builds a new one with the current
  mounts and keeps the identity (name, ports, keys, policy, stack). The console
  states what survives (persistent roots) and what does not (anything else written in
  the old container) before it asks. Recipe sandboxes are refused.
- **An agent starts connected.** Hermes, Claude Code, OpenCode and OpenChamber get the
  OmniRoute profile (both API families, one model) and, for Claude Code and OpenCode,
  the config files they read (`~/.claude/settings.json`, `opencode.json`) with the
  proxy and the model, never the key. A gateway with an address is egress: through the
  proxy, decided and logged. Only a container on the sandbox network bypasses it.
- **DNS block rules are enforced** in the decision: a name an operator blocked is
  denied whatever a network rule or window says.

## Alternatives rejected

**Live mount with nsenter from the runner.** It needs a privileged runner and writes
into another container's mount namespace; the failure mode is a sandbox with a
different filesystem than its record says.

**One shared password per sandbox.** A credential nobody can tell apart or revoke
alone.

**Putting the API key into the agent's config file.** The environment already carries
it; a file in a persistent home outlives a rotation.

## What would invalidate it

A runtime that can add a mount to a running sandbox (an OpenShell driver), where
`recreate` becomes `attach`.
