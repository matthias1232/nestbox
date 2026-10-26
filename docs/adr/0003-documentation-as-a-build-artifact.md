# ADR 0003 — Documentation as a build artifact

Status: Accepted
Date: 2026-10-26

## Context

Documentation written at the end of a project describes what the author
remembered, not what the code does. The prototype had a solid README and no
module documentation at all, so a reader had to reconstruct intent from code.

The maintainer's requirement is explicit and repeated: documentation for AI
agents must exist **everywhere**, must be produced **during** implementation,
and must explain not only what was built but **why**, so future work can
extend the system without re-deriving the reasoning.

## Decision

Documentation is a build artifact with four layers, and missing documentation
fails the build.

| Layer | Artifact | Reader | Question |
|---|---|---|---|
| Orientation | root `AGENTS.md` | anyone arriving | Where do I start |
| Module contract | `AGENTS.md` per folder | agent working there | What does this own, what is forbidden |
| Decision record | `docs/adr/NNNN-*.md` | anyone extending | Why this way, what was rejected |
| Machine-readable | `graphify-out/`, `docs/brain/` | agent querying | How do these relate |

Enforced by `tests/architecture.rs` and `tests/docs_consistency.rs`:

- Every folder under `src/` has an `AGENTS.md`.
- Every `pub fn` has a doc comment.
- Every ADR references paths that exist.
- The documented endpoint list matches the OpenAPI document.

## Alternatives rejected

**Documentation in a wiki.** Rejected: it drifts from the code silently and is
not in the same review as the change. Documentation in the repository fails
review when it is wrong.

**Generate all documentation from code.** Rejected as the only mechanism:
generated docs describe signatures, not intent. The reason a policy scope is
binary, or why a rule needs approval, cannot be derived from a type signature.
Generation is used only for the API reference, which *is* derivable.

**A single large design document.** Rejected: it becomes stale as one unit.
Per-folder and per-decision documents go stale one at a time, which is visible
and fixable.

**Documentation as a separate phase at the end.** Rejected: this is the
failure mode being corrected. The rule is that the `AGENTS.md` exists before
the first code in the folder, so it describes intent rather than a
post-hoc rationalisation.

## Consequences

- Every new folder costs an `AGENTS.md` before it costs code. Slower start,
  faster every subsequent change.
- The CI check makes a missing document a build failure, so the discipline is
  mechanical rather than dependent on goodwill.
- ADRs are immutable, so superseded decisions remain visible. The history of
  the reasoning is preserved, which is the point.
- The documentation quality itself cannot be fully automated. A present but
  shallow document passes the check. The mitigation is review and the
  `Extending this module` section, which forces the author to write something
  actionable rather than a restatement of the filename.

## What would invalidate this

- Nothing foreseeable. If the checks become noise, they would be refined, not
  removed; the requirement for agent-readable documentation is a product
  requirement, not an engineering preference.
