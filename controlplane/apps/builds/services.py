"""Deterministic rendering of a recipe into every deployable artefact.

One recipe, four outputs: an Ubuntu autoinstall ISO, a cloud-init seed, a Brev setup script and a
Proxmox provisioning plan. They are rendered from the same fields, so they cannot drift apart, and
every function here is pure enough to unit-test without a hypervisor.
"""
from __future__ import annotations

import hashlib
import json
import shlex
from typing import Any

import yaml

from apps.builds.models import BuildRecipe
from apps.core.crypto import decrypt

# The installer must not depend on a package mirror: one failed apt download aborts the whole
# unattended install. Only packages that exist on the ISO itself go here; everything else is
# installed on first boot, where a failure is visible and retryable instead of fatal.
BASE_PACKAGES = ["openssh-server"]

DOCKER_PACKAGES: list[str] = []

FIRSTBOOT_UNIT = "nestbox-firstboot.service"


# ---------------------------------------------------------------------------------------------
# network
# ---------------------------------------------------------------------------------------------
def render_netplan(recipe: BuildRecipe) -> dict[str, Any]:
    iface = "primary"
    if recipe.network_mode == "dhcp":
        ethernet: dict[str, Any] = {"dhcp4": True, "optional": True}
        if recipe.dns:
            ethernet["nameservers"] = {"addresses": recipe.dns}
        if recipe.dns_search:
            ethernet.setdefault("nameservers", {})["search"] = recipe.dns_search
    else:
        ethernet = {
            "dhcp4": False,
            "addresses": [recipe.address or "192.168.1.10/24"],
            "routes": [{"to": "default", "via": recipe.gateway or "192.168.1.1"}],
            "nameservers": {
                "addresses": recipe.dns or ["1.1.1.1"],
                **({"search": recipe.dns_search} if recipe.dns_search else {}),
            },
        }
    return {"version": 2, "ethernets": {iface: ethernet}}


def render_resolved(recipe: BuildRecipe) -> str:
    lines = ["[Resolve]"]
    if recipe.dns:
        lines.append("DNS=" + " ".join(recipe.dns))
    if recipe.dns_domain:
        lines.append(f"Domains={recipe.dns_domain}")
    if recipe.dns_search:
        lines.append("SearchDomains=" + " ".join(recipe.dns_search))
    return "\n".join(lines) + "\n"


def render_chrony(recipe: BuildRecipe) -> str:
    if not recipe.ntp_enabled:
        return "pool ntp.ubuntu.com iburst maxsources 4\n"
    servers = recipe.ntp_servers or ["ntp.ubuntu.com"]
    return "\n".join(f"server {s} iburst" for s in servers) + "\n"


# ---------------------------------------------------------------------------------------------
# preseed workloads
# ---------------------------------------------------------------------------------------------
def _preseed(recipe: BuildRecipe) -> list[dict[str, Any]]:
    """Normalise preseed nests into a stable, validated shape."""
    out: list[dict[str, Any]] = []
    for item in recipe.preseed_nests or []:
        if not isinstance(item, dict) or not item.get("name"):
            continue
        out.append(
            {
                "name": str(item["name"]),
                "pack": str(item.get("pack", "")),
                "port": int(item.get("port", 8080)),
                "path": str(item.get("path", "/")),
            }
        )
    return out


def _ports(recipe: BuildRecipe) -> list[int]:
    ports = {recipe.ui_port, recipe.admin_port}
    ports.update(p["port"] for p in _preseed(recipe))
    return sorted(ports)


def _port_conflicts(recipe: BuildRecipe) -> list[str]:
    seen: dict[int, str] = {}
    problems: list[str] = []
    # The admin UI is served by the same listener as the UI on the same port; only a genuinely
    # distinct admin port can collide.
    entries = [("ui", recipe.ui_port)]
    if recipe.admin_port != recipe.ui_port:
        entries.append(("admin", recipe.admin_port))
    entries += [(p["name"], p["port"]) for p in _preseed(recipe)]
    for label, port in entries:
        if not 1 <= port <= 65535:
            problems.append(f"{label}: port {port} is out of range")
            continue
        if port in seen:
            problems.append(f"{label} and {seen[port]} both claim port {port}")
        else:
            seen[port] = label
    return problems


