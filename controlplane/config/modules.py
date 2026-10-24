"""The module catalog: what this image can do, and which parts are switched on.

This module is deliberately free of Django imports: settings import it before the app registry
exists, to decide which apps are installed at all. The catalog is baked into the image (code is
the truth for *availability*), while the enabled set comes from
``/var/lib/nestbox/active-modules.json`` — written by first boot and by the admin — or, when that
file does not exist yet, from ``NESTBOX_MODULES``.

Precedence: state file > ``NESTBOX_MODULES`` > every module in the catalog.
"""
from __future__ import annotations

import contextlib
import json
import os
import tempfile
from dataclasses import dataclass, field
from pathlib import Path


class ModuleError(ValueError):
    """A module request cannot be satisfied."""


class UnknownModule(ModuleError):
    """A key is not part of the catalog that was baked into this image."""


class ModuleNotToggleable(ModuleError):
    """The module is always on, or otherwise fixed by the image."""


class DependencyError(ModuleError):
    """Enabling or disabling a module would leave the dependency graph inconsistent."""


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
        title="Core",
        description="Authentication, nodes, health, the module registry and API composition.",
        category="Core",
        icon="hub",
        toggleable=False,
        order=0,
    ),
    "api": ModuleSpec(
        key="api",
        app="apps.api",
        title="API infrastructure",
        description="DRF foundations: auth, roles, error format, schema and API documentation.",
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
        title="Identity",
        description="Users, roles, profiles and the audit log.",
        category="Core",
        icon="badge",
        requires=("core",),
        toggleable=False,
        order=10,
        api_prefix="identity",
    ),
    "containers": ModuleSpec(
        key="containers",
        app="apps.containers",
        title="Container",
        description="Containers and services as the runtime reports them, including actions.",
        category="Operations",
        icon="dns",
        requires=("core",),
        order=20,
        api_prefix="containers",
    ),
    "sandboxes": ModuleSpec(
        key="sandboxes",
        app="apps.sandboxes",
        title="Nests",
        description="Sandboxes, templates and the configuration a nest receives.",
        category="Operations",
        icon="grid_view",
        requires=("core", "containers"),
        order=30,
        api_prefix="sandboxes",
    ),
    "agentpacks": ModuleSpec(
        key="agentpacks",
        app="apps.agentpacks",
        title="Agent packs",
        description="The bundled agents (Hermes, OpenClaw, coding agent, ...) as a catalog.",
        category="Operations",
        icon="deployed_code",
        requires=("core",),
        order=40,
        api_prefix="agent-packs",
    ),
    "openshell": ModuleSpec(
        key="openshell",
        app="apps.openshell",
        title="OpenShell",
        description="Gateways, workspace assignments and the binding to the sandbox runtime.",
        category="Operations",
        icon="shield",
        requires=("core",),
        order=45,
        api_prefix="openshell",
    ),
    "policies": ModuleSpec(
        key="policies",
        app="apps.policies",
        title="Policies",
        description="The AI firewall: network, filesystem, processes and Landlock as rules.",
        category="Security",
        icon="policy",
        requires=("sandboxes",),
        order=50,
        api_prefix="policies",
    ),
    "network": ModuleSpec(
        key="network",
        app="apps.network",
        title="Network",
        description="Certificates, DNS records, proxy routes and overlay peers.",
        category="Infrastructure",
        icon="lan",
        requires=("core",),
        order=60,
        api_prefix="network",
    ),
    "tls": ModuleSpec(
        key="tls",
        app="apps.tls",
        title="TLS",
        description="Internal CA, certificate lifecycle and the ACME integration.",
        category="Infrastructure",
        icon="lock",
        requires=("network",),
        order=61,
        api_prefix="tls",
    ),
    "netconfig": ModuleSpec(
        key="netconfig",
        app="apps.netconfig",
        title="Network configuration",
        description="Validate and safely apply network changes (netplan, DNS, NTP).",
        category="Infrastructure",
        icon="settings_ethernet",
        requires=("core",),
        order=62,
        api_prefix="netconfig",
    ),
    "builds": ModuleSpec(
        key="builds",
        app="apps.builds",
        title="ISO builds",
        description="Recipes and jobs that produce an installable ISO - and nothing else.",
        category="Supply chain",
        icon="album",
        requires=("core", "sandboxes", "network"),
        order=70,
        api_prefix="builds",
    ),
    "provisioning": ModuleSpec(
        key="provisioning",
        app="apps.provisioning",
        title="Provisioning",
        description="Cloud-init, Brev setup and the Proxmox plan for a recipe.",
        category="Supply chain",
        icon="rocket_launch",
        requires=("builds",),
        order=71,
        api_prefix="provisioning",
    ),
    "backups": ModuleSpec(
        key="backups",
        app="apps.backups",
        title="Backups",
        description="Targets, snapshots and restores of the appliance.",
        category="Operations",
        icon="backup",
        requires=("core",),
        order=80,
        api_prefix="backups",
    ),
}

