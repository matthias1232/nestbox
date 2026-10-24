#!/usr/bin/env bash
# Install the portable Nestbox foundation on a clean Ubuntu 24.04+ host.
# Usage: curl -fsSL <raw-url>/scripts/bootstrap/ubuntu.sh | sudo bash
set -euo pipefail

REPO_URL="${NESTBOX_REPO_URL:-https://github.com/matthias1232/nestbox.git}"
REF="${NESTBOX_REF:-main}"
INSTALL_DIR="${NESTBOX_INSTALL_DIR:-/opt/nestbox}"
CONFIG_DIR="${NESTBOX_CONFIG_DIR:-/etc/nestbox}"

if [ "$(id -u)" -ne 0 ]; then
  exec sudo --preserve-env=NESTBOX_REPO_URL,NESTBOX_REF,NESTBOX_INSTALL_DIR,NESTBOX_CONFIG_DIR bash "$0" "$@"
fi

export DEBIAN_FRONTEND=noninteractive
apt-get update -qq
apt-get install -y -qq ca-certificates curl git openssl docker.io docker-compose-v2
systemctl enable --now docker

if [ -d "$INSTALL_DIR/.git" ]; then
  git -C "$INSTALL_DIR" fetch --depth 1 origin "$REF"
  git -C "$INSTALL_DIR" checkout --detach FETCH_HEAD
else
  git clone --depth 1 --branch "$REF" "$REPO_URL" "$INSTALL_DIR"
fi

install -d -m 0755 "$CONFIG_DIR" /var/lib/openshell
if [ ! -s "$CONFIG_DIR/runtime.env" ]; then
  cp "$INSTALL_DIR/compose/.env.runtime.example" "$CONFIG_DIR/runtime.env"
fi
if [ ! -s "$CONFIG_DIR/provider.env" ]; then
  cp "$INSTALL_DIR/compose/provider.env.example" "$CONFIG_DIR/provider.env"
  chmod 0600 "$CONFIG_DIR/provider.env"
fi

if ! grep -q '^NESTBOX_SECRET_KEY=' "$CONFIG_DIR/nestbox.env" 2>/dev/null; then
  umask 077
  cat > "$CONFIG_DIR/nestbox.env" <<ENV
NESTBOX_SECRET_KEY=$(openssl rand -base64 48 | tr -d '\n')
NESTBOX_DB_PASSWORD=$(openssl rand -base64 24 | tr -d '\n')
NESTBOX_DOCKER_GID=$(stat -c '%g' /var/run/docker.sock)
NESTBOX_ALLOWED_HOSTS=*
NESTBOX_BOOTSTRAP_ADMIN_EMAIL=${NESTBOX_BOOTSTRAP_ADMIN_EMAIL:-admin@example.com}
NESTBOX_BOOTSTRAP_ADMIN_PASSWORD=${NESTBOX_BOOTSTRAP_ADMIN_PASSWORD:-change-me-now}
ENV
fi

cd "$INSTALL_DIR"
docker compose --env-file "$CONFIG_DIR/nestbox.env" --env-file "$CONFIG_DIR/runtime.env" \
  -f compose/docker-compose.yml --profile runtime up -d --build
echo "Nestbox controller: https://<host>/"
echo "OpenShell gateway: http://127.0.0.1:8080 (gRPC gateway; verify with the OpenShell CLI)"
echo "Next: edit $CONFIG_DIR/provider.env, then run $INSTALL_DIR/scripts/nemoclaw/onboard.sh"
