"""The module catalog: what this image can do, and which parts are switched on.

This module is deliberately free of Django imports: settings import it before the app registry
exists, to decide which apps are installed at all. The catalog is baked into the image (code is
the truth for *availability*), while the enabled set comes from
``/var/lib/nestbox/active-modules.json`` — written by first boot and by the admin — or, when that
file does not exist yet, from ``NESTBOX_MODULES``.

Precedence: state file > ``NESTBOX_MODULES`` > every module in the catalog.
"""
from __future__ import annotations

import json
import os
from dataclasses import dataclass, field
from pathlib import Path


@dataclass(frozen=True)
class ModuleSpec:
    key: str
    app: str
    title: str
    description: str
    category: str = "Core"
    icon: str = "extension"
    requires: tuple[str, ...] = ()
    toggleable: bool = True
    order: int = 100
    api_prefix: str = ""
    extra: dict = field(default_factory=dict)


CATALOG: dict[str, ModuleSpec] = {
    "core": ModuleSpec(
        key="core",
        app="apps.core",
        title="Kern",
        description="Auth, Knoten, Health, Modul-Registry und die Komposition der API.",
        category="Core",
        icon="hub",
        toggleable=False,
        order=0,
    ),
    "api": ModuleSpec(
        key="api",
        app="apps.api",
        title="API-Infrastruktur",
        description="DRF-Grundlagen: Auth, Rollen, Fehlerformat, Schema und die API-Dokumentation.",
        category="Core",
        icon="api",
        requires=("core",),
        toggleable=False,
        order=1,
        api_prefix="",
    ),
    "identity": ModuleSpec(
        key="identity",
        app="apps.identity",
        title="Identität",
        description="Benutzer, Rollen, Profile und das Audit-Log.",
        category="Core",
        icon="badge",
        requires=("core",),
        order=10,
        api_prefix="identity",
    ),
    "containers": ModuleSpec(
        key="containers",
        app="apps.containers",
        title="Container",
        description="Container und Dienste, wie sie die Laufzeit meldet, samt Aktionen.",
        category="Betrieb",
        icon="dns",
        requires=("core",),
        order=20,
        api_prefix="containers",
    ),
    "sandboxes": ModuleSpec(
        key="sandboxes",
        app="apps.sandboxes",
        title="Nester",
        description="Sandboxen, Vorlagen und die Konfiguration, die ein Nest mitbekommt.",
        category="Betrieb",
        icon="grid_view",
        requires=("core", "containers"),
        order=30,
        api_prefix="sandboxes",
    ),
    "agentpacks": ModuleSpec(
        key="agentpacks",
        app="apps.agentpacks",
        title="Agent-Packs",
        description="Die mitgelieferten Agenten (Hermes, OpenClaw, Coding-Agent …) als Katalog.",
        category="Betrieb",
        icon="deployed_code",
        requires=("core",),
        order=40,
        api_prefix="agent-packs",
    ),
    "openshell": ModuleSpec(
        key="openshell",
        app="apps.openshell",
        title="OpenShell",
        description="Gateways, Workspace-Zuordnungen und die Anbindung an die Sandbox-Laufzeit.",
        category="Betrieb",
        icon="shield",
        requires=("core",),
        order=45,
        api_prefix="openshell",
    ),
    "policies": ModuleSpec(
        key="policies",
        app="apps.policies",
        title="Policies",
        description="Die KI-Firewall: Netzwerk, Dateisystem, Prozesse und Landlock als Regeln.",
        category="Sicherheit",
        icon="policy",
        requires=("sandboxes",),
        order=50,
        api_prefix="policies",
    ),
    "network": ModuleSpec(
        key="network",
        app="apps.network",
        title="Netzwerk",
        description="Zertifikate, DNS-Einträge, Proxy-Routen und Overlay-Peers.",
        category="Infrastruktur",
        icon="lan",
        requires=("core",),
        order=60,
        api_prefix="network",
    ),
    "tls": ModuleSpec(
        key="tls",
        app="apps.tls",
        title="TLS",
        description="Interne CA, Zertifikatslebenszyklus und die ACME-Anbindung.",
        category="Infrastruktur",
        icon="lock",
        requires=("network",),
        order=61,
        api_prefix="tls",
    ),
    "netconfig": ModuleSpec(
        key="netconfig",
        app="apps.netconfig",
        title="Netzkonfiguration",
        description="Netzwerkänderungen prüfen und sicher anwenden (netplan, DNS, NTP).",
        category="Infrastruktur",
        icon="settings_ethernet",
        requires=("core",),
        order=62,
        api_prefix="netconfig",
    ),
    "builds": ModuleSpec(
        key="builds",
        app="apps.builds",
        title="ISO-Builds",
        description="Rezepte und Jobs, die ein installierbares ISO erzeugen — und nur das.",
        category="Lieferkette",
        icon="album",
        requires=("core", "sandboxes", "network"),
        order=70,
        api_prefix="builds",
    ),
    "provisioning": ModuleSpec(
        key="provisioning",
        app="apps.provisioning",
        title="Provisioning",
        description="Cloud-init, Brev-Setup und der Proxmox-Plan zu einem Rezept.",
        category="Lieferkette",
        icon="rocket_launch",
        requires=("builds",),
        order=71,
        api_prefix="provisioning",
    ),
    "backups": ModuleSpec(
        key="backups",
        app="apps.backups",
        title="Backups",
        description="Ziele, Snapshots und Wiederherstellungen der Appliance.",
        category="Betrieb",
        icon="backup",
        requires=("core",),
        order=80,
        api_prefix="backups",
    ),
}

