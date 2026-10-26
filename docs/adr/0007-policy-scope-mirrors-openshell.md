# ADR 0007 — Policy scope mirrors OpenShell

Status: Accepted
Date: 2026-10-26

## Context

The product needs policy templates, per-sandbox assignment, and an admin
override. The natural instinct is to invent a scope hierarchy: organization →
environment → cluster → gateway → sandbox, with inheritance at each level.

Reading the OpenShell documentation (`docs/how-it-works/policies/manage-policies.mdx`)
shows that OpenShell has exactly two scopes: a per-sandbox policy and a single
gateway-global policy. A global policy **replaces** every sandbox policy and
**blocks** further sandbox-level edits until it is deleted.

Inventing a third scope would mean Nestbox computes an effective policy that
OpenShell cannot express, and the GUI would show a state the runtime cannot
enforce. That gap is where "the policy says allow but traffic is blocked"
support tickets come from.

## Decision

The policy model mirrors OpenShell exactly.

- `policies.scope` is `sandbox` or `global`. No other value is permitted.
- Sandbox policies are assigned via `sandbox_policies`, which carries the
  desired version and precedence.
- A global policy is an admin override that replaces all sandbox policies and
  blocks sandbox-level edits. Deleting it restores the previous selection, and
  the delete can fail if the restored policies would be invalid — the API
  surfaces that failure rather than silently dropping the global policy.
- Templates live in Nestbox (`policy_templates`) and *produce* a policy
  document. They are a Nestbox convenience, not a fourth OpenShell scope.

## Alternatives rejected

**Full inheritance chain with cluster and gateway layers.** Rejected: OpenShell
cannot enforce it. Nestbox would flatten it into one document at apply time,
which means the GUI edits a hierarchy that does not survive the round trip.

**Per-gateway policy.** Rejected for the same reason. A gateway's configuration
is operator settings, not policy; see ADR 0012.

**Only sandbox policies, no global.** Rejected: loses the real admin override
that OpenShell provides, so an incident that needs "block everything now"
cannot be expressed.

**Derive policy from templates at runtime instead of storing a document.**
Rejected: the effective policy must be inspectable and diffable exactly as the
gateway enforces it. Storing the resolved document is what makes `diff` and
`rollback` honest.

## Consequences

- The GUI must present exactly two scopes. A user asking "which policy applies
  to this cluster" gets "the sandbox policy, or the global override if active".
- Templates are clearly a composition tool, labelled as such, so nobody assumes
  they are enforced objects.
- When OpenShell adds a scope, this ADR is superseded and the schema gains a
  value. The binary constraint is what makes that visible rather than silent.

## What would invalidate this

- OpenShell introducing scoped policies above the sandbox, for example a
  workspace policy. Then the `scope` constraint gains a value and the
  resolution service gains a precedence rule, which is an additive change.