def recipe_warnings(recipe: BuildRecipe) -> list[str]:
    warnings = _port_conflicts(recipe)
    if recipe.tls_mode == "letsencrypt" and not recipe.tls_domain:
        warnings.append("Let's Encrypt requires a public FQDN (tls_domain).")
    if recipe.tls_mode == "letsencrypt" and recipe.acme_challenge == "dns01":
        warnings.append("DNS-01 needs a provider API token at first boot; HTTP-01 is simpler.")
    if recipe.tls_mode == "upload" and recipe.certificate_id is None:
        warnings.append("Upload mode selected but no certificate is attached.")
    if not recipe.ssh_authorized_keys and not recipe.admin_password_encrypted:
        warnings.append("Neither an SSH key nor an admin password is set; the box would be unreachable.")
    if recipe.network_mode == "static" and not recipe.gateway:
        warnings.append("Static addressing without a gateway: the box will not reach the internet.")
    return warnings


# ---------------------------------------------------------------------------------------------
# cloud-init / autoinstall
# ---------------------------------------------------------------------------------------------
def _crypt_hash(password: str) -> str:
    """SHA-512 crypt, the format subiquity and cloud-init both accept.

    The hash goes into the image; the plaintext never does.
    """
    if not password:
        return "!"
    try:
        import crypt  # noqa: PLC0415  (deprecated in 3.12, removed in 3.13)

        return crypt.crypt(password, crypt.mksalt(crypt.METHOD_SHA512))
    except Exception:  # noqa: BLE001
        return "!"


def render_autoinstall(recipe: BuildRecipe) -> dict[str, Any]:
    packages = list(dict.fromkeys([*BASE_PACKAGES, *(DOCKER_PACKAGES if recipe.install_docker else []), *recipe.packages]))
    # A failing late-command aborts the whole install, so each one is explicitly non-fatal: a
    # missing unit or an absent optional package must never cost the operator an installation run.
    late = [
        "curtin in-target -- sh -c 'systemctl enable docker 2>/dev/null || true'",
        f"curtin in-target -- sh -c 'systemctl enable {FIRSTBOOT_UNIT} 2>/dev/null || true'",
        "curtin in-target -- sh -c 'timedatectl set-timezone UTC || true'",
    ]
    doc: dict[str, Any] = {
        "version": 1,
        "locale": recipe.locale,
        "keyboard": {"layout": recipe.keyboard},
        "identity": {
            "hostname": recipe.hostname,
            "username": recipe.username,
            # Only ever a SHA-512 crypt hash. The plaintext password never enters the image.
            "password": _crypt_hash(decrypt(recipe.admin_password_encrypted)),
        },
        "ssh": {
            "install-server": True,
            "allow-pw": bool(recipe.admin_password_encrypted),
            "authorized-keys": recipe.ssh_authorized_keys,
        },
        "network": render_netplan(recipe),
        "storage": {"layout": {"name": "direct"}},
        "packages": packages,
        "late-commands": late,
        "shutdown": "reboot",
    }
    return doc


def render_cloud_init(recipe: BuildRecipe) -> dict[str, Any]:
    password = decrypt(recipe.admin_password_encrypted)
    recipe_payload = {
        "version": 1,
        "hostname": recipe.hostname,
        "profile": recipe.profile,
        "ubuntu_series": recipe.ubuntu_series,
        "network": {
            "mode": recipe.network_mode,
            "address": recipe.address,
            "gateway": recipe.gateway,
            "dns": recipe.dns,
            "dns_domain": recipe.dns_domain,
            "dns_search": recipe.dns_search,
        },
        "ntp": {"enabled": recipe.ntp_enabled, "servers": recipe.ntp_servers},
        "ports": {"ui": recipe.ui_port, "admin": recipe.admin_port},
        "tls": {
            "mode": recipe.tls_mode,
            "domain": recipe.tls_domain,
            "email": recipe.tls_email,
            "challenge": recipe.acme_challenge,
        },
        "preseed_nests": _preseed(recipe),
        "install": {
            "docker": recipe.install_docker,
            "openshell": recipe.install_openshell,
        },
    }

    files = [
        {
            "path": "/etc/nestbox/recipe.json",
            "permissions": "0600",
            "content": json.dumps(recipe_payload, indent=2),
        },
        {
            "path": "/etc/systemd/resolved.conf.d/10-nestbox.conf",
            "permissions": "0644",
            "content": render_resolved(recipe),
        },
        {
            "path": "/etc/chrony/conf.d/10-nestbox.conf",
            "permissions": "0644",
            "content": render_chrony(recipe),
        },
        {
            "path": "/etc/netplan/10-nestbox.yaml",
            "permissions": "0600",
            "content": yaml.safe_dump(render_netplan(recipe), sort_keys=False),
        },
    ]

    doc: dict[str, Any] = {
        "hostname": recipe.hostname,
        "manage_etc_hosts": True,
        "timezone": "UTC",
        "locale": recipe.locale,
        "write_files": files,
        "runcmd": [
            ["systemctl", "restart", "systemd-resolved"],
            ["systemctl", "enable", "--now", "chrony"],
            ["systemctl", "daemon-reload"],
            ["systemctl", "enable", "--now", FIRSTBOOT_UNIT],
        ],
    }
    if recipe.force_password_change and password:
        # Expire the password without shipping it again: the hash already lives in autoinstall.
        doc["chpasswd"] = {"expire": True}
    return doc


