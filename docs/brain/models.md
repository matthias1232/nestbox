# Models

## The idea

A model row is a requirement profile, not a promise. The catalog says what a
model needs; the probe says what this machine has; one pure function in
`domain::model` turns the two into a verdict and the numbers behind it.

That split is why the badge in the console and the install gate in the API cannot
disagree: they are the same call.

## The catalog

The catalog is data, not code (ADR 0013). Migration `0013_model_catalog.sql`
seeds it; each row carries:

| Column | Meaning |
|---|---|
| `min_vram_bytes` | The GPU gate: what one card, or the unified pool, must have |
| `recommended_vram_bytes` | The comfortable figure — "starts" versus "usable" |
| `min_ram_bytes` | The CPU gate; for `cpu` rows this is the whole gate |
| `recommended_ram_bytes` | Comfortable system RAM |
| `min_cpu_cores` / `recommended_cpu_cores` | Core gate for the CPU path |
| `runtime` | `cuda`, `cpu`, or `hybrid` (GPU preferred, RAM fallback) |
| `arch`, `artifact_format` | `tensorrt`/`nvfp4`, `gguf`, `safetensors`/`awq`/`fp8` |
| `publisher`, `license` | Who made it and under what terms |
| `unified_memory_only` | Weights exceed any single card; only a shared pool fits |
| `is_cpu_only` | Runs with no GPU at all |

Families in the catalog: NVIDIA/Nemotron (Nano 9B/12B, Nemotron 3 Nano/Super/
Ultra, Nemotron 4 340B, Llama-Nemotron 70B/8B), DeepSeek (R1 distills 1.5B–32B,
R1 and V3.1 671B, Coder V2 Lite), Qwen (Qwen3 0.6B–32B, 235B-A22B, Coder 30B-A3B,
Qwen2.5 Coder 1.5B/7B, Qwen2.5 3B), Llama (3.2 1B/3B, 3.1 8B/70B, 3.3 70B,
Llama 4 Scout/Maverick), Gemma (2 2B/9B/27B, 3 1B/4B/12B/27B, 3n E4B), Phi
(3 Mini, 3.5 Mini, 3 Medium, 4 14B, 4 Mini), plus Mistral 7B/Nemo 12B/Mixtral
8x7B and the smallest usable models (TinyLlama 1.1B, SmolLM2 360M/1.7B).

### How the numbers were derived

The migration states the rule once, and each row is checked against it:

- `weights = parameters × bytes per parameter` at the stated format (FP16 2.0,
  FP8 1.0, Q4_K_M 0.60, NVFP4/AWQ 0.55). MoE figures use the **total** parameter
  count, because every expert must be resident.
- `min_vram = weights + KV cache and runtime overhead` (roughly ×1.2 below
  10 GiB, weights + 6…16 GiB above), rounded up.
- `recommended_vram = min × 1.25…1.5`.
- `min_ram`, for CPU/GGUF rows, is the file plus ~2 GiB of working memory,
  because llama.cpp mmaps the weights.

These are planning figures, not measurements, and the doc says so. A vendor
minimum, where published, wins over the formula.

## The live check

On load the console asks for the catalog, and the server classifies every row
against the most recent probe (`?refresh_hardware=true` forces a new one).

The probe reads the kernel's own files rather than tools that may be absent:

| Fact | Source |
|---|---|
| CPU model, cores, threads | `/proc/cpuinfo` (`physical id`/`core id` pairs; the logical count on ARM) |
| System RAM | `/proc/meminfo` `MemTotal` |
| GPU name, memory, free | `nvidia-smi` |
| CUDA version | `nvidia-smi` header |
| Unified pool (DGX Spark / GB10) | Only when `nvidia-smi` names a GB10 device or the CPU identifies as GB10/Grace; then `MemTotal` is the pool, and `unified_memory_source` records where the number came from |

The unified pool is **never inferred from "no discrete card but a lot of RAM"**.
That inference would make every CPU-only VM look like a Spark and let GPU gates
pass on hardware that cannot run them. If a GB10 is recognised but `MemTotal` is
unreadable, the pool stays unknown and the note says so.

## The verdict

Order matters, and the order is the rule:

1. remote → `remote`
2. installed → `already_installed` / `update_available`
3. unified-only without a unified pool → `needs_unified_memory`
4. `cpu` → RAM and cores → `cpu_installable` / `insufficient_ram` / `insufficient_cpu`
5. `hybrid` → the GPU gate if there is a GPU, else the CPU gate
6. `cuda` → `no_gpu` / `insufficient_gpus` / `insufficient_vram` / `installable`

`Fit::fits()` is the console's "fits this machine" filter: `installable`,
`cpu_installable`, `already_installed`, `update_available`. `Fit::allows_install()`
is the button: everything except `already_installed`.

## Have versus need

Each verdict carries `bars`: one per resource, computed server-side
(`domain::model::resource_bars`) so the client draws the same numbers the gate
used.

- `available` is what the machine has, `minimum` the gate, `recommended` the
  comfortable figure.
- `status` is `ok` (at or above recommended), `tight` (between minimum and
  recommended), `short` (below minimum), `unknown` (machine figure missing), or
  `not_required` (this resource does not gate this model).
- On a unified machine the unified bar replaces the VRAM bar, because showing
  both would double-count one pool.

The console renders them as two tracks: "have" above, "need" below, with the
minimum solid and the recommendation lighter.

## Install, update, remove

Each is a job, because each has stages and can fail partway.

| Action | Stages (Phase 2 for the artefact download) |
|---|---|
| Install | download → verify checksum → unpack → start server → register provider → publish endpoint |
| Update | compare versions → install new → switch over → keep old until healthy |
| Uninstall | stop server → remove artifacts → revoke provider profile |

**The download is not implemented yet.** An install job records the intent and
state transition; no weights are fetched. The Models page says this in plain
words rather than showing a progress bar that counts nothing. This is the one
honest gap in the feature, and it is listed in `docs/architecture/phase1-review.md`.

## Cloud models

Cloud models appear only when a cloud provider is attached. They are marked as
provided by that provider and not installed locally, because the operator must
be able to tell at a glance what runs on their hardware and what does not. They
never appear in the "fits this machine" filter.

## Context length is the hidden cost

A 1M-token window needs a KV cache measured in hundreds of gigabytes. On a
single card, the practical default is a capped context (32K–128K). The catalog
carries the context length so the console can warn instead of letting an
operator size the window and then hit an out-of-memory at request time.

## On a machine with no GPU

Nothing breaks, and it is not a dead end any more. Detection reports CPU model,
cores, and RAM with the reason no GPU was found. GPU models are `no_gpu` with
that reason; CPU and hybrid models are classified against RAM and cores, so a
CPU-only appliance still has installable models. This is the intended first
experience on a machine being evaluated before hardware arrives, and it is the
state of the VM200 appliance, which is why the browser check asserts that
`cpu_installable` appears there.

## Where this lives in the code

| Concern | File |
|---|---|
| Catalog, hardware, and fit types; the classifier; the bars | `services/control-api-rust/src/domain/model.rs` |
| Detection, catalog assembly, facets, install jobs | `services/control-api-rust/src/services/model_service.rs` |
| Catalog queries and hardware rows | `services/control-api-rust/src/store/postgres/models.rs` |
| Endpoints | `services/control-api-rust/src/api/models.rs` |
| Catalog data | `services/control-api-rust/migrations/0012_model_requirements.sql`, `0013_model_catalog.sql` |
| Console | `services/web-console-solid/src/features/models/` |
| Bars and capacity | `services/web-console-solid/src/components/ResourceBars.jsx`, `CapacityOverview.jsx` |
