# ADR 0034 — The proxy identifies a sandbox by its address, and asks before it tunnels

Status: Accepted
Date: 2026-10-29

## Context

Checked against the running appliance, the egress path did nothing it was designed
to: every request from a sandbox was answered 503 and nothing was logged. Four
separate gaps stacked:

1. the proxy never presented its node id and token, so the control API refused
   every check (401) and fail-closed turned that into a deny;
2. no sandbox was ever identified, because the header that should have named one
   was never set by anything;
3. HTTPS CONNECT went to a certificate broker that has no CA on this deployment,
   and Envoy's ext_authz cannot carry a CONNECT to an HTTP service at all;
4. nothing read the proxy's log, so the Traffic tab stayed empty; and assigning a
   policy wrote a row the decider never read.

## Decision

- **Identity is the connection's address.** The Lua filter removes every
  `x-nestbox-*` header the client sent and sets `x-nestbox-source-ip` from the
  socket. The control API asks the runner which container holds that address, looks
  the sandbox up by name, and answers with `x-nestbox-sandbox`, which the access log
  records. A sandbox cannot name another sandbox.
- **The proxy proves who it is.** `start-egress-proxy.sh` writes the node id and
  token into an in-memory copy of the configuration and of the Lua filter. The
  token is a copy in `secrets/proxy`, readable by the proxy's user alone; the proxy
  mounts nothing else from `secrets/`.
- **A tunnel is decided by destination before a byte moves.** For CONNECT the Lua
  filter asks the control API itself (`httpCall`), because ext_authz cannot. The
  control API's router answers a CONNECT as a policy check. Anything but `200`,
  including no answer, refuses the tunnel. What is inside the encrypted stream is
  not read, and the log says so (`connect: true`, no path).
- **The policy a sandbox carries is the policy that is enforced.** The decider
  compiles the governing policy's document (the global override, otherwise the
  assigned policy) into rules at decision time, behind every stored rule, and
  narrows stored rules to the ones that apply to the sandbox. A rule on a policy
  applies only to sandboxes carrying it.
- **The runner ships the proxy's log.** It already holds the Docker socket and the
  node token; it posts the access log to the telemetry endpoint every two seconds
  and resumes from the same point after a failure.

## Alternatives rejected

**A `x-nestbox-sandbox` header set by the sandbox or its image.** Whoever can write
it can claim to be anyone.

**Terminating TLS for every tunnel.** It needs a CA trusted by every client image;
a client without it gets a TLS error, not a decision. It stays available as a
deliberate mode (the certificate broker and the loopback listener are unchanged).

**Letting ext_authz fail open for CONNECT.** The default must be that a proxy which
cannot ask refuses.

## What would invalidate it

Several sandboxes sharing one address (host networking, NAT between the sandbox
network and the proxy), or a second node whose proxy is on another network, where
the source address no longer names one container.
