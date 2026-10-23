#!/bin/sh
set -eu

python manage.py migrate --noinput
python manage.py bootstrap
python manage.py module_state --init
python manage.py load_packs || true
python manage.py seed_nests || true
python manage.py compilemessages --ignore .venv || true

# The appliance terminates TLS in gunicorn: first boot writes a self-signed pair (or the
# operator later replaces it with an ACME certificate) into /etc/nestbox/tls.
set -- --bind 0.0.0.0:8000 \
  --workers "${NESTBOX_WORKERS:-3}" \
  --timeout 120 \
  --access-logfile - \
  --error-logfile -

if [ -s /etc/nestbox/tls/cert.pem ] && [ -s /etc/nestbox/tls/key.pem ]; then
  echo "[nestbox] serving https with /etc/nestbox/tls/cert.pem"
  set -- "$@" --certfile /etc/nestbox/tls/cert.pem --keyfile /etc/nestbox/tls/key.pem
else
  echo "[nestbox] no certificate found, serving plain http"
fi

exec gunicorn config.wsgi:application "$@"
