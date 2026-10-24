#!/usr/bin/env bash
# Run inside Ubuntu 24.04 on WSL2. Docker Desktop WSL integration is preferred;
# a local Docker Engine is supported for development when systemd is enabled.
set -euo pipefail

if ! grep -qi microsoft /proc/sys/kernel/osrelease; then
  echo "This bootstrapper is intended for WSL2; use bootstrap/ubuntu.sh on native Linux." >&2
  exit 2
fi

exec "$(dirname "$0")/ubuntu.sh" "$@"
