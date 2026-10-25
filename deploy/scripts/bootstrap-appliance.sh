#!/usr/bin/env bash
set -euo pipefail

# Run once as root on a fresh Ubuntu 24.04 appliance.
# This script installs no Django packages or services.
APP_USER=nestbox
APP_DIR=/opt/nestbox
SSH_KEY='ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAICtmR6u9ewhyekpNhM3GHw7gCZUoJXfB9QF3nWeF3vJZ nestbox-builder'

apt-get update
apt-get install -y ca-certificates curl git docker.io docker-compose-plugin
id "$APP_USER" >/dev/null 2>&1 || useradd -m -s /bin/bash "$APP_USER"
usermod -aG docker,sudo "$APP_USER"
install -d -m 700 -o "$APP_USER" -g "$APP_USER" "/home/$APP_USER/.ssh"
printf '%s\n' "$SSH_KEY" > "/home/$APP_USER/.ssh/authorized_keys"
chown "$APP_USER:$APP_USER" "/home/$APP_USER/.ssh/authorized_keys"
chmod 600 "/home/$APP_USER/.ssh/authorized_keys"
install -d -m 0755 -o "$APP_USER" -g "$APP_USER" "$APP_DIR"
systemctl enable --now docker
printf '%s\n' 'Bootstrap complete. Deploy the FastAPI and React Admin stack with deploy/scripts/deploy-appliance.sh.'
