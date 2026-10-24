"""Deterministic provisioning artefacts for a build recipe.

The builds module owns the recipe and the ISO seed. Provisioning consumes both and renders the
plans an operator runs outside the image: an idempotent installer, a Brev setup script and a
Proxmox plan. The dependency only points this way (provisioning -> builds), so builds never has to
know which deployment targets exist.
"""
from __future__ import annotations

import json
import shlex
from typing import Any

import yaml

from apps.builds.models import BuildRecipe
from apps.builds.services import (
    preseed_nests,
    recipe_warnings,
    render_chrony,
    render_manifest,
    render_netplan,
    render_seed,
)
from apps.core.crypto import decrypt


# ---------------------------------------------------------------------------------------------
# setup scripts
# ---------------------------------------------------------------------------------------------
def _env_block(recipe: BuildRecipe) -> str:
    password = decrypt(recipe.admin_password_encrypted)
    lines = [
        f"NESTBOX_HOSTNAME={shlex.quote(recipe.hostname)}",
        f"NESTBOX_ADMIN_EMAIL={shlex.quote(recipe.admin_email)}",
        f"NESTBOX_ADMIN_PASSWORD={shlex.quote(password)}",
        f"NESTBOX_UI_PORT={recipe.ui_port}",
        f"NESTBOX_TLS_MODE={shlex.quote(recipe.tls_mode)}",
        f"NESTBOX_TLS_DOMAIN={shlex.quote(recipe.tls_domain)}",
        f"NESTBOX_TLS_EMAIL={shlex.quote(recipe.tls_email)}",
        f"NESTBOX_NETWORK_MODE={shlex.quote(recipe.network_mode)}",
        f"NESTBOX_DNS_SERVERS={shlex.quote(' '.join(recipe.dns))}",
    ]
    if recipe.dns_domain:
        lines.append(f"NESTBOX_DNS_DOMAIN={shlex.quote(recipe.dns_domain)}")
    return "\n".join(lines)


