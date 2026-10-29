#!/usr/bin/env bash
set -euo pipefail

# Apply the pushed branch to the dev appliance.
#
# The running stack lives in a git checkout (`/opt/nestbox/site`), so a deploy is
# "push, then fast-forward that checkout and rebuild", not a file copy. An rsync
# with --delete into another directory once removed files from the box; a
# fast-forward cannot delete anything the branch does not itself remove, and it
# refuses (instead of overwriting) when the checkout has diverged.
#
#   git push origin HEAD            # first: the box pulls, it does not receive
#   deploy/scripts/deploy-appliance.sh
HOST=${NESTBOX_HOST:-192.168.55.50}
USER=${NESTBOX_USER:-nestbox}
KEY=${NESTBOX_KEY:-$HOME/.ssh/nestbox_builder}
SITE=${NESTBOX_SITE:-/opt/nestbox/site}
BRANCH=${NESTBOX_BRANCH:-$(git rev-parse --abbrev-ref HEAD)}

ssh -i "$KEY" "$USER@$HOST" \
  "cd '$SITE' && git fetch origin '$BRANCH' && git merge --ff-only FETCH_HEAD && cd deploy && docker compose up -d --build"
