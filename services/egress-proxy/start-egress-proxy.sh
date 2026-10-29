#!/bin/sh
# Prepare the configuration, then become Envoy.
#
# Envoy asks the control API for every decision, and the control API only answers
# a node that proves who it is. The node id and token are not in the image (a
# token in an image is a token in a registry): they are written into a copy of
# the configuration here, from the node directory the compose file mounts. The
# copy lives in memory (`/run/envoy`) and is readable only by Envoy.
#
# This script starts as root, because it has to read a 0600 token file owned by
# the API's user, and drops to `envoy` before anything that parses traffic runs.
# Run as `envoy` already (for example `docker compose run --entrypoint envoy`
# to validate the image), it skips the preparation and uses the image's own
# configuration.

set -eu

CONF=/etc/envoy/envoy.yaml

if [ "$(id -u)" = "0" ]; then
    RUN=/run/envoy
    mkdir -p "$RUN"
    NODE_ID="${NESTBOX_NODE_ID:-}"
    TOKEN=""
    if [ -r "${NESTBOX_NODE_TOKEN_FILE:-/run/node/token}" ]; then
        TOKEN="$(tr -d '\r\n ' < "${NESTBOX_NODE_TOKEN_FILE:-/run/node/token}")"
    fi
    if [ -z "$NODE_ID" ] || [ -z "$TOKEN" ]; then
        echo "egress-proxy: no node id or token; the control API will refuse every check and every request is denied" >&2
    fi
    # `|` as the delimiter: neither value can contain it (a uuid and a hex token).
    sed -e "s|__NESTBOX_NODE_ID__|${NODE_ID}|g" -e "s|__NESTBOX_NODE_TOKEN__|${TOKEN}|g" \
        /etc/envoy/envoy.yaml > "$RUN/envoy.yaml"
    chown envoy:envoy "$RUN" "$RUN/envoy.yaml"
    chmod 0700 "$RUN"
    chmod 0600 "$RUN/envoy.yaml"
    exec su-exec envoy "$0" "$@"
fi

[ -f /run/envoy/envoy.yaml ] && CONF=/run/envoy/envoy.yaml

# The certificate broker is only useful for the TLS-reading mode; when its CA
# secret is absent it exits with a message and Envoy carries on, because the
# default HTTPS path (a decided tunnel) does not need it.
/usr/local/bin/egress-mitm &
broker=$!
sleep 1
if ! kill -0 "$broker" 2>/dev/null; then
    echo "egress-proxy: the certificate broker is not running; TLS is decided by destination, not read" >&2
fi

exec envoy -c "$CONF" --log-level "${ENVOY_LOG_LEVEL:-info}"
