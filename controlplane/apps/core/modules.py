"""Runtime view of the module catalog.

``config.modules`` answers "what could run"; this module answers "what runs right now", records what
the operator wants, and writes the state file that the next process start reads.
"""
from __future__ import annotations

from config.modules import CATALOG, ModuleSpec, active_keys, resolve, state_file, write_state
from django.conf import settings

from apps.core.models import ModuleState


def catalog_dict() -> dict[str, ModuleSpec]:
    return CATALOG


def catalog() -> list[ModuleSpec]:
    return sorted(CATALOG.values(), key=lambda spec: (spec.order, spec.key))


def active() -> set[str]:
    return set(settings.NESTBOX_ACTIVE_MODULES)


def is_active(key: str) -> bool:
    return key in active()


def spec(key: str) -> ModuleSpec | None:
    return CATALOG.get(key)


def ensure_states() -> None:
    """Create a row for every catalog entry, so the admin can render switches for all of them."""
    current = active()
    for entry in catalog():
        ModuleState.objects.get_or_create(
            key=entry.key, defaults={"enabled": entry.key in current}
        )


def desired_keys() -> list[str]:
    """What the operator wants: rows if they exist, otherwise the active set."""
    rows = {row.key: row.enabled for row in ModuleState.objects.all()}
    if not rows:
        return sorted(active())
    return sorted(key for key, enabled in rows.items() if enabled)


def set_enabled(key: str, enabled: bool) -> tuple[str, ...]:
    """Record a wish, persist the effective set and flag the restart it needs."""
    if key not in CATALOG:
        raise KeyError(key)
    entry = CATALOG[key]
    if not entry.toggleable:
        raise ValueError(f"{key} cannot be switched")
    ensure_states()
    state, _ = ModuleState.objects.get_or_create(key=key)
    state.enabled = enabled
    state.save(update_fields=["enabled", "updated_at"])

    effective = write_state(desired_keys())
    differs = set(effective) != active()
    ModuleState.objects.update(pending_restart=differs)
    return effective


def pending_restart() -> bool:
    return ModuleState.objects.filter(pending_restart=True).exists()


def mark_applied() -> None:
    ModuleState.objects.filter(pending_restart=True).update(pending_restart=False)


def init_state_file() -> tuple[str, ...]:
    """Seed the state file from the environment on first boot (idempotent)."""
    if state_file().exists():
        return active_keys()
    return write_state(desired_keys() or list(active()))


def effective_for(keys: list[str] | tuple[str, ...]) -> tuple[str, ...]:
    return resolve(keys)
