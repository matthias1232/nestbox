#!/usr/bin/env bash
# Container start for a Hermes sandbox.
#
# Two things happen before the container's real command, and both are
# idempotent so a restart is free:
#
# 1. The egress proxy's CA is trusted, when the deployment mounted one. The CA is
#    generated per deployment (`deploy/scripts/make-egress-ca.sh`), so it cannot
#    be baked into the image. It is appended to the `certifi` bundle of the
#    interpreter Hermes runs on, because that is the bundle `httpx`/`mcp` use;
#    the system store is not writable as an unprivileged user. `curl` in a login
#    shell reads `CURL_CA_BUNDLE`, so the same bundle is exported there.
# 2. Nothing else. The MCP server entry lives in `~/.hermes/config.yaml` with
#    `${VAR}` references (baked at build), so no credential is written to disk
#    here and a missing token fails closed inside Hermes.
set -euo pipefail

ca_file="${NESTBOX_EGRESS_CA:-/secrets/egress-ca.crt}"

if [ -f "$ca_file" ]; then
    python_bin="$(ls -d /home/nestbox/.hermes/tools/python-*/bin/python3 2>/dev/null | head -1 || true)"
    if [ -n "$python_bin" ]; then
        bundle="$("$python_bin" -c 'import certifi; print(certifi.where())' 2>/dev/null || true)"
        if [ -n "$bundle" ] && [ -w "$bundle" ]; then
            if ! grep -q "Nestbox egress proxy CA" "$bundle" 2>/dev/null; then
                {
                    printf '\n# Nestbox egress proxy CA\n'
                    cat "$ca_file"
                } >> "$bundle"
                printf 'nestbox: trusted %s in %s\n' "$ca_file" "$bundle"
            fi
            # A login shell (and therefore `docker exec ... bash -lc`) picks this
            # up for curl and for anything else that honours the OpenSSL
            # variables, without touching the read-only system store.
            for profile in /home/nestbox/.profile /home/nestbox/.bashrc; do
                if [ -f "$profile" ] && ! grep -q "Nestbox egress proxy CA env" "$profile"; then
                    {
                        printf '\n# Nestbox egress proxy CA env\n'
                        printf 'export SSL_CERT_FILE="%s"\n' "$bundle"
                        printf 'export REQUESTS_CA_BUNDLE="%s"\n' "$bundle"
                        printf 'export CURL_CA_BUNDLE="%s"\n' "$bundle"
                    } >> "$profile"
                fi
            done
            export SSL_CERT_FILE="$bundle"
            export REQUESTS_CA_BUNDLE="$bundle"
            export CURL_CA_BUNDLE="$bundle"
        else
            printf 'nestbox: could not locate a writable certifi bundle; %s not trusted\n' "$ca_file" >&2
        fi
    fi
else
    printf 'nestbox: no egress CA at %s; HTTPS through a terminating proxy will fail verification\n' "$ca_file"
fi

exec "$@"
