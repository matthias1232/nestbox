#!/bin/sh
# Start the certificate broker, then become Envoy.
#
# The broker is the only process that needs the node CA key. It reads it from
# the Docker secret at /run/secrets/egress_ca.key (see AGENTS.md); when the
# secret is absent it exits with a message and Envoy keeps running without TLS
# termination, which means CONNECT fails closed instead of opening an
# uninspected tunnel. That is the failure mode we want: a proxy that cannot see
# does not pretend it can.

set -eu

/usr/local/bin/egress-mitm &
broker=$!

# If the broker dies, Envoy keeps serving: plain HTTP and the decision path are
# unaffected, only CONNECT termination is gone. Say so once instead of leaving a
# silent half-working proxy behind. Nothing supervises the broker: a crash loop
# would be louder in the image than in a log line nobody reads, and the failure
# is fail-closed either way.
sleep 1
if ! kill -0 "$broker" 2>/dev/null; then
    echo "egress-proxy: the certificate broker is not running; HTTPS fails closed" >&2
fi

exec envoy -c /etc/envoy/envoy.yaml --log-level "${ENVOY_LOG_LEVEL:-info}"
