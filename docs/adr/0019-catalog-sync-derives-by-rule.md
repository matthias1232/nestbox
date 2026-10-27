# ADR 0019 — The catalog is derived from the repositories by rule, and a curated row wins

Status: Accepted
Date: 2026-10-27

## Context

The catalog started as 53 hand-written rows and grew to 85. Hand-writing is
honest — every number has a source — but it does not scale: the open-weight
landscape changes weekly, and an operator asking "what exists?" should not wait
for a release.

The obvious answer, "fetch it from Hugging Face", has two failure modes that are
worse than a stale catalog:

1. **A guessed number becomes a gate.** If the sync infers VRAM from a parameter
   count and a fudge factor, the catalog will confidently claim a model fits a
   card it does not fit. ADR 0013 exists because that guess used to be code.
2. **A sync overwrites a curated row.** The hand-checked DeepSeek V4 entry, with
   its published INT8 weight size, becomes whatever the API returned that night.

A third failure mode is scale: two million repositories, plus NGC organisations
that exist only for tests.

## Decision

**Requirements are derived by the rule in ADR 0013, and only from published
artefacts.** The sync reads `safetensors.total` for the parameter count, the sum
of the weight blobs for `weights_bytes`, `config.json` for context length and
expert counts, and `cardData.license` for the licence. From those it computes
the same gates a human would: minimum VRAM is weights plus runtime and KV
overhead (about 10 % above 100 GiB, 6–16 GiB below), recommended is 1.25× the
minimum rounded up, and a model larger than one card is `unified_memory_only`
with `min_gpu_count = ceil(min_vram / 80)` as the discrete alternative.

**A missing number means no gate, not an estimate.** When a repository does not
publish a parameter count, the row is written with null requirements and the
classifier reports `unknown`. `unknown` never reads as `installable`.

**A curated row wins.** Every seeded row has `pinned = true`, and the upsert
skips pinned rows entirely — not their numbers, not their name. New rows are
`pinned = false`. The sync may add siblings; it never edits a hand-checked one.

**The sync is bounded and per-source.** Hugging Face is filtered to
`text-generation`, sorted by downloads, limited to a top-N, and restricted to a
publisher allowlist (deepseek-ai, Qwen, meta-llama, google, microsoft,
mistralai, nvidia, moonshotai, zai-org, MiniMaxAI, openai, unsloth, bartowski).
Ollama comes from its registry manifests and config blobs. NGC search is
restricted to `orgName=nvidia`, because the unfiltered search returns test
organisations. build.nvidia.com contributes NIM availability as a boolean — it
publishes no sizes, so it never sets one.

**Each source fails alone.** Every HTTP call has a timeout, each source's error
is written to the job log, and the other sources still finish. A run that added
nothing is a successful run with zero additions, not a failed job.

**Rows are never deleted.** A model that disappears upstream keeps its row and
its last `synced_at`. Deleting a catalog row would orphan installs and break
references in recipes.

## Alternatives rejected

**Estimate VRAM from parameters and a bits-per-weight assumption.** Rejected:
this is the exact shortcut ADR 0013 removed. A gate computed from a guess is a
promise the platform cannot keep.

**Let the sync take precedence and rebuild the catalog each night.** Rejected:
the curated rows carry information the API does not publish (measured
footprints, the honest "this needs twenty cards"), and a nightly rebuild would
throw it away.

**Fetch everything from Hugging Face and filter in the console.** Rejected: the
catalog would be unusable at two million rows, and the facets would count noise.

**A separate sync service.** Rejected in this phase: one job in the existing job
runner, on the existing daily schedule, is the smaller thing that works.

## Consequences

- A synced row can be `unknown` for a long time; the console shows that instead
  of hiding the model.
- The allowlist is data in code and needs a review when a new publisher appears.
  That is deliberate: an open filter is how a catalog fills with forks.
- `source`, `source_id`, `synced_at`, `pinned`, and `nim_available` are part of
  the model type, so the console can show provenance per row.
- The nightly job writes counts per source into the job log; that log is the
  evidence that the sync did something.

## What would invalidate this

A published, machine-readable VRAM requirement per model would replace the
derivation rule with a lookup. Until then the rule stays, because it is the only
version whose arithmetic can be read and tested.
