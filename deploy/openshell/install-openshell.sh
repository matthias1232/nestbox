#!/usr/bin/env bash
# Install NVIDIA OpenShell (pinned release) and the gateway that Nestbox drives.
#
# Idempotent. Run on the appliance host as the user that owns the gateway
# (not root); it uses sudo for the package and for `loginctl enable-linger`.
#
#   deploy/openshell/install-openshell.sh [client-dir]
#
# The client directory (default: deploy/data/openshell-client) receives ONLY what
# the sandbox runner needs to authenticate to the gateway: the CA certificate, the
# client certificate and key, and the gateway registration. The CA private key and
# the JWT signing key stay in ~/.local/state/openshell/tls and are never shared.
set -euo pipefail

VERSION="0.1.6"
DEB="openshell_${VERSION}-1_amd64.deb"
BASE="https://github.com/NVIDIA/OpenShell/releases/download/v${VERSION}"
PORT=17670
HERE="$(cd "$(dirname "$0")" && pwd)"
CLIENT_DIR="${1:-$HERE/../data/openshell-client}"

if ! command -v openshell >/dev/null || [ "$(openshell --version | awk '{print $2}')" != "$VERSION" ]; then
  tmp="$(mktemp -d)"; trap 'rm -rf "$tmp"' EXIT
  ( cd "$tmp"
    curl -fLsS -O "$BASE/$DEB" -O "$BASE/openshell-checksums-sha256.txt"
    grep " $DEB\$" openshell-checksums-sha256.txt | sha256sum -c -
    sudo apt-get install -y "./$DEB" )
fi

sudo loginctl enable-linger "$USER"
export XDG_RUNTIME_DIR="/run/user/$(id -u)"
mkdir -p "$HOME/.config/openshell"
install -m 0600 "$HERE/gateway.toml" "$HOME/.config/openshell/gateway.toml"
systemctl --user enable openshell-gateway >/dev/null 2>&1
systemctl --user restart openshell-gateway

for _ in $(seq 1 30); do
  openshell gateway add "https://127.0.0.1:$PORT" --local --name openshell >/dev/null 2>&1 \
    && openshell status 2>/dev/null | grep -q "Authenticated" && break
  sleep 2
done
openshell status | head -8

# The runner's view: same gateway, reached by the name the server certificate
# already carries (host.openshell.internal), with the client bundle only.
mkdir -p "$CLIENT_DIR/gateways/openshell/mtls"
src="$HOME/.config/openshell/gateways/openshell/mtls"
install -m 0600 "$src/ca.crt" "$src/tls.crt" "$src/tls.key" "$CLIENT_DIR/gateways/openshell/mtls/"
printf '{\n  "name": "openshell",\n  "gateway_endpoint": "https://host.openshell.internal:%s",\n  "is_remote": false,\n  "gateway_port": 0,\n  "auth_mode": "mtls"\n}\n' "$PORT" \
  > "$CLIENT_DIR/gateways/openshell/metadata.json"
printf 'openshell' > "$CLIENT_DIR/active_gateway"
chmod -R go-rwx "$CLIENT_DIR"
echo "runner client bundle: $CLIENT_DIR"
