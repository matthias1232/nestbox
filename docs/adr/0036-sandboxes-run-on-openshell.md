# ADR 0036 — Sandboxes run on the NVIDIA OpenShell gateway

Status: Accepted
Date: 2026-10-29

## Context

Until now a Nestbox sandbox was one Docker container that we created, and egress was
decided by our own Envoy proxy asking the control API per request. That was a
development stand-in for OpenShell (ADR 0007, ADR 0014): the policy model mirrored
OpenShell's, but nothing enforced it the way OpenShell does (Landlock, seccomp, a
trusted supervisor that mediates every connection and knows which *program* made it).

OpenShell 0.1.6 is a stable release with a Docker compute driver, and this host meets
its requirements (Linux 6.8 with Landlock, Docker 29).

## Decision

1. **The gateway is the original NVIDIA OpenShell**, installed from the pinned
   release package (checksum-verified) by `deploy/openshell/install-openshell.sh`,
   running as a systemd user service with the Docker compute driver and mTLS.
2. **A sandbox is an OpenShell sandbox.** `NESTBOX_SANDBOX_RUNTIME=openshell` makes
   the runner create it through the gateway (`openshell sandbox create --detach
   --restart-policy always`). Each sandbox is then two containers: the workload
   (no network) and its supervisor. The old Docker path stays for tests and for hosts
   without OpenShell.
3. **Nestbox decides nothing about a connection on OpenShell.** It compiles the
   effective rules of a sandbox into the gateway's `network_policies`
   (`domain/openshell_policy.rs`) and pushes them live. A deny is never an entry; what
   cannot be expressed is reported, not dropped. Startup-only sections (filesystem,
   Landlock, process) are left as the sandbox started with them.
4. **Every sandbox has a supervisor row**, created with the sandbox. On OpenShell it
   mirrors the supervisor container (status, version).
5. **The runner is the only adapter.** It holds the `openshell` CLI and the mTLS
   client bundle (CA certificate, client certificate and key; never the CA key).
   Observation (processes, inventory, metrics, terminal, SSH door) goes through one
   workload lookup that finds our own and OpenShell containers alike. Exec goes through
   the gateway, so a command runs inside the boundary. Web ports are reached through
   `openshell forward service`, because the workload has no network.
6. **Connection decisions come back** by reading each supervisor's OCSF log
   (`NET:OPEN` / `NET:REFUSE`), and are stored with the sandbox they belong to.

## Consequences

* The supervisor runs on the host network and resolves names with the host's
  resolver: services the agent must reach are addressed through the host
  (`host.openshell.internal`), e.g. the MCP hub on a loopback port.
* OpenShell inspects MCP calls itself; the policy allows the revisions agents speak
  (`2025-03-26` to `2026-07-28`).
* A native-TCP rule cannot name an IP literal; such a rule is relayed without a
  protocol.
* The gateway listens on the host (mTLS only) so the runner container can reach it.
* Bind mounts for persistent roots need `allow_driver_config` and
  `enable_bind_mounts` with resource admission off; the runner still validates every
  source against its persistent base first.
* A sandbox's own local state is lost when it is recreated onto OpenShell; data in
  persistent roots is kept.

## Not yet

* Provider credentials are still handed to the agent as environment values. OpenShell
  providers (the supervisor injects the credential only on approved requests) are the
  intended end state.
* Nestbox's global policies are not yet mapped to OpenShell's `--global` policy.
