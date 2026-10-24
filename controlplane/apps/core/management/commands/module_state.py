"""Inspect and drive the module switches from the command line or first boot."""
from __future__ import annotations

from config import modules as catalog
from config.modules import ModuleError, state_file
from django.core.management.base import BaseCommand, CommandError

from apps.core import modules as runtime


def _as_bool(raw: str) -> bool:
    value = raw.strip().lower()
    if value in {"on", "true", "1", "yes"}:
        return True
    if value in {"off", "false", "0", "no"}:
        return False
    raise CommandError(f"cannot read {raw!r}; use MODULE=on or MODULE=off")


class Command(BaseCommand):
    help = "Show, set or initialise the active modules, and write the state file."

    def add_arguments(self, parser):  # noqa: ANN001
        parser.add_argument("--list", action="store_true", help="Print catalog and state")
        parser.add_argument(
            "--init", action="store_true",
            help="Seed the state file from the environment (idempotent, for first boot)",
        )
        parser.add_argument(
            "--set", action="append", metavar="MODULE[=on|=off]",
            help="Switch a module, e.g. --set policies=off (repeatable)",
        )
        parser.add_argument(
            "--force", action="store_true",
            help=(
                "With --init: rewrite the state file from NESTBOX_MODULES "
                "(every module when it is unset)"
            ),
        )

    def handle(self, *args, **options):  # noqa: ANN002, ANN003
        if options["init"]:
            if options["force"]:
                effective = runtime.replace_desired(catalog.seed_keys())
                self.stdout.write(f"state file written: {state_file()}")
            else:
                present = state_file().exists()
                effective = runtime.init_state_file()
                label = "state file present" if present else "state file written"
                self.stdout.write(f"{label}: {state_file()}")

            runtime.mark_applied()
            if runtime.pending_restart():
                # The database carried a different wish than the set this process booted with;
                # keep the flags, because clearing them would claim an unapplied change is live.
                self.stdout.write(
                    self.style.WARNING(
                        "restart required: the desired modules differ from the running ones"
                    )
                )
            else:
                self.stdout.write(
                    self.style.SUCCESS("modules applied: " + ", ".join(effective))
                )

        if options["set"]:
            changes: dict[str, bool] = {}
            for raw in options["set"]:
                key, separator, value = raw.partition("=")
                key = key.strip()
                if not key:
                    raise CommandError(f"cannot read {raw!r}; use MODULE=on or MODULE=off")
                changes[key] = _as_bool(value) if separator else True

            try:
                runtime.apply_changes(changes)
            except ModuleError as error:
                raise CommandError(str(error)) from error

            self.stdout.write(
                "updated, "
                + ("restart required" if runtime.pending_restart() else "already active")
            )

        if options["list"] or not (options["init"] or options["set"]):
            active = set(runtime.active())
            desired = set(runtime.desired_keys())
            for entry in runtime.catalog():
                mark = "on " if entry.key in active else "off"
                star = "" if entry.toggleable else "*"
                changed = (entry.key in desired) != (entry.key in active)
                pending = " (restart required)" if changed else ""
                self.stdout.write(f"  [{mark}]{star} {entry.key:<14} {entry.title}{pending}")
            self.stdout.write(self.style.SUCCESS(f"running: {', '.join(sorted(active))}"))
            self.stdout.write(f"desired: {', '.join(sorted(desired))}")
            restart = "required" if runtime.pending_restart() else "not required"
            self.stdout.write(f"restart: {restart}")
            self.stdout.write(f"state file: {state_file()}")
