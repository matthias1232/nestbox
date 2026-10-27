# ADR 0018 — A model is an install on a node, and the fit is measured against what is free

Status: Accepted
Date: 2026-10-27

## Context

ADR 0013 put model requirements in the catalog and the verdict in one function.
The catalog row then also carried `state`, `installed_version`, and
`installed_at` — which made a catalog entry a thing that is "installed", as if
the platform were one machine.

Three problems followed from that shape, and all three appeared on the first
real appliance:

1. **Two nodes cannot hold the same model.** The row is the model; installing it
   on a second machine would overwrite the first install's state.
2. **The fit is computed against total capacity.** A machine with 32 GiB of RAM
   says yes to a 24 GiB model that is already holding 20 GiB. The verdict is
   true about the hardware and false about the node.
3. **The hardware is measured in the wrong process.** The control API probes
   itself. In a container it sees the CPU and memory it was given, and only the
   GPU that was passed through. On VM200 that happens to match the host; on any
   remote node it describes the wrong machine.

## Decision

**Install state lives on `model_installs(environment_id, model_key, node_id)`.**
A catalog row describes a model; an install row is one copy of it on one
machine. The same model may be installed on several nodes, and each install
carries its own state, version, error, job, and provider profile.

**An install reserves memory for as long as it exists.** At install time the
service computes `reserved_ram_bytes` and `reserved_vram_bytes` from the
catalog's recommended figure, falling back to the minimum, and writes them on
the row. A failed install releases its reservation; an uninstalling one keeps it
until the job finishes, because the server may still be running.

**The fit is computed against the node's free budget, not its total.** Free is
the latest `hardware_profiles` row for the node minus the reservations of its
installs, clamped at zero. `NodeBudget` is the only type that does this
arithmetic; `classify` takes the free numbers. A model that would fit on an
empty node but not on this one is `insufficient_ram` or `insufficient_vram`, and
the reason names the installs that hold the memory.

**The hardware is probed on the node, through its runner.** `GET /v1/hardware`
returns the runner host's raw `/proc/cpuinfo`, `/proc/meminfo`, and `nvidia-smi`
text plus whether the Docker daemon has an NVIDIA runtime. The parsers stay in
Rust (`parse_cpuinfo`, `parse_meminfo_total`, `parse_nvidia_smi`,
`detect_unified_pool`), because a second implementation in Python would be a
second opinion. The probe is a measurement request, and it also marks the node
`online` and records `container_gpu`.

**Nodes stay global rows, their children are tenant-scoped.** A machine is
shared by every environment on it, the way a gateway's host already is. What
runs on it — sandboxes, installs, hardware profiles, recipes, roots — carries
its own `environment_id` and its own row-level policy.

**Installing does not start anything.** The install reserves memory, creates a
provider profile in state `pending` (ADR 0016), and enqueues a job. Downloading
weights and starting a model server is phase 2, and the API says so.

## Alternatives rejected

**Keep `state` on the catalog row and add a `node_id` column.** Rejected: that
is a one-to-many relation expressed as a column, and the second install
overwrites the first. The bug would surface as "the model is installed but the
other node cannot use it".

**Compute the fit from the total and let the UI warn.** Rejected: a verdict that
says "fits" and then fails at install is worse than a verdict that says why it
does not fit. The console has to explain a refusal, not apologise for one.

**Probe in the control API and pass the node's `nvidia-smi` through a mount.**
Rejected: the container sees only what was mounted into it, and the mount would
have to be recomputed per node. The runner already has the host's view.

**A second `nodes` table for execution hosts.** Rejected: the concept already
existed for gateways. One notion of "node", extended, beats two that drift.

## Consequences

- `models` no longer answers "is this installed"; `model_installs` does. The API
  keeps the catalog endpoint and gains `GET /models/installs`.
- A node without a hardware profile reports zeros and `unknown`, never a guess.
  The console must render that state rather than treating it as "no capacity".
- Reservation without a running server is visible as such: the profile is
  `pending` and the console says the server is not started yet.
- `latest_hardware` takes a `node_id`; a probe writes a row per measurement, so
  history stays available for a capacity trend.
- `container_gpu=false` makes every GPU model `no_gpu` on that node, whatever
  the host sees. This is the rule that keeps a VM from pretending to have a GPU.

## What would invalidate this

A scheduler that places a model across nodes (splitting one model over two
machines) would need a reservation per fragment, not per install. If that
happens, this ADR is superseded rather than amended: the install row would
become a placement.
