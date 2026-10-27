#!/usr/bin/env bash
set -euo pipefail

# Place the inference credential where the seeded provider profile expects it.
#
# The value is read from OMNIROUTE_API_KEY when set, otherwise prompted without
# echo. It is written straight to a 0600 file: it never becomes a command
# argument, never enters shell history, and never appears in a log.
#
# Works both on the appliance and from a workstation that has an SSH route to the
# Docker host (set NESTBOX_SSH, e.g. NESTBOX_SSH=root@pve01).

SECRET_DIR=${NESTBOX_SECRET_DIR:-secrets}
TARGET_FILE="$SECRET_DIR/omniroute_api_key"

read_key() {
  if [[ -n "${OMNIROUTE_API_KEY:-}" ]]; then
    printf '%s' "$OMNIROUTE_API_KEY"
    return
  fi
  local value
  read -r -s -p "Inference API key: " value
  printf '\n' >&2
  printf '%s' "$value"
}

write_locally() {
  mkdir -p "$SECRET_DIR"
  chmod 0700 "$SECRET_DIR"
  # A temp file plus mv keeps the mode and avoids a half-written secret.
  local tmp
  tmp=$(mktemp "$SECRET_DIR/.omniroute_api_key.XXXXXX")
  read_key > "$tmp"
  chmod 0600 "$tmp"
  mv "$tmp" "$TARGET_FILE"
  echo "wrote $TARGET_FILE (mode 0600)"
}

if [[ -n "${NESTBOX_SSH:-}" ]]; then
  # Remote path: send the value over stdin so it is not visible in `ps`.
  key=$(read_key)
  printf '%s' "$key" | ssh "$NESTBOX_SSH" \
    "set -e; d='$SECRET_DIR'; mkdir -p \"\$d\"; chmod 0700 \"\$d\"; t=\$(mktemp \"\$d/.k.XXXXXX\"); cat > \"\$t\"; chmod 0600 \"\$t\"; mv \"\$t\" \"\$d/omniroute_api_key\"; echo 'wrote '\"\$d\"'/omniroute_api_key'"
else
  write_locally
fi

echo "Recreate an agent sandbox to pick the key up; a running one keeps its old environment."
