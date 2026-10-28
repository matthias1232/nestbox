# cluster/ — gateways, supervisors, and the graph

## Purpose

Show where workloads run and whether the control plane and the gateway agree.

## What it must make obvious

- **Drift**: the gateway's desired settings version differs from the applied one.
  That is the state the sync button exists for, so it has to be visible without
  opening anything.
- **Placement safety**: a drifted or unhealthy gateway receives no new sandboxes.
- **The flow**: gateway → sandbox, so "which gateway carries this workload" is
  answerable by looking.
- **What the supervisor measured**: its best-practice checks are shown per row
  with the state and the measured value in the tooltip, and the two actions an
  operator repeats (check, restart the agent) are one click. The whitelist lives
  in the supervisor, so a button here cannot invent a third action.

## Files

| File | Responsibility |
|---|---|
| `ClusterPage.jsx` | Gateways, sync, supervisors, topology |
