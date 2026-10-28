# ADR 0028 — An install serves one artefact from one pinned image

Status: Accepted
Date: 2026-10-28

## Context

ADR 0018 made an install a reservation: memory on a node, a provider profile in
state `pending`, a job that records the intent. That was honest about what it
did — nothing was fetched and nothing ran — but it left the product one step
short of the point. An install that never answers is a reservation for a model
nobody can use.

Phase 2 is that step: fetch the weights, start a server, and let the profile
stop saying "pending" when the server answers. The question is how much of that
the control plane should do itself.

## Decision

**The control plane decides what; the node decides how.** The control plane
reserves memory, picks the runtime, and owns the state machine. The runner on
the node owns the disk and the Docker daemon: it fetches the file, starts one
container, and stops it. Every network call happens outside a transaction.

**One artefact, by reference, in one format.** `models.artifact_ref` names a
single file as `huggingface:<repo>/<file>`. Not a snapshot: a GGUF file is a
complete model, while a safetensors snapshot is several files whose names the
platform would have to know and pins separately. A model without a reference is
refused with `422 model.not_servable` **before** memory is reserved, because a
reservation for something that cannot run is a lie with a number attached.

**One runtime, by row.** `model_server_images` holds the runtime name
(`llama_cpp`), the repository, the digest, the port, and the argument template.
The first row is `ghcr.io/ggml-org/llama.cpp@sha256:8ee4ec7c…` on port 8080. A
tag is refused by a check constraint: an image is a dependency with a version,
and `latest` is not a version.

**The states say what is happening.** `installing` means reserved and committed;
`downloading` means the node is fetching the file; `starting` means the container
is up but has not answered; `installed` means it answered. The probe is the
install's own provider profile — the thing sandboxes will use — not a health
check of the container, because a container that runs but cannot answer is the
failure worth catching.

**Two guards the node keeps for itself.** The image must be on
`NESTBOX_MODEL_IMAGES` (exact digest reference), and every absolute path in the
server command must be the model file the runner mounted. The model container
joins the internal runtime network only, drops all capabilities, and gets the
reservation as its memory limit. So a compromised control plane still cannot
start an arbitrary image, mount `/etc`, or reach the internet from the model
container.

**A failure leaves nothing running.** A download that fails, a server that
never answers, and an install interrupted by a restart all stop the container
and mark the install `failed` with the reason. The boot settles
`downloading`/`starting` rows exactly as it settles interrupted builds. On
uninstall the container is stopped *before* the row is deleted: if the node
cannot stop it, the uninstall is refused rather than orphaning memory. The
artefact stays on disk, so a later install of the same model does not download
it again.

**Uninstall is the only path that frees memory**, and it now also means "stop
the server". A model whose provider profile a sandbox selects still refuses with
`409 provider.in_use` and names the sandbox.

## Alternatives rejected

**Download and serve inside the control API.** It would put a multi-gigabyte
write and a Docker socket in the tenant-facing process, and the control plane
container deliberately sees no node devices.

**A tag instead of a digest**, or an image name supplied by the request. Both
let a registry or a caller change what runs on the node without a review.

**A health check on the container instead of a probe of the profile.** Docker
says "running" before llama.cpp has mapped the weights, and the profile is the
endpoint the sandboxes use; measuring anything else would report a state nobody
experiences.

**Trusting `2Gi`-style memory limits in the runner.** The API normalises them to
bytes; the runner takes bytes and passes them through.

## Consequences

- Only GGUF on llama.cpp is served in this phase. vLLM, NIM, and GPU servers are
  a second runtime row and a second image, not a rewrite.
- No multi-node serving: one install is one container on one node.
- The port is written in two places (the registry row and the provider
  endpoint); the install probe is what proves they agree.
- The model container's memory limit is the reservation, so a model whose real
  peak exceeds its catalog figure is killed by the kernel rather than starving
  its neighbours. The catalog figure is the promise; a wrong figure shows up as
  a failed or flapping install, not as a silent overcommit.

## What would invalidate this

A model format that is not one file (or a manifest the runner can validate),
a runtime that needs more than an argv and a memory limit, or a scheduler that
places one model across several nodes.
