"""Runtime view of the module catalog.

``config.modules`` answers "what could run"; this module answers "what runs right now", records what
the operator wants, and writes the state file that the next process start reads.

The running set is frozen when this module is imported, because that is exactly when Django reads
``NESTBOX_ACTIVE_MODULES`` and installs the apps. Everything the operator changes afterwards is a
*desired* set: it is stored in :class:`~apps.core.models.ModuleState` and in the state file, and it
takes effect on the next start. Comparing the two is what "restart required" means.
"""
from __future__ import annotations

from config.modules import (
    CATALOG,
    DependencyError,
    ModuleNotToggleable,
    ModuleSpec,
    UnknownModule,
    dependents,
    resolve,
    state_file,
    validate_activation,
    write_state,
)
from django.conf import settings
from django.db import transaction

from apps.core.models import ModuleState

#: Snapshot of the apps this process actually installed. Never recompute from settings: an
#: override after startup would make the UI claim a restart was already applied.
_RUNNING: frozenset[str] = frozenset(settings.NESTBOX_ACTIVE_MODULES)


def catalog_dict() -> dict[str, ModuleSpec]:
    return CATALOG


def catalog() -> list[ModuleSpec]:
    return sorted(CATALOG.values(), key=lambda spec: (spec.order, spec.key))


def running() -> frozenset[str]:
    """The module set this process booted with (frozen at import)."""
    return _RUNNING


def active() -> set[str]:
    return set(_RUNNING)


def is_active(key: str) -> bool:
    return key in _RUNNING


def spec(key: str) -> ModuleSpec | None:
    return CATALOG.get(key)


def _rows() -> dict[str, bool]:
    return {
        row.key: row.enabled
        for row in ModuleState.objects.filter(key__in=list(CATALOG))
    }


def ensure_states() -> None:
    """Create a row for every catalog entry, so the admin can render switches for all of them.

    Existing rows are left alone: they carry the operator's desired state, which may legitimately
    differ from the running set until the next restart.
    """
    current = running()
    for entry in catalog():
        ModuleState.objects.get_or_create(
            key=entry.key, defaults={"enabled": entry.key in current}
        )


def desired_keys() -> tuple[str, ...]:
    """What the operator wants, as a complete and resolved set.

    Rows are authoritative. Modules that predate the table (or a partially seeded table) fall
    back to the running set, so an upgrade never silently disables a module merely because its
    row was never written.
    """
    rows = _rows()
    if not rows:
        return resolve(_RUNNING)

    enabled = {key for key, is_enabled in rows.items() if is_enabled}
    enabled.update(key for key in _RUNNING if key not in rows)
    return resolve(enabled)


def _record(effective: tuple[str, ...]) -> None:
    """Make the rows match ``effective`` and flag every real difference for a restart."""
    wanted = set(effective)
    rows = {row.key: row for row in ModuleState.objects.all()}

    for entry in catalog():
        enabled = entry.key in wanted
        pending = enabled != (entry.key in _RUNNING)
        row = rows.get(entry.key)
        if row is None:
            ModuleState.objects.create(
                key=entry.key, enabled=enabled, pending_restart=pending
            )
        elif row.enabled != enabled or row.pending_restart != pending:
            row.enabled = enabled
            row.pending_restart = pending
            row.save(update_fields=["enabled", "pending_restart", "updated_at"])

    # Rows for modules the image no longer carries are kept (data is not ours to delete), but
    # they must not keep the controller in a permanent "restart required" state.
    ModuleState.objects.exclude(key__in=list(CATALOG)).update(pending_restart=False)


def replace_desired(keys: list[str] | tuple[str, ...]) -> tuple[str, ...]:
    """Write a complete desired set and its state file in one transaction."""
    effective = validate_activation(resolve(keys, strict=True))
    with transaction.atomic():
        _record(effective)
        write_state(effective)
    return effective


def apply_changes(changes: dict[str, bool]) -> tuple[str, ...]:
    """Validate and persist several switches as one change.

    The request is resolved before anything is written, so a batch that would contradict itself
    (``A=off B=on`` while ``B`` requires ``A``) fails without a partial state file.
    """
    if not changes:
        return desired_keys()

    for key in changes:
        if key not in CATALOG:
            raise UnknownModule(f"unknown module: {key!r}")
        if not CATALOG[key].toggleable:
            raise ModuleNotToggleable(f"module {key!r} is always on and cannot be switched")

    current = set(desired_keys())
    for key, enabled in changes.items():
        if enabled:
            current.add(key)
        else:
            current.discard(key)

    proposed = resolve(current, strict=True)
    for key, enabled in changes.items():
        if not enabled and key in proposed:
            blocking = [name for name in dependents(key) if name in proposed]
            raise DependencyError(
                f"cannot disable {key!r}: still required by "
                + ", ".join(repr(name) for name in blocking)
            )

    return replace_desired(proposed)


def set_enabled(key: str, enabled: bool) -> tuple[str, ...]:
    """Record one wish and persist it for the next start.

    Enabling closes the dependency graph and records the requirements as enabled too. Disabling
    a module that an enabled module still requires is refused: silently switching the requirement
    back on would contradict the operator's request and make the UI lie.
    """
    return apply_changes({key: enabled})


def pending_restart() -> bool:
    """A restart is due when the desired set differs from this boot, or a row still says so.

    The row flag matters in a fresh management-command process: its settings already read the
    new state file, so comparing desired with *its* running set would wrongly look applied while
    the long-running controller still serves the old modules.
    """
    if ModuleState.objects.filter(pending_restart=True).exists():
        return True
    return set(desired_keys()) != set(_RUNNING)


def mark_applied() -> None:
    """Clear the restart flags, but only when the desired set really is running.

    Called from first boot after the state file is seeded. If the database carries an older wish
    (for example after restoring a backup), the flags are recomputed instead of cleared.
    """
    desired = desired_keys()
    if set(desired) == set(_RUNNING):
        ModuleState.objects.update(pending_restart=False)
    else:
        _record(desired)


def init_state_file() -> tuple[str, ...]:
    """Seed the state file on first boot without overwriting an existing contract.

    * A present file wins; settings have already read it, so it is the running set.
    * Without a file, recorded wishes are written (an upgrade keeps what the operator chose).
    * Without rows either, this boot's environment seed is the wish.
    """
    if state_file().exists():
        return resolve(active())

    has_rows = ModuleState.objects.exists()
    keys = desired_keys() if has_rows else tuple(_RUNNING)
    return write_state(keys)


def effective_for(keys: list[str] | tuple[str, ...]) -> tuple[str, ...]:
    return resolve(keys)
