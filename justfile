set shell := ["bash", "-c"]

venv:
    uv venv --python 3.12
    uv pip install -e ".[dev]"

migrate:
    cd controlplane && ../.venv/bin/python manage.py migrate

bootstrap:
    cd controlplane && ../.venv/bin/python manage.py bootstrap

packs:
    cd controlplane && ../.venv/bin/python manage.py load_packs

serve:
    cd controlplane && ../.venv/bin/python manage.py runserver 0.0.0.0:8000

test:
    .venv/bin/python -m pytest -q

lint:
    .venv/bin/ruff check controlplane scripts
    .venv/bin/python scripts/lint_no_ascii_diagrams.py

api-schema:
    cd controlplane && ../.venv/bin/python manage.py spectacular \
        --file ../contracts/openapi-v1.json --fail-on-warn --validate

api-check: api-schema
    git diff --exit-code contracts/openapi-v1.json

translate:
    cd controlplane && ../.venv/bin/python manage.py makemessages -a --no-obsolete --ignore .venv
    ../.venv/bin/python scripts/i18n/audit.py

translate-compile:
    cd controlplane && ../.venv/bin/python manage.py compilemessages --ignore .venv

assets-check:
    python3 scripts/lint_no_ascii_diagrams.py

clean:
    rm -rf controlplane/staticfiles .pytest_cache .ruff_cache
