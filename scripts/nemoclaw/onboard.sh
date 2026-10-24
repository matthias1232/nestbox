#!/usr/bin/env bash
# Explicit, provider-safe NemoClaw onboarding for a running Nestbox/OpenShell stack.
# This script deliberately does not accept credentials as arguments and never writes
# them into the repository or a Compose file.
set -euo pipefail

INSTALL_DIR="${NESTBOX_INSTALL_DIR:-/opt/nestbox}"
CONFIG_DIR="${NESTBOX_CONFIG_DIR:-/etc/nestbox}"
PROVIDER_ENV="${NESTBOX_PROVIDER_ENV:-$CONFIG_DIR/provider.env}"

if [ ! -r "$PROVIDER_ENV" ]; then
  echo "Provider configuration not found: $PROVIDER_ENV" >&2
  exit 2
fi

set -a
# shellcheck disable=SC1090
. "$PROVIDER_ENV"
set +a

: "${NEMOCLAW_PROVIDER:?Set NEMOCLAW_PROVIDER in $PROVIDER_ENV}"
: "${NEMOCLAW_AGENT:=openclaw}"
: "${NEMOCLAW_SANDBOX_NAME:=nestbox-agent}"

if ! command -v nemoclaw >/dev/null 2>&1; then
  echo "Installing NemoClaw through NVIDIA's official installer."
  echo "This requires your explicit acceptance of NVIDIA third-party software terms."
  curl -fsSL https://www.nvidia.com/nemoclaw.sh | bash
fi

cd "$INSTALL_DIR"
docker compose --env-file "$CONFIG_DIR/nestbox.env" --env-file "$CONFIG_DIR/runtime.env" \
  -f compose/docker-compose.yml --profile runtime up -d

echo "Starting NemoClaw onboarding for provider '$NEMOCLAW_PROVIDER' and sandbox '$NEMOCLAW_SANDBOX_NAME'."
echo "The installer will request any required provider credential interactively."
nemoclaw onboard