def render_install_script(recipe: BuildRecipe) -> str:
    """Idempotent installer for an existing Ubuntu 22.04/24.04/26.04 host."""
    nests = preseed_nests(recipe)
    nest_lines = "\n".join(
        f'  create_nest {shlex.quote(n["name"])} {shlex.quote(n["pack"])} {n["port"]}'
        for n in nests
    )
    return f"""#!/usr/bin/env bash
# Nestbox installer — generated from recipe {recipe.name!r}.
# Target: existing Ubuntu {recipe.ubuntu_series}. Idempotent: safe to re-run.
set -euo pipefail

{_env_block(recipe)}

REPO_URL="${{NESTBOX_REPO_URL:-https://github.com/matthias1232/nestbox.git}}"
INSTALL_DIR="${{NESTBOX_INSTALL_DIR:-/opt/nestbox}}"
STATE_DIR=/var/lib/nestbox
CONF_DIR=/etc/nestbox

log() {{ printf '[nestbox] %s\\n' "$*"; }}

if [ "$(id -u)" -ne 0 ]; then
  log "re-running with sudo"
  exec sudo bash "$0" "$@"
fi

log "installing base packages"
export DEBIAN_FRONTEND=noninteractive
apt-get update -qq
apt-get install -y -qq ca-certificates curl git jq wireguard-tools chrony {"docker.io docker-compose-v2" if recipe.install_docker else ""}

mkdir -p "$STATE_DIR" "$CONF_DIR" /var/log/nestbox

log "writing network configuration"
cat > /etc/netplan/10-nestbox.yaml <<'NETPLAN'
{yaml.safe_dump(render_netplan(recipe), sort_keys=False)}
NETPLAN
chmod 600 /etc/netplan/10-nestbox.yaml
if command -v netplan >/dev/null 2>&1; then netplan apply || log "netplan apply failed"; fi

log "writing NTP and resolver configuration"
cat > /etc/chrony/conf.d/10-nestbox.conf <<'CHRONY'
{render_chrony(recipe)}
CHRONY
systemctl enable --now chrony || true

log "fetching Nestbox"
if [ -d "$INSTALL_DIR/.git" ]; then
  git -C "$INSTALL_DIR" fetch --depth 1 origin main
  git -C "$INSTALL_DIR" reset --hard origin/main
else
  mkdir -p "$INSTALL_DIR"
  git clone --depth 1 "$REPO_URL" "$INSTALL_DIR"
fi

log "writing environment"
cat > "$CONF_DIR/nestbox.env" <<'ENV'
NESTBOX_SECRET_KEY=$(head -c 48 /dev/urandom | base64 | tr -d '=+/')
NESTBOX_ALLOWED_HOSTS=*
NESTBOX_BOOTSTRAP_ADMIN_EMAIL=$NESTBOX_ADMIN_EMAIL
NESTBOX_BOOTSTRAP_ADMIN_PASSWORD=$NESTBOX_ADMIN_PASSWORD
NESTBOX_UI_PORT=$NESTBOX_UI_PORT
ENV
chmod 600 "$CONF_DIR/nestbox.env"

log "starting the stack"
cd "$INSTALL_DIR"
docker compose --env-file "$CONF_DIR/nestbox.env" up -d --build

create_nest() {{
  local name="$1" pack="$2" port="$3"
  log "preseeding nest $name ($pack) on port $port"
  curl -fsS -X POST "http://127.0.0.1:${{NESTBOX_UI_PORT}}/api/v1/sandboxes/" \\
    -H "Content-Type: application/json" \\
    -d "{{\\"name\\":\\"$name\\",\\"agent_pack_key\\":\\"$pack\\"}}" || log "nest $name could not be created yet"
}}

{nest_lines or "  : # no preseed nests"}
log "done. Open http://$(hostname -I | awk '{{print $1}}'):$NESTBOX_UI_PORT/"
"""


def render_brev_setup(recipe: BuildRecipe) -> str:
    """Setup script for `brev start --setup-path`. Runs as the default user, uses sudo."""
    nests = preseed_nests(recipe)
    nest_lines = "\n".join(
        f'create_nest {shlex.quote(n["name"])} {shlex.quote(n["pack"])} {n["port"]}' for n in nests
    )
    return f"""#!/usr/bin/env bash
# Nestbox setup script for NVIDIA Brev — generated from recipe {recipe.name!r}.
# Brev runs this as the default user after the instance boots.
set -euo pipefail

{_env_block(recipe)}

REPO_URL="${{NESTBOX_REPO_URL:-https://github.com/matthias1232/nestbox.git}}"
INSTALL_DIR="${{NESTBOX_INSTALL_DIR:-$HOME/nestbox}}"

log() {{ printf '[nestbox/brev] %s\\n' "$*"; }}

log "system packages (sudo)"
sudo apt-get update -qq
sudo apt-get install -y -qq ca-certificates curl git jq {"docker.io docker-compose-v2" if recipe.install_docker else ""}
sudo usermod -aG docker "$USER" || true

log "checkout"
if [ -d "$INSTALL_DIR/.git" ]; then
  git -C "$INSTALL_DIR" fetch --depth 1 origin main && git -C "$INSTALL_DIR" reset --hard origin/main
else
  git clone --depth 1 "$REPO_URL" "$INSTALL_DIR"
fi

log "configuration"
sudo mkdir -p /etc/nestbox /var/lib/nestbox
sudo tee /etc/nestbox/nestbox.env >/dev/null <<'ENV'
NESTBOX_SECRET_KEY=$(head -c 48 /dev/urandom | base64 | tr -d '=+/')
NESTBOX_ALLOWED_HOSTS=*
NESTBOX_BOOTSTRAP_ADMIN_EMAIL=$NESTBOX_ADMIN_EMAIL
NESTBOX_BOOTSTRAP_ADMIN_PASSWORD=$NESTBOX_ADMIN_PASSWORD
ENV

log "starting the stack on port $NESTBOX_UI_PORT"
cd "$INSTALL_DIR"
sudo docker compose --env-file /etc/nestbox/nestbox.env up -d --build

create_nest() {{
  local name="$1" pack="$2" port="$3"
  log "preseeding nest $name ($pack) on port $port"
  curl -fsS -X POST "http://127.0.0.1:$NESTBOX_UI_PORT/api/v1/sandboxes/" \\
    -H "Content-Type: application/json" \\
    -d "{{\\"name\\":\\"$name\\",\\"agent_pack_key\\":\\"$pack\\"}}" || true
}}

{nest_lines or "  : # no preseed nests"}

IP=$(hostname -I | awk '{{print $1}}')
log "ready: http://$IP:$NESTBOX_UI_PORT/  (use your Brev port forward / connect)"
"""


