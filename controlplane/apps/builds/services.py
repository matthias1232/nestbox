"""Deterministic rendering of a recipe into an Ubuntu autoinstall ISO and its NoCloud seed.

Deployment artefacts are not built here: the installer, Brev setup and Proxmox plan live in
``apps.provisioning`` and depend on this module, never the other way around.
"""
from __future__ import annotations

import hashlib
import json
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
    # Ubuntu server releases do not promise a stable NIC name (ens18, enp1s0,
    # eth0, ...).  A literal ``primary`` interface works in neither a fresh
    # VM nor bare metal.  Match the first conventional Ethernet device and
    # give it a deterministic config ID instead.
    iface = "all-ethernet"
    if recipe.network_mode == "dhcp":
        ethernet: dict[str, Any] = {"match": {"name": "en*"}, "dhcp4": True, "optional": True}
        if recipe.dns:
            ethernet["nameservers"] = {"addresses": recipe.dns}
        if recipe.dns_search:
            ethernet.setdefault("nameservers", {})["search"] = recipe.dns_search
    else:
        ethernet = {
            "match": {"name": "en*"},
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
def preseed_nests(recipe: BuildRecipe) -> list[dict[str, Any]]:
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
    ports.update(p["port"] for p in preseed_nests(recipe))
    return sorted(ports)


def _port_conflicts(recipe: BuildRecipe) -> list[str]:
    seen: dict[int, str] = {}
    problems: list[str] = []
    # The admin UI is served by the same listener as the UI on the same port; only a genuinely
    # distinct admin port can collide.
    entries = [("ui", recipe.ui_port)]
    if recipe.admin_port != recipe.ui_port:
        entries.append(("admin", recipe.admin_port))
    entries += [(p["name"], p["port"]) for p in preseed_nests(recipe)]
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
        "preseed_nests": preseed_nests(recipe),
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
        "preseed_nests": preseed_nests(recipe),
        "warnings": recipe_warnings(recipe),
        "updated_at": recipe.updated_at.isoformat(),
    }


ARTEFACT_RENDERERS = {
    "iso": lambda r: render_seed(r)["autoinstall.yaml"],
    "cloud_init": lambda r: render_seed(r)["user-data"],
    "manifest": lambda r: json.dumps(render_manifest(r), indent=2),
}


def render_all(recipe: BuildRecipe) -> dict[str, Any]:
    """Everything an ISO build consumes: the NoCloud seed and its manifest.

    Deployment artefacts (installer, Brev, Proxmox) live in ``apps.provisioning``; this module
    deliberately renders only what goes onto or next to the image.
    """
    return {
        "seed": render_seed(recipe),
        "manifest.json": json.dumps(render_manifest(recipe), indent=2),
        "warnings": recipe_warnings(recipe),
    }
