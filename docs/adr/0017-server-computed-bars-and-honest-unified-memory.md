# ADR 0017 — Resource bars are server-computed, and unified memory is recognised, never inferred

Status: Accepted
Date: 2026-10-27

## Context

ADR 0013 put the requirements in the database and the verdict in one function.
The console then grew a second job: show *why*, as "have versus need" bars for
VRAM, RAM, and CPU cores, and scale the machine's own capacity against the
catalog.

Two temptations appear at that point:

1. Let the browser compute the bar widths from raw requirement columns. It has
   the numbers; the arithmetic is trivial.
2. Infer a unified-memory pool from "no discrete GPU but a lot of RAM". Every
   CPU-only VM would then look like a DGX Spark, and GPU gates would pass on
   hardware that cannot run them.

Both make the display disagree with the gate in a way that is invisible until
someone installs.

## Decision

**The server returns the bars.** `domain::model::resource_bars` produces one
`ResourceBar` per resource (`vram | unified_memory | ram | cpu_cores | gpus`)
with `available`, `minimum`, `recommended`, and a status (`ok | tight | short |
unknown | not_required`). The console renders them; it does not divide.

**Unified memory is only reported on evidence.** A pool is recorded only when
`nvidia-smi` names a GB10 device or `/proc/cpuinfo` identifies the part as
GB10/Grace. The pool value is `MemTotal`, and `unified_memory_source` says so.
If the machine is recognised but `MemTotal` is unreadable, the pool stays
`NULL`, the note says "unknown", and the classifier does not award a GPU gate
it cannot justify.

**Filters switch verdicts, they do not recompute them.** `fits=true` keeps rows
whose verdict is in `Fit::fits()`; family, publisher, runtime, and CPU-only are
SQL predicates. Counting is the client's only arithmetic.

## Alternatives rejected

**Compute bar widths in SolidJS.** Rejected: two implementations of "does this
fit" is exactly the drift ADR 0013 removed. A bar that renders 40 % while the
gate says "short" is worse than no bar.

**Infer the unified pool from RAM size.** Rejected: a 128 GB x86 server is not a
GB10 and cannot give its RAM to a GPU kernel. The false positive would mark
80 GB models installable on a machine that has no GPU at all.

**Add a `unified_memory_bytes` guess plus a warning.** Rejected: a warning does
not stop the install button. A number in a gate has to be measured or absent.

**Send only raw requirements and let a shared JS module classify.** Rejected:
the API would stop being the authority, and a stale bundle would make a
different decision than the server.

## Consequences

- The bars are consistent with the verdict by construction, because the same
  call produces both.
- A resource that does not gate a model is `not_required`, not omitted, so the
  table has a stable shape.
- The catalog is now large enough (53 rows) that filters run in SQL, and the
  console applies its live filters to an already-loaded, already-classified
  list rather than issuing a request per keystroke.
- Adding a resource means adding a bar variant and a `Resource` value, both in
  `domain::model`, and the console picks it up without a second rule.

## What would invalidate this

- A driver interface that reports a unified pool explicitly (for example a
  `nvidia-smi` field for shared memory), which would make the GB10 name match
  unnecessary.
- Measured per-model memory from a runtime, which would let `available` and
  `minimum` be compared against a real allocation instead of a planning figure.