def render_brev_command(recipe: BuildRecipe, *, gpu: str = "n1-highmem-4:nvidia-l4:1") -> str:
    script = f"dist/brev/{recipe.name}.sh"
    return (
        f'brev start https://github.com/matthias1232/nestbox \\\n'
        f'  --name {recipe.name} --gpu "{gpu}" \\\n'
        f'  --setup-repo . --setup-path ./{script}'
    )


def render_proxmox_plan(recipe: BuildRecipe, *, vmid: int = 201, node: str = "pve01",
                        storage: str = "local-btrfs", bridge: str = "vmbr0",
                        memory_mb: int = 4096, disk_gb: int = 100, cores: int = 2) -> str:
    iso = f"nestbox-{recipe.ubuntu_series}-{recipe.profile}.iso"
    return f"""#!/usr/bin/env bash
# Proxmox provisioning plan for recipe {recipe.name!r}.
# Run on the Proxmox host. Creates a VM, attaches the built ISO, boots it with a serial console.
set -euo pipefail

VMID={vmid}
NODE={node}
STORAGE={storage}
BRIDGE={bridge}
ISO=/var/lib/vz/template/iso/{iso}

qm create "$VMID" --name {shlex.quote(recipe.name)} --node "$NODE" \\
  --memory {memory_mb} --cores {cores} --cpu host --ostype l26 \\
  --scsihw virtio-scsi-single --net0 virtio,bridge="$BRIDGE" \\
  --serial0 socket --vga serial0 --agent enabled=1 \\
  --boot order=ide2 --ide2 "$STORAGE":iso/{iso},media=cdrom

qm set "$VMID" --scsi0 "$STORAGE":{disk_gb}
qm start "$VMID"
echo "attached $ISO to VM $VMID; follow the install with: qm terminal $VMID"
"""


def render_provisioning(recipe: BuildRecipe) -> dict[str, Any]:
    """Every deployment artefact, rendered from the same recipe as the ISO.

    The seed is included because provisioning targets need the same cloud-init document the image
    uses; it is rendered by ``apps.builds.services`` rather than duplicated here.
    """
    return {
        "seed": render_seed(recipe),
        "install.sh": render_install_script(recipe),
        "brev-setup.sh": render_brev_setup(recipe),
        "brev-command.txt": render_brev_command(recipe),
        "proxmox-plan.sh": render_proxmox_plan(recipe),
        "manifest.json": json.dumps(render_manifest(recipe), indent=2),
        "warnings": recipe_warnings(recipe),
    }


#: The aggregate kept its old name so callers that moved with the renderers keep working.
render_all = render_provisioning

PROVISIONING_RENDERERS = {
    "install.sh": render_install_script,
    "brev-setup.sh": render_brev_setup,
    "brev-command.txt": render_brev_command,
    "proxmox-plan.sh": render_proxmox_plan,
    "manifest.json": lambda r: json.dumps(render_manifest(r), indent=2),
}
