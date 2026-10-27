# egress-proxy/ — the only way out of a sandbox

## Purpose

Envoy configured as a forward proxy. Every outbound connection from a sandbox
passes through it, and for every request it asks the control API what to do.

It owns **no rules**. The rule set lives in `network_rules` and is compiled by the
control API (ADR 0001, 0013). If this folder ever grows an allowlist, there are
two policies and one of them is wrong.

## The two promises

1. **Default deny, fail closed.** No matching rule is a deny. A control API that
   does not answer is a deny (`failure_mode_allow: false`). A sandbox with no
   rules reaches nothing.
2. **What it could not see, it says it could not see.** An HTTPS request without
   this node's CA in the image is a CONNECT tunnel: host, port, SNI, bytes. The
   console labels it, rather than leaving an empty path that looks like a bug.

## Files

| File | Responsibility |
|---|---|
| `Dockerfile` | The image, pinned by digest |
| `envoy.yaml` | Listener 3128, ext_authz to the control API, JSON access log, 64 KiB buffer |
| `extensions/mcp.lua` | Recognise JSON-RPC MCP and put the tool name in dynamic metadata |

## Why Envoy

mitmproxy terminates TLS out of the box but puts a second technology stack and a
Python event loop in the path of every request. `agentgateway` is a reverse proxy
with explicit routes, which would duplicate `network_rules` with a config file.
Envoy is one static binary, has `CONNECT` termination with its own CA, and the
decision stays where the rules are.

## The one component with extra rights

`NET_ADMIN` and `NET_RAW`, and only for the redirect rules that catch a tool which
ignores `HTTP_PROXY`. The sandbox network is `internal: true`, so a raw socket
still has no route out; the capabilities are for the proxy container's own
iptables, not for the sandbox. This is documented in the deploy README because it
is the only exception to "nothing is privileged".

## Not published on the host

An unauthenticated forward proxy on a LAN interface is an open relay with a
friendly name. Sandboxes reach it over the `runtime` network as
`egress-proxy:3128`; an operator who wants to test it goes through
`docker exec`.

## Open work

- CONNECT termination (`connect_config.terminate_tls`) and the per-node CA, plus
  the leaf certificate the proxy presents.
- The response side of MCP (result, `is_error`, duration).
- The status endpoint that reports `policy_hash_reported`, which is what makes
  the console's "deployed" badge true.
- Body capture into the access log (base64) once the CA path exists; without
  termination there is no body for HTTPS.
