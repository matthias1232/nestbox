#!/bin/sh
# Start OpenChamber in the foreground on the sandbox's web port.
#
# The UI is reachable from the LAN through the node's published port, so it must
# not be open: a password is required. One can be supplied in
# OPENCHAMBER_UI_PASSWORD; otherwise a random one is made and printed ONCE to the
# sandbox log (the Logs tab), which only an operator of this sandbox can read.
set -eu

if [ -z "${OPENCHAMBER_UI_PASSWORD:-}" ]; then
  OPENCHAMBER_UI_PASSWORD="$(head -c 18 /dev/urandom | base64 | tr -dc 'A-Za-z0-9' | head -c 20)"
  echo "nestbox-openchamber: generated UI password: ${OPENCHAMBER_UI_PASSWORD}"
fi
export OPENCHAMBER_UI_PASSWORD

echo "nestbox-openchamber: starting on port ${OPENCHAMBER_PORT:-3000}"
exec openchamber serve --port "${OPENCHAMBER_PORT:-3000}" --host 0.0.0.0 --foreground
