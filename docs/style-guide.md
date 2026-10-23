# Style guide

## Language

English is the source language. There is no `locale/en/` catalog: msgids are the English strings.
Every user-visible string goes through `{% translate %}` or `gettext`, so a translation is data,
not a code change.

Never translate: `error.code` values, enum values, log lines, policy YAML keys, pack keys,
identifiers, docstrings.

## Code

- `ruff` and `mypy` are the authority; the CI runs both.
- Docstrings in Google style, in English.
- No silent `except:` — catch the narrowest type and either handle it or re-raise.
- Every mutating API action writes an `AuditEvent`.

## Units

- timestamps: ISO 8601, UTC in the API, localized in the UI
- sizes: integer bytes
- durations: integer milliseconds
- no localized numbers in machine-readable output

## Images

- decorative assets come from `render_image`; informational diagrams come from Mermaid sources
- no text inside generated images; labels come from the diagram layer
- no third-party logos or wordmarks
- prompts are committed in `assets/prompts/` so every asset is reproducible

## Commits

Conventional Commits, English, imperative: `feat(policies): hot-reload network rules`.
Sign off with `git commit -s` (DCO). No CLA.
