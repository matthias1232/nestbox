#!/usr/bin/env bash
# Nestbox node join. Run as root on the machine that should become a node:
#   curl -fsSL "__CONTROL_URL__/api/v1/join/__TOKEN__/script" | sudo bash
#
# What it does, in order: installs Docker and WireGuard if they are missing, makes a WireGuard key
# (the private half never leaves this machine), trades the one-time token for an identity, brings
# up the tunnel to the control plane, and starts the node's runner and egress proxy from images the
# control plane serves. It is safe to run again: the token is single-use, so a second run stops
# with a clear message instead of making a second node.
set -euo pipefail

CONTROL="__CONTROL_URL__"
TOKEN="__TOKEN__"
DIR=/opt/nestbox-node

say() { printf '\033[1m==> %s\033[0m\n' "$*"; }
need_root() { [ "$(id -u)" = 0 ] || { echo "Run this as root (sudo bash)." >&2; exit 1; }; }
need_root

say "Preparing the machine"
export DEBIAN_FRONTEND=noninteractive
if command -v apt-get >/dev/null 2>&1; then
  apt-get update -qq
  apt-get install -y -qq wireguard-tools python3 curl ca-certificates openssl >/dev/null
else
  for tool in wg python3 curl openssl; do
    command -v "$tool" >/dev/null 2>&1 || { echo "Install '$tool' first; this installer knows apt only." >&2; exit 1; }
  done
fi
if ! command -v docker >/dev/null 2>&1; then
  say "Installing Docker"
  curl -fsSL https://get.docker.com | sh >/dev/null
fi
systemctl enable --now docker >/dev/null 2>&1 || true

say "Making this node's WireGuard key"
umask 077
mkdir -p "$DIR/secrets" /etc/wireguard
[ -s /etc/wireguard/nb0.key ] || wg genkey > /etc/wireguard/nb0.key
PUB=$(wg pubkey < /etc/wireguard/nb0.key)

say "Joining the control plane"
REQUEST=$(python3 - "$PUB" "$(hostname)" <<'PY'
import json, sys
print(json.dumps({"public_key": sys.argv[1], "hostname": sys.argv[2]}))
PY
)
ANSWER=$(curl -fsS -X POST -H 'content-type: application/json' -d "$REQUEST" "$CONTROL/api/v1/join/$TOKEN") || {
  echo "The control plane refused the join. The link is single-use and expires; ask for a new one." >&2
  exit 1
}
field() { python3 -c 'import json,sys; print(json.load(sys.stdin)[sys.argv[1]])' "$1" <<<"$ANSWER"; }
NODE_NAME=$(field node_name)
OVERLAY_IP=$(field overlay_ip)
HUB_PUB=$(field hub_public_key)
HUB_ENDPOINT=$(field hub_endpoint)
HUB_IP=$(field hub_ip)
REGISTRY=$(field registry)
CONTROL_OVERLAY=$(field control_url)
NODE_TOKEN=$(field node_token)
NODE_ID=$(field node_id)
ENV_ID=$(field environment_id)

say "Bringing up the tunnel ($OVERLAY_IP -> $HUB_ENDPOINT)"
cat > /etc/wireguard/nb0.conf <<CONF
[Interface]
Address = $OVERLAY_IP/32
PrivateKey = $(cat /etc/wireguard/nb0.key)

[Peer]
PublicKey = $HUB_PUB
Endpoint = $HUB_ENDPOINT
AllowedIPs = $HUB_IP/32
PersistentKeepalive = 25
CONF
chmod 600 /etc/wireguard/nb0.conf
systemctl enable wg-quick@nb0 >/dev/null 2>&1 || true
wg-quick down nb0 >/dev/null 2>&1 || true
wg-quick up nb0
for _ in $(seq 1 20); do ping -c1 -W1 "$HUB_IP" >/dev/null 2>&1 && break; sleep 1; done
ping -c1 -W2 "$HUB_IP" >/dev/null 2>&1 || { echo "The tunnel is up but $HUB_IP does not answer. Is UDP ${HUB_ENDPOINT##*:} open to the control plane?" >&2; exit 1; }

say "Letting Docker pull from the control plane's registry ($REGISTRY)"
CHANGED=$(python3 - "$REGISTRY" <<'PY'
import json, os, sys
path = "/etc/docker/daemon.json"
conf = json.load(open(path)) if os.path.exists(path) and os.path.getsize(path) else {}
reg = conf.setdefault("insecure-registries", [])
if sys.argv[1] in reg:
    print("same")
else:
    reg.append(sys.argv[1])
    os.makedirs("/etc/docker", exist_ok=True)
    json.dump(conf, open(path, "w"), indent=2)
    print("changed")
PY
)
if [ "$CHANGED" = changed ]; then systemctl restart docker; sleep 3; fi

