# policies/ — policy documents and rule approval

## Purpose

Edit the four-section policy, see what a change will actually do, and approve
rule drafts.

## The distinction this screen must never hide

Only `network_policies` applies to a running sandbox. The other three sections
need a recreate. The save response says which happened (`applies_now`,
`requires_recreate`, `rejected`), and this screen shows that answer rather than
a generic "saved".

## Rule drafts

A draft is not a rule. Approval is the only path that writes a rule, which is
what makes the workflow meaningful (ADR 0011).
