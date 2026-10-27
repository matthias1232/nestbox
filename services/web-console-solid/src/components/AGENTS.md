# components/ — reusable UI

## Purpose

Building blocks that know nothing about sandboxes, policies, or models. A
component here takes data through props and emits events through callbacks.

That rule is what makes them reusable: `TopologyGraph` renders nodes and edges,
not gateways and sandboxes.

## Rules

1. No fetching. A component that fetches cannot be used in two places with
   different data.
2. Props in, callbacks out. No global state reads.
3. One job per file, named for what it looks like or does.
4. No business rule. "A sandbox may only be stopped while running" belongs in
   the domain, and the component receives `disabled`.

## Files

| File | Responsibility |
|---|---|
| `StatusBadge.jsx` | A coloured label for a status string |
| `Drawer.jsx` | A side panel with a title and a close action |
| `TopologyGraph.jsx` | Layered node/edge diagram (gateway → sandbox) |
| `TrafficTable.jsx` | A dense, virtualisation-friendly event table |
| `FitBadge.jsx` | Model fit verdict with its reason |
| `Tabs.jsx` | Accessible tab strip |
| `Empty.jsx` | A consistent empty state |
