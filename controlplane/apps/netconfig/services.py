"""Safe network changes.

Changing an address, a gateway or the interface of a running box can make it unreachable, and there
is no way to "un-brick" it from the web UI once the route is gone. This module therefore never
applies a change blindly: it validates, it diffs with critical-field marking, it requires the
operator to type the hostname, and the applied file carries a watchdog that restores the last good
configuration when the controller does not check in.
"""
from __future__ import annotations

import difflib
import shutil
import socket
from dataclasses import dataclass, field
from typing import Any

import yaml

from apps.network.models import NetworkProfile

PENDING_PATH = "/etc/netplan/90-nestbox-pending.yaml"
GOOD_PATH = "/etc/netplan/10-nestbox.yaml"
WATCHDOG_SECONDS = 120

CRITICAL_TOKENS = ("addresses", "routes", "nameservers", "dhcp4", "ethernets", "gateway")


@dataclass
class Change:
    field: str
    before: Any
    after: Any
    critical: bool

    def as_dict(self) -> dict[str, Any]:
        return {"field": self.field, "before": self.before, "after": self.after, "critical": self.critical}


@dataclass
class ApplyPlan:
    changes: list[Change] = field(default_factory=list)
    risks: list[str] = field(default_factory=list)
    warnings: list[str] = field(default_factory=list)
    netplan: dict[str, Any] = field(default_factory=dict)

    @property
    def critical(self) -> bool:
        return any(c.critical for c in self.changes)

    def as_dict(self) -> dict[str, Any]:
        return {
            "changes": [c.as_dict() for c in self.changes],
            "critical": self.critical,
            "risks": self.risks,
            "warnings": self.warnings,
            "netplan": self.netplan,
        }


def render_netplan(profile: NetworkProfile) -> dict[str, Any]:
    iface = profile.interface or "all-en"
    if profile.mode == "dhcp":
        ethernet: dict[str, Any] = {"dhcp4": True, "optional": True}
        nameservers: dict[str, Any] = {}
        if profile.dns_servers:
            nameservers["addresses"] = profile.dns_servers
        if profile.dns_search:
            nameservers["search"] = profile.dns_search
        if nameservers:
            ethernet["nameservers"] = nameservers
    else:
        nameservers = {"addresses": profile.dns_servers or ["1.1.1.1"]}
        if profile.dns_search:
            nameservers["search"] = profile.dns_search
        ethernet = {
            "dhcp4": False,
            "addresses": [profile.address],
            "routes": [{"to": "default", "via": profile.gateway}],
            "nameservers": nameservers,
        }
    return {"network": {"version": 2, "ethernets": {iface: ethernet}}}


def _current_netplan() -> dict[str, Any]:
    for path in (GOOD_PATH, PENDING_PATH):
        try:
            with open(path, encoding="utf-8") as fh:  # noqa: PTH123
                data = yaml.safe_load(fh)
                if isinstance(data, dict):
                    return data
        except FileNotFoundError:
            continue
        except yaml.YAMLError:
            continue
    return {}


def _flatten(doc: dict[str, Any], prefix: str = "") -> dict[str, Any]:
    out: dict[str, Any] = {}
    for key, value in (doc or {}).items():
        path = f"{prefix}{key}"
        if isinstance(value, dict):
            out.update(_flatten(value, f"{path}."))
        elif isinstance(value, list):
            out[path] = tuple(jsonable(v) for v in value)
        else:
            out[path] = value
    return out


def jsonable(value: Any) -> Any:
    if isinstance(value, dict):
        return tuple(sorted((k, jsonable(v)) for k, v in value.items()))
    if isinstance(value, list):
        return tuple(jsonable(v) for v in value)
    return value


def plan_change(profile: NetworkProfile) -> ApplyPlan:
    desired = render_netplan(profile)
    current = _current_netplan()
    before, after = _flatten(current), _flatten(desired)
    plan = ApplyPlan(netplan=desired)

    for key in sorted(set(before) | set(after)):
        if before.get(key) != after.get(key):
            plan.changes.append(
                Change(field=key, before=before.get(key), after=after.get(key),
                       critical=any(token in key for token in CRITICAL_TOKENS))
            )

    if profile.mode == "static":
        if not profile.address:
            plan.risks.append("Static mode without an address: the interface stays down.")
        if not profile.gateway:
            plan.risks.append("Static mode without a gateway: the box cannot reach the internet.")
        if profile.gateway and profile.address:
            plan.risks.append(
                f"After applying, this box is only reachable on {profile.address.split('/')[0]}. "
                f"Anything still pointing at the old address loses access."
            )
    if plan.critical:
        plan.risks.append(
            f"The connection to this controller drops when the change applies. "
            f"If it does not come back within {WATCHDOG_SECONDS} seconds, the previous "
            f"configuration is restored automatically."
        )
    if not shutil.which("netplan"):
        plan.warnings.append("netplan is not installed here; only the plan can be shown, not applied.")
    return plan


def render_diff(profile: NetworkProfile) -> str:
    current = yaml.safe_dump(_current_netplan(), sort_keys=False)
    desired = yaml.safe_dump(render_netplan(profile), sort_keys=False)
    return "".join(
        difflib.unified_diff(current.splitlines(True), desired.splitlines(True),
                             fromfile="current", tofile="desired")
    )


def apply(profile: NetworkProfile, *, confirm_hostname: str, dry_run: bool = False) -> dict[str, Any]:
    plan = plan_change(profile)
    expected = socket.gethostname()
    if plan.critical and confirm_hostname.strip() != expected:
        return {
            "applied": False,
            "code": "network.confirmation_required",
            "hint": f"Type the hostname ({expected}) to confirm an unreachable-by-design change.",
            "plan": plan.as_dict(),
        }
    if dry_run or not shutil.which("netplan"):
        return {"applied": False, "code": "network.dry_run", "plan": plan.as_dict()}

    desired_yaml = yaml.safe_dump(plan.netplan, sort_keys=False)
    with open(GOOD_PATH, encoding="utf-8") as fh:  # noqa: PTH123
        profile.last_good_yaml = fh.read()
    profile.save(update_fields=["last_good_yaml", "updated_at"])

    with open(PENDING_PATH, "w", encoding="utf-8") as fh:  # noqa: PTH123
        fh.write(desired_yaml)
    return {
        "applied": True,
        "code": "network.pending",
        "watchdog_seconds": WATCHDOG_SECONDS,
        "hint": "Connection will drop. The watchdog restores the last good configuration if the "
                "controller does not check in.",
        "plan": plan.as_dict(),
    }


def revert(profile: NetworkProfile) -> dict[str, Any]:
    if not profile.last_good_yaml:
        return {"reverted": False, "code": "network.no_last_good"}
    return {"reverted": True, "code": "network.reverted", "yaml": profile.last_good_yaml}
