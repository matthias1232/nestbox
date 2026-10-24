#!/usr/bin/env bash
# Pass as the setup script to `brev start --setup-path`.
set -euo pipefail

export NESTBOX_INSTALL_DIR="${NESTBOX_INSTALL_DIR:-$HOME/nestbox}"
export NESTBOX_CONFIG_DIR="${NESTBOX_CONFIG_DIR:-/etc/nestbox}"
exec "$(dirname "$0")/ubuntu.sh" "$@"