#: Installed regardless of the state file: without them there is no request handling at all.
ALWAYS_ON: tuple[str, ...] = ("core", "api")

DEFAULT_STATE_DIR = "/var/lib/nestbox"


def state_dir() -> Path:
    return Path(os.environ.get("NESTBOX_STATE_DIR", DEFAULT_STATE_DIR))


def state_file() -> Path:
    return state_dir() / "active-modules.json"


def _normalise(raw: str) -> list[str]:
    if raw.strip().lower() in {"all", "*"}:
        return list(CATALOG)
    return [part.strip() for part in raw.split(",") if part.strip()]


def _from_env() -> list[str] | None:
    raw = os.environ.get("NESTBOX_MODULES", "")
    return _normalise(raw) if raw.strip() else None


def _from_file() -> list[str] | None:
    try:
        data = json.loads(state_file().read_text(encoding="utf-8"))
    except (OSError, ValueError):
        return None
    if isinstance(data, dict):
        data = data.get("modules")
    if isinstance(data, list):
        return [str(item) for item in data]
    return None


def resolve(keys: list[str] | tuple[str, ...]) -> tuple[str, ...]:
    """Add always-on modules and close the dependency graph over the given keys."""
    wanted = {key for key in keys if key in CATALOG} | set(ALWAYS_ON)
    changed = True
    while changed:
        changed = False
        for key in tuple(wanted):
            for dependency in CATALOG[key].requires:
                if dependency not in wanted:
                    wanted.add(dependency)
                    changed = True
    return tuple(sorted(wanted, key=lambda key: (CATALOG[key].order, key)))


def requested_keys() -> list[str]:
    """The modules somebody asked for, before dependency resolution."""
    return _from_file() or _from_env() or list(CATALOG)


def active_keys() -> tuple[str, ...]:
    """The modules this process runs with."""
    return resolve(requested_keys())


def write_state(keys: list[str] | tuple[str, ...]) -> tuple[str, ...]:
    """Persist the effective set; returns it after resolution."""
    effective = resolve(keys)
    directory = state_dir()
    directory.mkdir(parents=True, exist_ok=True)
    target = state_file()
    temporary = target.with_suffix(".json.tmp")
    temporary.write_text(
        json.dumps({"modules": list(effective)}, indent=2) + "\n", encoding="utf-8"
    )
    temporary.replace(target)
    return effective


def is_active(key: str) -> bool:
    return key in active_keys()
