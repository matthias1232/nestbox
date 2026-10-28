# ADR 0030 — Sandboxes that belong together are linked explicitly, one level deep

Status: Accepted
Date: 2026-10-28

## Context

An agent such as Hermes needs helper sandboxes to run, and the list showed every
one of them as an unrelated row. Each helper is a sandbox in its own right — own
container, own policy, own traffic — so the relation is membership, not
containment.

## Decision

`sandboxes.parent_id` points at the sandbox that leads the stack. A stack is a lead
and its members, one level deep: a lead is not itself a member, a lead with members
cannot become one, and nobody leads themselves. The service enforces this, so the
console shows the server's sentence instead of reimplementing the rule.

Deleting a lead frees its members (`ON DELETE SET NULL`). They are real sandboxes
with state; deleting them with the lead would destroy work nobody asked to
destroy. The console says so before the delete.

## Alternatives rejected

**Infer the group from the name or the recipe.** A naming convention breaks the
first time someone renames a sandbox, and a recipe describes how a sandbox was
built, not which others it needs.

**A stack table with its own id.** One more row to keep in step for a relation that
is a single pointer; it would also invite stacks of stacks.

**An unlimited tree.** The list would need a reading order. One level reads at a
glance, and a second level has no use that has been asked for.
