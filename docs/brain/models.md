# Models

## The idea

A model is a provider profile with an endpoint. That sentence carries most of
the design.

- A **local** model is an endpoint on this machine, usually an OpenAI-compatible
  server on the overlay.
- A **cloud** model is an endpoint somewhere else, with a credential.
- The **policy** is the same in both cases. What changes is one profile field.

That is why the console does not have a separate "local models" concept with a
separate policy path. It has a model list, and each entry says where it runs.

## The catalog

The catalog is data, not code (ADR 0013). Each row carries the requirements
that decide whether it can run here:

| Model | Parameters | Context | NVFP4 weights | Minimum VRAM |
|---|---|---|---|---|
| Nemotron Nano 12B v2 VL | 12.6B dense | 128K | ~14 GB | 24 GB |
| Nemotron 3 Nano | 30B-A3B | 1M | ~18 GB | 32 GB |
| Nemotron 3 Super | 120B-A12B | 1M | ~65 GB | 80 GB |
| Nemotron 3 Ultra | 550B-A55B | 1M | ~280 GB | 4 × 96 GB |

The numbers are estimates and are labelled as such, because the vendor does not
publish a VRAM table. The website's claims page holds the same standard: a
number without a source does not ship.

## The live check

On load, the console asks the control plane what this machine has. The answer
comes from the platform probe: GPU name, VRAM per card, card count, CUDA
version, and whether the machine has unified memory (a GB10-class box reports
memory differently from a discrete card).

The server then classifies each model:

`installable` · `already_installed` · `update_available` ·
`insufficient_vram` · `insufficient_gpus` · `no_gpu`

The classification and the shortfall both come from the server, so the badge and
the reason cannot disagree with the gate.

## Install, update, remove

Each is a job, because each has stages and can fail partway.

| Action | Stages |
|---|---|
| Install | download → verify checksum → unpack → start server → register provider → publish endpoint |
| Update | compare versions → install new → switch over → keep old until healthy |
| Uninstall | stop server → remove artifacts → revoke provider profile |

The result of an install is not just "done": it is the connection details,
because the point of installing a model is to use it.

## Cloud models

Cloud models appear only when a cloud provider is attached. They are marked as
provided by that provider and not installed locally, because the operator must
be able to tell at a glance what runs on their hardware and what does not.

## Context length is the hidden cost

A 1M-token window needs a KV cache measured in hundreds of gigabytes. On a
single card, the practical default is a capped context (32K–128K). The catalog
carries the context length so the console can warn instead of letting an
operator size the window and then hit an out-of-memory at request time.

## On a machine with no GPU

Nothing breaks. Detection reports no GPU, every local model is classified
`no_gpu` with the reason, and cloud models work normally. This is the intended
first experience on a machine that is being evaluated before hardware arrives.

## Where this lives in the code

| Concern | File |
|---|---|
| Catalog and hardware types | `services/control-api-rust/src/domain/model_catalog.rs` |
| Detection | `services/control-api-rust/src/services/hardware_detector.rs` |
| Fit classification | `services/control-api-rust/src/services/model_catalog_service.rs` |
| Install and update jobs | `services/control-api-rust/src/services/model_installer.rs` |
| Console | `services/web-console-solid/src/features/models/` |