#: Installed regardless of the state file: without them there is no request handling at all.
ALWAYS_ON: tuple[str, ...] = ("core", "api", "identity")

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


def resolve(
    keys: list[str] | tuple[str, ...], *, strict: bool = False
) -> tuple[str, ...]:
    """Add always-on modules and close the dependency graph over the given keys.

    A state file can outlive the image that wrote it, so unknown keys are ignored by default:
    dropping a module from the catalog must not stop the controller from booting. Activation
    paths pass ``strict=True`` and get an :class:`UnknownModule` instead.
    """
    unknown = sorted(set(keys) - set(CATALOG))
    if unknown and strict:
        raise UnknownModule("unknown module(s): " + ", ".join(unknown))

    wanted = {key for key in keys if key in CATALOG} | set(ALWAYS_ON)
    pending = list(wanted)
    while pending:
        key = pending.pop()
        if key not in CATALOG:
            raise DependencyError(f"module {key!r} is required but not in the catalog")
        for dependency in CATALOG[key].requires:
            if dependency not in wanted:
                wanted.add(dependency)
                pending.append(dependency)

    return tuple(sorted(wanted, key=lambda key: (CATALOG[key].order, key)))


def _order(keys: set[str]) -> tuple[str, ...]:
    return tuple(sorted(keys, key=lambda key: (CATALOG[key].order, key)))


def validate_activation(keys: list[str] | tuple[str, ...]) -> tuple[str, ...]:
    """Check a complete, already resolved set before it is written anywhere.

    Unknown modules and missing requirements are errors here: unlike startup, an operator
    action must not silently change the meaning of the request.
    """
    wanted = set(keys)
    unknown = sorted(wanted - set(CATALOG))
    if unknown:
        raise UnknownModule("unknown module(s): " + ", ".join(unknown))

    missing_always_on = [key for key in ALWAYS_ON if key not in wanted]
    if missing_always_on:
        raise DependencyError(
            "always-on module(s) must stay enabled: " + ", ".join(missing_always_on)
        )

    for key in _order(wanted):
        missing = [dependency for dependency in CATALOG[key].requires if dependency not in wanted]
        if missing:
            raise DependencyError(
                f"module {key!r} requires " + ", ".join(repr(dep) for dep in missing)
            )
    return _order(wanted)


def dependents(key: str) -> tuple[str, ...]:
    """Modules that name ``key`` as a direct requirement."""
    return _order({entry.key for entry in CATALOG.values() if key in entry.requires})


def requested_keys() -> list[str]:
    """The modules somebody asked for, before dependency resolution."""
    stored = _from_file()
    if stored is not None:
        return stored
    seeded = _from_env()
    if seeded is not None:
        return seeded
    return list(CATALOG)


def seed_keys() -> tuple[str, ...]:
    """What first boot uses when no state file exists: ``NESTBOX_MODULES`` or everything."""
    seeded = _from_env()
    return resolve(seeded if seeded is not None else list(CATALOG))


def active_keys() -> tuple[str, ...]:
    """The modules this process runs with."""
    return resolve(requested_keys())


def write_state(keys: list[str] | tuple[str, ...]) -> tuple[str, ...]:
    """Persist the effective set atomically; returns it after resolution.

    The state file is the boot contract, so a half-written file is worse than no change at
    all: write a unique temporary file in the same directory, fsync it, then ``os.replace``.
    A reader either sees the old file or the new one, never a truncated mixture.
    """
    effective = resolve(keys)
    directory = state_dir()
    directory.mkdir(parents=True, exist_ok=True)
    target = state_file()
    payload = json.dumps({"version": 1, "modules": list(effective)}, indent=2) + "\n"

    descriptor, temporary = tempfile.mkstemp(
        dir=directory, prefix=f".{target.name}.", suffix=".tmp"
    )
    try:
        with os.fdopen(descriptor, "w", encoding="utf-8") as handle:
            handle.write(payload)
            handle.flush()
            os.fsync(handle.fileno())
        os.chmod(temporary, 0o644)
        os.replace(temporary, target)
    except BaseException:
        with contextlib.suppress(OSError):
            os.unlink(temporary)
        raise

    # Make the rename durable too. Not every filesystem supports fsync on a directory.
    try:
        directory_fd = os.open(directory, os.O_RDONLY)
    except OSError:
        directory_fd = None
    if directory_fd is not None:
        try:
            os.fsync(directory_fd)
        except OSError:
            pass
        finally:
            os.close(directory_fd)

    return effective


def is_active(key: str) -> bool:
    return key in active_keys()
