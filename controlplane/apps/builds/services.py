"""Deterministic rendering of a build recipe into cloud-init input.

This is the reviewable, testable half of the ISO pipeline: a recipe becomes an autoinstall
document and a NoCloud seed. The baking step consumes exactly this output.
"""
from __future__ import annotations

import hashlib
import json
from typing import Any

import yaml

from apps.builds.models import BuildRecipe

DEFAULT_PACKAGES = [
    "docker.io",
    "wireguard-tools",
    "openssh-server",
    "ca-certificates",
    "curl",
    "jq",
]


def _network_section(recipe: BuildRecipe) -> dict[str, Any]:
    if recipe.network_mode == "dhcp":
        return {
            "version": 2,
            "ethernets": {
                "primary": {"dhcp4": True, "optional": True},
            },
        }
    return {
        "version": 2,
        "ethernets": {
            "primary": {
                "dhcp4": False,
                "addresses": [recipe.address or "192.168.1.10/24"],
                "routes": [{"to": "default", "via": recipe.gateway or "192.168.1.1"}],
                "nameservers": {"addresses": recipe.dns or ["1.1.1.1"]},
            }
        },
    }


def render_autoinstall(recipe: BuildRecipe) -> dict[str, Any]:
    """Render the subiquity autoinstall document (``/autoinstall.yaml`` on the ISO)."""
    packages = list(dict.fromkeys([*DEFAULT_PACKAGES, *recipe.packages]))
    return {
        "version": 1,
        "locale": recipe.locale,
        "keyboard": {"layout": recipe.keyboard},
        "identity": {
            "hostname": recipe.hostname,
            "username": recipe.username,
            "password": "$6$rounds=4096$nestbox$replace-with-a-hashed-password",
        },
        "ssh": {
            "install-server": True,
            "allow-pw": False,
            "authorized-keys": recipe.ssh_authorized_keys,
        },
        "network": _network_section(recipe),
        "storage": {"layout": {"name": "direct"}},
        "packages": packages,
        "late-commands": [
            "curtin in-target -- systemctl enable docker",
            "curtin in-target -- systemctl enable nestbox-firstboot",
        ],
        "shutdown": "reboot",
    }


def render_cloud_init(recipe: BuildRecipe) -> dict[str, Any]:
    """Render the cloud-init ``user-data`` that drives first boot."""
    return {
        "hostname": recipe.hostname,
        "manage_etc_hosts": True,
        "timezone": "UTC",
        "locale": recipe.locale,
        "write_files": [
            {
                "path": "/etc/nestbox/recipe.json",
                "permissions": "0600",
                "content": json.dumps(
                    {
                        "profile": recipe.profile,
                        "network_mode": recipe.network_mode,
                        "tls": recipe.tls,
                        "proxy": recipe.proxy,
                        "preseed_sandbox": recipe.preseed_sandbox_id,
                    },
                    indent=2,
                ),
            }
        ],
        "runcmd": [
            ["systemctl", "daemon-reload"],
            ["systemctl", "enable", "--now", "nestbox-firstboot.service"],
        ],
    }


def render_seed(recipe: BuildRecipe) -> dict[str, str]:
    """The NoCloud seed files, ready to be written to ``/cdrom/nocloud/``."""
    autoinstall = yaml.safe_dump(render_autoinstall(recipe), sort_keys=False)
    user_data = "#cloud-config\n" + yaml.safe_dump(render_cloud_init(recipe), sort_keys=False)
    meta_data = yaml.safe_dump(
        {"instance-id": f"nestbox-{recipe.pk}", "local-hostname": recipe.hostname},
        sort_keys=False,
    )
    digest = hashlib.sha256((autoinstall + user_data).encode()).hexdigest()
    return {
        "autoinstall.yaml": autoinstall,
        "user-data": user_data,
        "meta-data": meta_data,
        "sha256": digest,
    }
