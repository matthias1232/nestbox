# Nestbox — product intent and index

## What it is

Nestbox is a control plane for running AI agents in isolated sandboxes, with
policy, observability, and a web console.

The problem it solves: an AI agent needs tools, network access, and model
credentials to be useful, and every one of those is a way for the agent to do
something the operator did not intend. Nestbox makes each of those grants
explicit, visible, and reversible.

## The four things it gives an operator

1. **Isolation you can see.** Every workload runs in its own sandbox with its
   own filesystem and process policy.
2. **Egress you can govern.** Every connection is decided by a policy, and the
   decision is visible in a live console.
3. **A model that is reachable but not exposed.** Credentials are injected at
   the gateway, never in the sandbox.
4. **A record.** Every action, every policy change, every approval.

## Product principles

**Nothing happens silently.** A rule takes effect only after review. A policy
change states whether it applies now or needs a restart. A sync is a job with a
log. The console shows the consequence before it happens, and the result after.

**The runtime is the authority.** Nestbox mirrors what OpenShell can actually
enforce. It does not invent a scope, a setting, or a state the runtime cannot
represent, because a UI that shows a state the runtime does not have is worse
than a UI that shows less.

**Local and cloud are the same shape.** A model is a provider profile with an
endpoint. Whether that endpoint is `localhost` or a cloud API changes one field
and nothing else in the policy.

**The deployment is the product.** This runs on a machine the operator owns,
possibly a single box with one GPU. It must work with no GPU at all, degrading
to cloud models rather than refusing to start.

## How to read this brain

| Document | Answers |
|---|---|
| [control-plane.md](control-plane.md) | How a request reaches a container |
| [policies.md](policies.md) | How policy is modelled, resolved, and enforced |
| [telemetry.md](telemetry.md) | How the live console gets data |
| [models.md](models.md) | How models are installed and served |
| [gateways.md](gateways.md) | Gateways, supervisors, and sync |
| [security.md](security.md) | The threat model and the controls |

## The scope of the current phase

M1 builds the foundation: the control store, the API, the console, the policy
and telemetry model, model management, and the operational jobs. Multi-gateway
routing, ACME renewal, and sandbox migration are modelled but not executed.
See `../architecture/scope.md` for the honest list.
