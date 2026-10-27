# ADR 0020 — Baselines, agent packs, and tools are data

Status: Accepted
Date: 2026-10-27

## Context

Agents arrived as `match` arms. `sandbox_service::pack_defaults` named four
packs — hermes, openclaw, git, ubuntu-dev — and returned an image, a command,
and a set of environment variable names. The console had no way to list them,
the API had no endpoint that answered "what can I put in a sandbox?", and a new
agent meant a release.

The catalog decision (ADR 0013) already established the rule for models: what
exists is data, what it needs is data, and code decides only the verdict. Packs
are the same kind of thing, and so are the base operating systems they run on
and the tools they install.

## Decision

**Three tables, three endpoints.**

- `baselines` — a base operating system: image, package manager (`apt` or
  `dnf`), default tools, and whether NVIDIA OpenShell supports it. Seeded with
  Ubuntu 24.04 (recommended), Debian 12, Rocky Linux 9.
- `agent_packs` — what goes into a sandbox and how: install kind (`script`,
  `git`, `npm`, `pip`, `binary`, `image`, `none`), install reference, the paths
  under the home that must persist, whether it needs a GPU or an inference
  endpoint, the image and command it runs with, its ports, and the environment
  variable slots it expects. Seeded with the four existing packs plus
  claude-code, opencode, openchamber, nemoclaw, and antigravity.
- `tool_catalog` — tools with their name per package manager, so a recipe can
  resolve `vim` to `vim` on apt and `vim-enhanced` on dnf.

**`pack_defaults` stops being a `match`.** It reads the pack by key. An unknown
pack is `404 pack.not_found`, not a silent fallback to the default image; a pack
that does not support the chosen baseline is `409 pack.incompatible` with the
reason. Both are refusals a builder can act on.

**The compatibility rule is a field, not a convention.** `base_os` is a list of
baseline keys; empty means every baseline. The builder and the sandbox service
both ask `AgentPack::supports(baseline_key)`, so the console and the API cannot
disagree about what can be built.

**Environment variable slots are names, never values.** A pack declares
`OPENAI_API_KEY`; the value comes from the provider profile and the secret
locator at use (ADR 0005, 0016). This is what keeps a pack row safe to read.

## Alternatives rejected

**YAML files per pack in the repository.** Rejected: a file is a deployment
artefact, so adding an agent would still mean a release, and the console would
need a second parser. YAML also invites values where only names belong.

**Keep the `match` and add a `GET /agent-packs` that returns a constant.** The
constant would be a third copy of the same list, and the first divergence would
be invisible.

**Let a pack declare its requirements as free text.** Rejected: the builder has
to decide, and free text cannot be decided. `requires_gpu` and
`needs_inference` are booleans for the same reason the model gates are numbers.

## Consequences

- The nine packs are seeded by migration; changing one is a migration or an
  admin endpoint, not a code change. In this phase there is no write endpoint:
  a pack is catalog data, curated with the release.
- `tool_profiles` gains `package_manager`, so a tool profile records which
  manager resolved its package list.
- The builder validates a recipe against the tables, so an incompatible pack is
  refused before a job is queued.
- `GET /agent-packs?baseline=` and `GET /tool-catalog?baseline=` make the
  builder's choices derivable, so the console does not hard-code the matrix.

## What would invalidate this

A pack format that needs arbitrary build steps — several layers, generated
files, per-architecture branches — would outgrow the column set. At that point
a pack becomes a recipe template, and this ADR is superseded rather than
extended with a `jsonb` catch-all.
