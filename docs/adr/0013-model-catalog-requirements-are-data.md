# ADR 0013 — Model requirements are data

Status: Accepted
Date: 2026-10-26

## Context

The console must answer "which models can this machine run" before the user
clicks install. The requirements are published in the project's own
documentation: Nemotron Nano 12B v2 VL needs roughly 14 GB of weights and fits
a 24 GB card; Nemotron 3 Nano needs about 18 GB; Nemotron 3 Super about 65–70 GB;
Nemotron 3 Ultra about 280–300 GB across multiple cards.

If those numbers are written twice — once in a Rust constant that gates the
install button, once in a TypeScript constant that colours the badge — they
drift. The user then sees "installable" on a model the server refuses, or the
reverse.

## Decision

The requirements live in the database, and both the gate and the display read
the same row.

```
models(id, name, family, parameters, context_length, quantization,
       weights_bytes, min_vram_bytes, min_gpu_count, source_uri, version, ...)
```

The server computes the fit and returns one of a closed set:

`installable`, `already_installed`, `update_available`, `insufficient_vram`,
`insufficient_gpus`, `no_gpu`.

The client renders the classification. It does not compute it. The shortfall
(`required 65536 MiB, available 24576 MiB`) also comes from the server, so the
explanation matches the decision.

## Alternatives rejected

**Hardcode the requirements in the frontend.** Rejected: the frontend cannot be
the authority for whether an install will succeed, and the numbers would exist
twice.

**Hardcode them in the backend as a Rust `static`.** Rejected as the primary
store: adding a model would require a deployment, and the catalog is expected to
grow. A constant table would also diverge from the seeded rows.

**Ask the user to judge.** Rejected: the requirement is a live check that tells
the operator quickly what is possible. Making them compare numbers by hand is
the problem being solved.

**Detect only, do not classify.** Rejected: "24 GB detected" is not an answer.
The answer is "Super does not fit, Nano does, and here is the gap".

## Consequences

- A model catalog update is a data migration, not a code release.
- The fit rule is one function with unit tests, including the awkward cases:
  unified memory on a GB10-class machine, multiple cards, and a model that fits
  only with a capped context length.
- The catalog must carry a `source_uri` and a version, so the numbers are
  attributable rather than folklore. The website's claims page sets the same
  standard for marketing copy.
- Hardware detection must be cached but refreshable, because VRAM changes when
  another workload occupies the GPU.

## What would invalidate this

- A runtime that reports required memory for a model itself, which would make
  the catalog numbers redundant for that provider.
