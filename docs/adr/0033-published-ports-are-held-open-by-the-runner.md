# ADR 0033 — Published ports are held open by the runner, and SSH is answered by it

Status: Accepted
Date: 2026-10-29

## Context

Since ADR 0021 a sandbox is given a port from its node's pool for SSH and one per
agent port, and `GET /sandboxes/{id}/connect` (ADR 0031) shows them. But nothing
listened: the rows stayed `pending`, the container had no port bindings (and the
runtime network is `internal`, so Docker could not have published them), and no
image carries `sshd`. The panel described a way in that did not exist.

## Decision

The **runner** holds the ports open. For each forward it listens on the node-side
port, published by Compose from `NESTBOX_FORWARD_RANGE` (default `22200-22299`,
which must equal the node's pool), and:

- `agent` and `custom` ports relay bytes to `<sandbox address>:<target>` on the
  sandbox network, which is how a web UI or an agent API is reached from the LAN;
- the `ssh` port is answered by the runner itself (asyncssh): public-key login with
  the sandbox's own key, then `docker exec` of the same sandbox, with a PTY when
  the client asks for one. `ssh host <command>` works, so `ssh host claude` does.

The control plane sends the **complete set** for a sandbox (`PUT
/v1/sandboxes/{name}/forwards`) with the public key, after create, publish,
withdraw and rotate, and records what the runner opened in `forwards.state`
(`active` or `failed`). The runner persists the set in a volume and restores it at
start; a read of the Access panel pushes the set again when the runner is behind,
so a recreated runner heals itself.

A forward reports two facts, shown separately: `listening` (the port is open on
the node) and `target_up` (something answers behind it). An agent that has not been
started leaves a port open with nothing behind it, and "active" would be a lie.

## What the SSH door does not do

No `scp`, `sftp`, or port forwarding (`-L`, `-R`, `-D`), so it cannot move data
around the egress policy. VS Code Remote-SSH needs those and does not work through
it; the console says so. SSH sessions are logged by the runner (who, when) but not
yet recorded in `sandbox_sessions`; that needs a callback to the control API.

## Alternatives rejected

**`sshd` in every image.** A daemon, host keys, capabilities and a user database
inside every sandbox; the sandboxes drop all capabilities, which `sshd`'s
privilege separation needs. And it would not reach the sandboxes that already exist.

**Docker `-p` bindings on the sandbox.** Impossible on an `internal` network, and
fixed at container creation, so a published port could never be added later.

**A thousand-port range.** Publishing 1000 ports starts 1000 relay processes. The
pool is 100 ports; a node that needs more sets it in Compose and in `nodes` together.

## Consequences

An agent port is as open as the node's address: anything on the LAN that can reach
`<node>:<port>` reaches the agent's UI. Bind the range to one interface with
`NESTBOX_FORWARD_BIND`, and put an authenticating proxy in front of a UI that has
no login of its own.

## What would invalidate it

A second runner per node, or a driver other than Docker (OpenShell), where the
listener's home moves with the driver.
