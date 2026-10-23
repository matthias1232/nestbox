#!/bin/sh
set -eu

python manage.py migrate --noinput
python manage.py bootstrap
python manage.py load_packs || true
python manage.py compilemessages --ignore .venv || true

exec gunicorn config.wsgi:application \
  --bind 0.0.0.0:8000 \
  --workers "${NESTBOX_WORKERS:-3}" \
  --timeout 120 \
  --access-logfile - \
  --error-logfile -
