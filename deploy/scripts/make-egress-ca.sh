#!/bin/sh
# Create the CA that the egress proxy uses to sign the certificates it presents
# to sandboxes, and that every sandbox image trusts.
#
# Two files come out of here:
#
#   deploy/secrets/egress/ca.key   0600, on the node, never in a sandbox,
#                                  never in the database (ADR 0005)
#   deploy/secrets/egress/ca.crt   the public half; copied into every sandbox
#                                  image as a trusted root at build time
#
# Why a per-node CA rather than a shared one: a stolen key compromises exactly
# one node's inspection, and rotating it is a local operation. Why a CA at all:
# without it a TLS request is a CONNECT tunnel — host, port, SNI, and byte counts
# and nothing else — and the console has to say so.
#
# The CA is EC P-256: smaller, faster, and as boring as RSA for this purpose.
# Valid ten years, because it signs short-lived leaf certificates and rebuilding
# every sandbox image because the root expired is a worse failure than a long
# root lifetime.
#
# Usage:
#   ./make-egress-ca.sh            create if absent
#   ./make-egress-ca.sh --force    replace an existing CA (all leaf certificates
#                                  signed by the old one stop being trusted)
#
# The script is idempotent and never prints the key.

set -eu

here=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
out_dir="${NESTBOX_EGRESS_CA_DIR:-$here/../secrets/egress}"
key="$out_dir/ca.key"
crt="$out_dir/ca.crt"
force=0
for argument in "$@"; do
    case "$argument" in
        --force) force=1 ;;
        *) echo "unknown argument: $argument" >&2; exit 2 ;;
    esac
done

if [ -f "$key" ] && [ "$force" -ne 1 ]; then
    echo "a CA already exists at $crt"
    if command -v openssl >/dev/null 2>&1; then
        openssl x509 -in "$crt" -noout -subject -enddate -fingerprint -sha256 2>/dev/null || true
    fi
    echo "pass --force to replace it; every sandbox image then needs a rebuild"
    exit 0
fi

if ! command -v openssl >/dev/null 2>&1; then
    echo "openssl is required" >&2
    exit 1
fi

umask 077
mkdir -p "$out_dir"
node_name=$(hostname 2>/dev/null || echo node)
subject="/CN=nestbox-egress-$node_name/O=Nestbox egress inspection"

# 1. The key, 0600 from the first byte it exists.
openssl ecparam -name prime256v1 -genkey -noout -out "$key" 2>/dev/null
chmod 0600 "$key"

# 2. A self-signed certificate with the CA flag set. `basicConstraints=critical,
#    CA:TRUE` is what makes it a root rather than a leaf; `pathlen:0` stops it
#    signing another CA.
openssl req -new -x509 -key "$key" -sha256 -days 3650 \
    -subj "$subject" \
    -addext "basicConstraints=critical,CA:TRUE,pathlen:0" \
    -addext "keyUsage=critical,keyCertSign,cRLSign" \
    -addext "subjectKeyIdentifier=hash" \
    -out "$crt"
chmod 0644 "$crt"

echo "created $crt"
openssl x509 -in "$crt" -noout -subject -enddate -fingerprint -sha256
echo
echo "next: rebuild the sandbox images so they trust this CA"
echo "  the image copies $crt to /usr/local/share/ca-certificates/nestbox-egress.crt"
echo "  and runs update-ca-certificates"
echo
echo "the key stays on this node at $key (0600). It must never be copied into a"
echo "sandbox and never be stored in PostgreSQL."
