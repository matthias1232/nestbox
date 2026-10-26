# ADR 0011 — Rule from event is a two-step workflow

Status: Accepted
Date: 2026-10-26

## Context

The product promise is: see a suspicious connection in the live console, click
it, and get a rule. The dangerous implementation is that the click creates the
rule and enforcement follows immediately, because a misclick then blocks a
production workload and the operator may not notice which connection they hit.

OpenShell itself models this as a workflow: `rule get` shows pending network
rules, and `rule approve` / `rule reject` decide them. The runtime already
distinguishes a proposed rule from an enforced one.

## Decision

Rule creation from an event always goes through a draft.

1. An operator selects one or more events in the live console.
2. The system creates a `rule_draft` with a prefilled, editable rule: sandbox
   scope, destination, protocol, port, and the proposed action.
3. The `rule_draft` is shown with a **diff against the currently enforced
   policy**, so the operator sees exactly what would change.
4. The operator approves or rejects explicitly.
5. Only approval creates or modifies a `network_rule` and triggers a rollout.

Bulk approval is supported, but it shows the combined diff first and still
requires one explicit confirmation.

## Alternatives rejected

**Immediate enforcement on click.** Rejected: a mistake blocks traffic at the
moment of the mistake, with no review step. The product's value is control;
control that acts before review is not control.

**A modal "are you sure" over immediate enforcement.** Rejected: it is a
confirmation without information. The operator sees a dialog, not the diff, so
they cannot judge the change.

**Only a prefill form, no draft record.** Rejected: then there is no record of
what was proposed and by whom, no way to review a proposal asynchronously, and
no match with the runtime's own approve/reject model.

## Consequences

- Two records exist: the draft and the enforced rule. The draft is durable, so
  proposals survive a page reload and can be reviewed by a second operator.
- The diff step requires computing the effective policy before and after, which
  is the same code used by the policy editor. That reuse is deliberate.
- A pending draft can become stale if the policy changes underneath it. The
  approve step re-validates and fails with a clear error rather than applying a
  diff computed against an old base.
- Bulk operations open a job rather than a synchronous write, so the operator
  sees progress and partial failures.

## What would invalidate this

- Nothing foreseeable. If a "trusted auto-approve" mode were requested for a
  narrow case, it would be an explicit per-environment setting with its own
  audit record, not the removal of the draft step.