def render_seed(recipe: BuildRecipe) -> dict[str, str]:
    """NoCloud seed files, ready for /cdrom/nocloud/ on the ISO.

    subiquity reads the autoinstall document from the seed's ``user-data`` under the
    ``autoinstall:`` key. A separate ``autoinstall.yaml`` next to it is not enough: cloud-init
    accepts the user-data, subiquity finds no autoinstall section, and the installer silently
    falls back to the interactive TUI. The target's own cloud-config travels as
    ``autoinstall.user-data``, which subiquity applies to the installed system.
    """
    document = render_autoinstall(recipe)
    document["user-data"] = render_cloud_init(recipe)

    user_data = "#cloud-config\n" + yaml.safe_dump({"autoinstall": document}, sort_keys=False)
    autoinstall = yaml.safe_dump(document, sort_keys=False)
    meta_data = yaml.safe_dump(
        {"instance-id": f"nestbox-{recipe.pk}-{recipe.updated_at:%Y%m%d%H%M%S}", "local-hostname": recipe.hostname},
        sort_keys=False,
    )
    digest = hashlib.sha256(user_data.encode()).hexdigest()
    return {
        "autoinstall.yaml": autoinstall,
        "user-data": user_data,
        "meta-data": meta_data,
        "sha256": digest,
    }


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
    nests = _preseed(recipe)
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
    nests = _preseed(recipe)
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


def render_manifest(recipe: BuildRecipe) -> dict[str, Any]:
    return {
        "name": recipe.name,
        "ubuntu_series": recipe.ubuntu_series,
        "profile": recipe.profile,
        "target": recipe.target,
        "hostname": recipe.hostname,
        "ui_url": recipe.ui_url,
        "ports": _ports(recipe),
        "tls": {"mode": recipe.tls_mode, "domain": recipe.tls_domain, "challenge": recipe.acme_challenge},
        "network": {
            "mode": recipe.network_mode,
            "dns": recipe.dns,
            "dns_domain": recipe.dns_domain,
            "ntp": recipe.ntp_enabled,
        },
        "preseed_nests": _preseed(recipe),
        "warnings": recipe_warnings(recipe),
        "updated_at": recipe.updated_at.isoformat(),
    }


ARTEFACT_RENDERERS = {
    "iso": lambda r: render_seed(r)["autoinstall.yaml"],
    "cloud_init": lambda r: render_seed(r)["user-data"],
    "brev": render_brev_setup,
    "install": render_install_script,
    "proxmox": render_proxmox_plan,
    "manifest": lambda r: json.dumps(render_manifest(r), indent=2),
    "brev_command": lambda r: render_brev_command(r),
}


def render_all(recipe: BuildRecipe) -> dict[str, Any]:
    seed = render_seed(recipe)
    return {
        "seed": seed,
        "install.sh": render_install_script(recipe),
        "brev-setup.sh": render_brev_setup(recipe),
        "brev-command.txt": render_brev_command(recipe),
        "proxmox-plan.sh": render_proxmox_plan(recipe),
        "manifest.json": json.dumps(render_manifest(recipe), indent=2),
        "warnings": recipe_warnings(recipe),
    }
