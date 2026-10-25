#!/usr/bin/env bash
set -euo pipefail

# Run from this repository after bootstrap. Uses the retained project SSH key.
HOST=${NESTBOX_HOST:-192.168.55.50}
USER=${NESTBOX_USER:-nestbox}
KEY=${NESTBOX_KEY:-$HOME/.ssh/nestbox_builder}
TARGET=/opt/nestbox

rsync -az --delete -e "ssh -i $KEY" \
  --exclude '.git' --exclude 'graphify-out' --exclude 'node_modules' --exclude '.venv' \
  ./ "$USER@$HOST:$TARGET/"
ssh -i "$KEY" "$USER@$HOST" "cd $TARGET && docker compose -f deploy/docker-compose.yml up -d --build"
