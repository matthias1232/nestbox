# Nodes

A node is a machine that runs sandboxes and models. Phase 1 has exactly one,
`local` — the host the control plane runs on — and the data model is written for
more without a schema change.

## What a node is, and is not

- It **is** an execution host: its own runner, its own storage, its own hardware.
- It is **not** a tenant. `nodes` is a global table, because a machine is shared
  by every environment on it, the same way a gateway's host is. What runs on a
  node — sandboxes, model installs, hardware profiles, recipes, roots — carries
  its own `environment_id` and its own row-level policy.
- It is **not** a capacity figure. Capacity is a measurement of the node minus
  what its installs reserved. There is no `capacity` column to drift.

## The one number that matters

`GET /nodes` answers each node with a `budget`:

| Field | Meaning |
|---|---|
| `ram_total_bytes`, `vram_total_bytes` | From the latest `hardware_profiles` row for that node |
| `ram_reserved_bytes`, `vram_reserved_bytes` | Sum over the node's model installs |
| `ram_free_bytes`, `vram_free_bytes` | Total minus reserved, clamped at zero |
| `cpu_cores`, `gpu_count` | From the same measurement |
| `installs`, `sandboxes` | How many contributed, how many run here |

`hardware_known: false` means the node has never been measured. The budget is
then zeros and the console says "not measured yet" — a zero that reads as a
measurement would make an empty machine look full and a real one look empty.

## Where the numbers come from

`GET /v1/hardware` on the node's runner returns the host's raw
`/proc/cpuinfo`, `/proc/meminfo`, and `nvidia-smi` output, plus whether the Docker
daemon has an NVIDIA runtime. The control plane parses it (ADR 0018): the
parsers are tested Rust, and a second implementation in Python would be a second
opinion. The probe also marks the node `online` and records `container_gpu`.

A node whose runner does not answer is marked `offline` (unreachable) or
`unknown` (it answered with an error), no profile is written, and the previous
measurement — if any — is returned unchanged. Without one, the answer is
`503 node.runtime_unavailable`: there is nothing honest to report.

`container_gpu: false` makes every GPU model `no_gpu` on that node, whatever the
host's `nvidia-smi` shows. A container with no NVIDIA runtime cannot have the
GPU, and pretending otherwise produces a sandbox that starts and cannot use it.

## Endpoints

| Method | Path | What it does |
|---|---|---|
| GET | `/nodes` | Every node with budget and sandbox count (`{"items": [...]}`) |
| GET | `/nodes/{id}` | One node with its budget |
| GET | `/nodes/{id}/sandboxes` | This environment's sandboxes there |
| PUT | `/nodes/{id}/display` | Rename or describe — admin |
| POST | `/nodes/{id}/probe` | Measure now through the runner — operator |
| GET | `/models/hardware?node=` | The same profile through the model API |
| GET | `/sandboxes?node=` | The sandbox list filtered (404 on an unknown node) |

## What is deliberately absent

There is no create or delete endpoint. A node without a runner behind it would
be a row that can do nothing, and phase 1 has one adapter for one machine. When
a second machine arrives, it arrives as a row plus an adapter, and the shape of
`budget` does not change.
