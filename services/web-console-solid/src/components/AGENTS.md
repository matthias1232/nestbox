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
5. No arithmetic that decides something. `ResourceBars` computes pixel widths
   from server numbers; it must not decide whether a model fits. That decision
   arrives as `status` and `fit`.

## Files

| File | Responsibility |
|---|---|
| `StatusBadge.jsx` | A coloured label for a status string |
| `Drawer.jsx` | A side panel with a title and a close action |
| `TopologyGraph.jsx` | Layered node/edge diagram (gateway → sandbox) |
| `TrafficTable.jsx` | A dense, virtualisation-friendly event table |
| `FitBadge.jsx` | Model fit verdict with its reason |
| `ResourceBars.jsx` | "Have versus need" bars for VRAM, RAM, cores; draws server numbers |
| `CapacityOverview.jsx` | The machine's own CPU, RAM, and GPU capacity, scaled against the catalog |
| `ModelFitCell.jsx` | Badge, compact bars, and the shortfall in words |
| `Tabs.jsx` | Accessible tab strip |
| `Empty.jsx` | A consistent empty state |
| `LogViewer.jsx` | One log panel for a sandbox, a supervisor (filtered) and an appliance component: follow, filter, copy; stops following after a failed read |
| `PendingPanel.jsx` | A tab body that a later package fills: what belongs there, which endpoint, who owns it |
