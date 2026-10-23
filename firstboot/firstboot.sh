#!/bin/bash
# Nestbox first boot.
#
# Applies the network configuration recorded in /etc/nestbox/recipe.json, brings up the
# container stack, registers the gateway seed and writes a self-check report the UI can show.
set -euo pipefail

RECIPE=/etc/nestbox/recipe.json
STATE=/var/lib/nestbox
LOG=/var/log/nestbox/firstboot.log
mkdir -p "$STATE" "$(dirname "$LOG")"
exec > >(tee -a "$LOG") 2>&1

echo "[nestbox] first boot starting"
test -f "$RECIPE" || { echo "[nestbox] no recipe, nothing to do"; exit 0; }

json() { python3 -c "import json,sys;d=json.load(open('$RECIPE'));print($1)"; }

MODE=$(json "d.get('network_mode','dhcp')")
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
  ADDRESS=$(json "d.get('address','')")
  GATEWAY=$(json "d.get('gateway','')")
  DNS=$(json "' '.join(d.get('dns',['1.1.1.1']))")
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

echo "[nestbox] starting container stack"
if [ -f /opt/nestbox/compose/docker-compose.yml ]; then
  docker compose -f /opt/nestbox/compose/docker-compose.yml up -d
else
  echo "[nestbox] compose file not present; skipping stack start"
fi

# Gateway seed: the controller registers the local gateway as the system gateway.
if [ -f /etc/openshell/gateways/local/metadata.json ]; then
  echo "[nestbox] gateway seed present"
else
  echo "[nestbox] no gateway seed; nests will be recorded as pending until one is registered"
fi

PROFILE=$(json "d.get('profile','slim')")
{
  echo "# Nestbox self-check"
  echo
  echo "- profile: $PROFILE"
  echo "- network: $MODE"
  echo "- generated: $(date -u +%FT%TZ)"
  echo "- containers: $(docker ps -q 2>/dev/null | wc -l)"
} > "$STATE/self-check.md"

touch "$STATE/.firstboot-done"
echo "[nestbox] first boot complete"
