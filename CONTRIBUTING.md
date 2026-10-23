# Contributing

Thanks for wanting to help. This project is small on purpose: a control plane over NVIDIA
OpenShell plus a reproducible appliance build.

## Ground rules

- **English only** in code, comments, docs, commits and issues.
- **Apache-2.0 with DCO.** Sign every commit: `git commit -s`. There is no CLA.
- **No ASCII diagrams.** Drawings are rendered (Mermaid → SVG) or generated as images. CI enforces it.
- **No third-party marks in names or artwork.** Read `NOTICE` first.
- **Claims need sources.** If you change a number in the docs, add or update a row in `docs/claims.md`.

## Good first issues

- A German translation catalog (`controlplane/locale/de/`) — the mechanism is ready, only strings are missing.
- An agent pack in `packs/` for a tool you actually use.
- A Nemotron VRAM measurement on your hardware, with the command and the checkpoint hash.

## Development

```bash
uv venv && uv pip install -e ".[dev]"
cd controlplane
python manage.py migrate
NESTBOX_BOOTSTRAP_ADMIN_EMAIL=you@example.com NESTBOX_BOOTSTRAP_ADMIN_PASSWORD=pw \
  python manage.py bootstrap
python manage.py load_packs
python manage.py runserver
```

Tests: `pytest -q`. Lint: `ruff check controlplane scripts` plus
`python scripts/lint_no_ascii_diagrams.py`.

## Pull requests

One concern per PR. Update `contracts/openapi-v1.json` only when the API change is intentional —
CI diffs it. Add a migration for every model change. New user-visible strings must be translatable.

## Security

Do not open a public issue for a vulnerability. See `SECURITY.md`.
