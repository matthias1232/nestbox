#!/bin/bash
# Nestbox first boot.
#
# Runs once, after the unattended install: installs the runtime packages with retries, applies the
# network configuration, starts the stack and writes a self-check report the UI can show.
set -euo pipefail

RECIPE=/etc/nestbox/recipe.json
STATE=/var/lib/nestbox
LOG=/var/log/nestbox/firstboot.log
mkdir -p "$STATE" "$(dirname "$LOG")"
exec > >(tee -a "$LOG") 2>&1

echo "[nestbox] first boot starting"
test -f "$RECIPE" || { echo "[nestbox] no recipe, nothing to do"; exit 0; }

json() { python3 -c "import json;d=json.load(open('$RECIPE'));print($1)"; }

retry() {
  local n=0
  until "$@"; do
    n=$((n + 1))
    [ "$n" -ge 5 ] && { echo "[nestbox] giving up after $n attempts: $*"; return 1; }
    echo "[nestbox] retry $n: $*"
    sleep $((n * 5))
  done
}

MODE=$(json "d.get('network',{}).get('mode','dhcp')")
echo "[nestbox] network mode: $MODE"

if [ "$MODE" = "dhcp" ]; then
  cat > /etc/netplan/10-nestbox.yaml <<'YAML'
network:
  version: 2
  ethernets:
    all-en:
      match: {name: "en*"}
      dhcp4: true
YAML
else
  ADDRESS=$(json "d.get('network',{}).get('address','')")
  GATEWAY=$(json "d.get('network',{}).get('gateway','')")
  DNS=$(json "' '.join(d.get('network',{}).get('dns') or ['1.1.1.1'])")
  cat > /etc/netplan/10-nestbox.yaml <<YAML
network:
  version: 2
  ethernets:
    all-en:
      match: {name: "en*"}
      dhcp4: false
      addresses: ["$ADDRESS"]
      routes: [{to: "default", via: "$GATEWAY"}]
      nameservers:
        addresses: [$DNS]
YAML
fi
chmod 600 /etc/netplan/10-nestbox.yaml
netplan apply || echo "[nestbox] netplan apply failed; review /etc/netplan/10-nestbox.yaml"

echo "[nestbox] installing runtime packages"
export DEBIAN_FRONTEND=noninteractive
retry apt-get update -qq || true
retry apt-get install -y -qq ca-certificates curl jq git chrony openssl docker.io docker-compose-v2 || \
  echo "[nestbox] package installation incomplete; the controller may not start"

systemctl enable --now chrony || true
systemctl enable --now docker || true

PROFILE=$(json "d.get('profile','slim')")
UI_PORT=$(json "d.get('ports',{}).get('ui',443)")
TLS_MODE=$(json "d.get('tls',{}).get('mode','self_signed')")
TLS_DOMAIN=$(json "d.get('tls',{}).get('domain','') or ''")
TLS_DIR=/etc/nestbox/tls
ENV_FILE=/etc/nestbox/nestbox.env
mkdir -p "$TLS_DIR" /etc/nestbox

echo "[nestbox] preparing the controller environment"
if [ ! -f "$ENV_FILE" ]; then
  ADMIN_EMAIL="admin@${TLS_DOMAIN:-nestbox.local}"
  ADMIN_PASSWORD=$(head -c 18 /dev/urandom | base64 | tr -d '=+/')
  cat > "$ENV_FILE" <<ENV
NESTBOX_SECRET_KEY=$(head -c 48 /dev/urandom | base64 | tr -d '=+/')
NESTBOX_DB_PASSWORD=$(head -c 24 /dev/urandom | base64 | tr -d '=+/')
NESTBOX_ALLOWED_HOSTS=*
NESTBOX_UI_PORT=$UI_PORT
NESTBOX_BOOTSTRAP_ADMIN_EMAIL=$ADMIN_EMAIL
NESTBOX_BOOTSTRAP_ADMIN_PASSWORD=$ADMIN_PASSWORD
ENV
  chmod 600 "$ENV_FILE"
  printf '%s\n' "$ADMIN_PASSWORD" > "$STATE/admin-password.txt"
  chmod 600 "$STATE/admin-password.txt"
  NEW_ADMIN=1
fi

if [ "$TLS_MODE" != "none" ] && [ ! -s "$TLS_DIR/cert.pem" ]; then
  echo "[nestbox] creating a self-signed certificate for ${TLS_DOMAIN:-$(hostname)}"
  CERT_IP=$(hostname -I | awk '{print $1}')
  openssl req -x509 -nodes -newkey ec -pkeyopt ec_paramgen_curve:prime256v1 -days 825 \
    -subj "/CN=${TLS_DOMAIN:-$(hostname)}" \
    -addext "subjectAltName=DNS:${TLS_DOMAIN:-$(hostname)},DNS:$(hostname),DNS:$(hostname).local,IP:$CERT_IP" \
    -keyout "$TLS_DIR/key.pem" -out "$TLS_DIR/cert.pem" 2>/dev/null || \
    echo "[nestbox] certificate creation failed; the UI stays on http"
  if [ -s "$TLS_DIR/key.pem" ]; then chmod 600 "$TLS_DIR/key.pem"; fi
fi

# The controller runs as uid 10001 inside its container; the mounted pair has to be
# readable for that user, and only for that user.
if [ -s "$TLS_DIR/key.pem" ]; then
  chown 10001:10001 "$TLS_DIR/key.pem" "$TLS_DIR/cert.pem" 2>/dev/null || true
  chmod 600 "$TLS_DIR/key.pem"
  chmod 644 "$TLS_DIR/cert.pem"
fi

SCHEME=http
if [ -s "$TLS_DIR/cert.pem" ]; then SCHEME=https; fi

echo "[nestbox] starting the stack (profile $PROFILE, port $UI_PORT)"
if [ -f /opt/nestbox/compose/docker-compose.yml ]; then
  retry docker compose --env-file "$ENV_FILE" -f /opt/nestbox/compose/docker-compose.yml up -d --build
else
  echo "[nestbox] compose file missing at /opt/nestbox/compose/docker-compose.yml"
fi

UI_HOST=$(hostname -I | awk '{print $1}')

{
  echo "# Nestbox self-check"
  echo
  echo "- profile: $PROFILE"
  echo "- network: $MODE"
  echo "- hostname: $(hostname)"
  echo "- addresses: $(hostname -I)"
  echo "- ui: $SCHEME://$UI_HOST:$UI_PORT/"
  echo "- generated: $(date -u +%FT%TZ)"
  echo "- containers: $(docker ps -q 2>/dev/null | wc -l)"
} > "$STATE/self-check.md"

touch "$STATE/.firstboot-done"
echo "[nestbox] first boot complete"
echo "[nestbox] web UI: $SCHEME://$UI_HOST:$UI_PORT/"

BANNER=$(mktemp)
{
  echo
  echo "Nestbox is ready."
  echo "  web UI: $SCHEME://$UI_HOST:$UI_PORT/"
  if [ "${NEW_ADMIN:-0}" = "1" ]; then
    echo "  admin:    $ADMIN_EMAIL"
    echo "  password: $ADMIN_PASSWORD"
    echo "  password file: $STATE/admin-password.txt"
  fi
  echo
} > "$BANNER"

# The unit logs to the journal; the summary also goes to the console so whoever sits in
# front of the machine or its Proxmox console sees the URL and the first password.
cat "$BANNER" >> /var/log/nestbox/firstboot.log || true
cat "$BANNER" > /dev/console 2>/dev/null || true
cat "$BANNER" > /dev/tty1 2>/dev/null || true
rm -f "$BANNER"


