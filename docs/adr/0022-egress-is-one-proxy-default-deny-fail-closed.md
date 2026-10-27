# ADR 0022 — One egress proxy per node, default deny, fail closed

Status: Accepted
Date: 2026-10-27

## Context

Phase 1 had no egress at all: the sandbox network was `internal: true`, the
gateway answered 403 to everything, and `network_rules` was a table nothing read.
The Admin Center promises the opposite — a traffic log that shows what a sandbox
did, and an "Allow this" button that changes what it may do.

That promise forces a choice about where the decision lives. Two places is one
place too many: a rule enforced partly in a proxy config and partly in the
database is a rule that can disagree with itself, and the disagreement shows up
as "the console says blocked but the request succeeded".

## Decision

**One forward proxy per node, Envoy, and it decides nothing.** Every outbound
connection from a sandbox goes through it. For each request it asks the control
API `POST /internal/policy/check`, which runs the single pure function
`domain::egress::decide` over the enabled rules and open windows. The proxy's own
configuration contains no allowlist.

**Default deny.** No matching rule and no open window is `deny`, with the reason
`default_deny` and a suggestion, not an empty page.

**Fail closed.** `failure_mode_allow` is off: a control API that cannot answer is
a deny with reason `backend_unavailable`. The alternative — let traffic through
while the decider is down — turns an outage into an outage plus a policy bypass.

**A sandbox with no proxy environment has no way out.** The network stays
`internal: true`; `HTTP_PROXY`/`HTTPS_PROXY` exist for tools that respect them,
and the iptables redirect in the proxy container catches some that do not. The
`internal` network remains the hard boundary for a raw socket, and the console
says that rather than implying the redirect is complete.

## Alternatives rejected

**mitmproxy.** Terminates TLS out of the box, but puts a Python stack in the path
of every request and a second rule engine next to ours.

**agentgateway.** A reverse proxy with explicit routes: it would duplicate
`network_rules` in a config file, and then two things claim to be the policy.

**Decide inside the proxy from a pushed snapshot.** Faster, and it makes the
console's "deployed" badge meaningless, because the proxy could be running a week
old rule set with no way to tell.

## Consequences

- Every request costs one sub-millisecond round trip to the control API; the
  decider caches an answer for 5 s per key, shorter for a window that is about to
  expire.
- `nodes.policy_hash_reported` against `policy_compilations.hash` makes
  "deployed" a comparison an operator can check, including when it is false.
- The proxy is the only component with `NET_ADMIN`/`NET_RAW`, and only for its own
  redirect rules. That exception is documented in `deploy/README.md`.
- A sandbox without the node CA in its image is tunnelled, not inspected. The
  console labels those rows instead of showing an empty path.

## What would invalidate this

A proxy that can enforce a compiled rule set with a verifiable, signed snapshot
and report its hash back would remove the per-request round trip without losing
the "deployed" fact. Until then the control API answers every request.
