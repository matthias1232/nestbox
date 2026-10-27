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

## Files

| File | Responsibility |
|---|---|
| `ClusterPage.jsx` | Gateways, sync, supervisors, topology |