say "Writing the node's files"
printf '%s' "$NODE_TOKEN" > "$DIR/secrets/node-token"
chmod 644 "$DIR/secrets/node-token"
mkdir -p "$DIR/secrets/proxy" "$DIR/state" /var/lib/nestbox/persistent /var/lib/nestbox/backups /var/lib/nestbox/egress
if [ ! -s "$DIR/secrets/proxy/egress-ca.key" ]; then
  openssl req -x509 -newkey rsa:2048 -nodes -days 3650 -subj "/CN=Nestbox node CA ($NODE_NAME)" \
    -keyout "$DIR/secrets/proxy/egress-ca.key" -out "$DIR/secrets/proxy/egress-ca.crt" >/dev/null 2>&1
fi
cp "$DIR/secrets/node-token" "$DIR/secrets/proxy/node-token"
chmod 644 "$DIR/secrets/proxy/"*
# The proxy and the runner run as their own users; they must be able to walk to these files.
# (The umask above is 077 for the WireGuard key, which is the one file that stays private.)
chmod 755 "$DIR" "$DIR/secrets" "$DIR/secrets/proxy"
cat > "$DIR/.env" <<ENV
REGISTRY=$REGISTRY
OVERLAY_IP=$OVERLAY_IP
HUB_IP=$HUB_IP
CONTROL_OVERLAY=$CONTROL_OVERLAY
CONTROL_HOST=$(python3 -c 'import sys,urllib.parse as u; p=u.urlparse(sys.argv[1]); print(p.hostname)' "$CONTROL_OVERLAY")
CONTROL_PORT=$(python3 -c 'import sys,urllib.parse as u; p=u.urlparse(sys.argv[1]); print(p.port or 80)' "$CONTROL_OVERLAY")
NODE_ID=$NODE_ID
NODE_NAME=$NODE_NAME
ENVIRONMENT_ID=$ENV_ID
NODE_DIR=$DIR
ENV
cat > "$DIR/docker-compose.yml" <<'COMPOSE'
__COMPOSE__
COMPOSE

say "Installing the sync timer (the node follows the control plane's version)"
cat > /usr/local/bin/nestbox-node-sync <<SYNC
#!/usr/bin/env bash
# Fetch the current node package from the control plane; if it changed, or a newer image exists,
# recreate the node's containers. Safe to run by hand.
set -euo pipefail
cd $DIR
. ./.env
TMP=\$(mktemp)
if curl -fsS -m 20 -H "Authorization: Bearer \$(cat secrets/node-token)" -H "x-nestbox-node: \$NODE_ID" "\$CONTROL_OVERLAY/api/v1/node/compose" -o "\$TMP" && [ -s "\$TMP" ]; then
  if ! cmp -s "\$TMP" docker-compose.yml; then cp "\$TMP" docker-compose.yml; echo "node package updated"; fi
fi
rm -f "\$TMP"
docker compose --env-file .env pull -q
docker compose --env-file .env up -d
SYNC
chmod 755 /usr/local/bin/nestbox-node-sync
cat > /etc/systemd/system/nestbox-node-sync.service <<UNIT
[Unit]
Description=Bring this Nestbox node in step with its control plane
After=network-online.target docker.service wg-quick@nb0.service
[Service]
Type=oneshot
ExecStart=/usr/local/bin/nestbox-node-sync
UNIT
cat > /etc/systemd/system/nestbox-node-sync.timer <<UNIT
[Unit]
Description=Nestbox node sync every five minutes
[Timer]
OnBootSec=2min
OnUnitActiveSec=5min
[Install]
WantedBy=timers.target
UNIT
systemctl daemon-reload
systemctl enable --now nestbox-node-sync.timer >/dev/null 2>&1 || true

say "Pulling and starting the runner and the egress proxy"
cd "$DIR"
docker compose --env-file .env pull
docker compose --env-file .env up -d
for _ in $(seq 1 30); do curl -fsS "http://$OVERLAY_IP:9000/healthz" >/dev/null 2>&1 && break; sleep 2; done
curl -fsS "http://$OVERLAY_IP:9000/healthz" >/dev/null 2>&1 || { echo "The runner did not come up; see: docker compose -f $DIR/docker-compose.yml logs" >&2; exit 1; }
say "Done. This machine is node '$NODE_NAME'. It shows up online in the console within a minute."
