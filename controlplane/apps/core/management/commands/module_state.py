"""Inspect and drive the module switches from the command line or first boot."""
from __future__ import annotations

from config.modules import active_keys, state_file
from django.core.management.base import BaseCommand, CommandError

from apps.core import modules as runtime


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
            help="With --init: overwrite an existing state file from the environment",
        )

    def handle(self, *args, **options):  # noqa: ANN002, ANN003
        if options["init"]:
            if state_file().exists() and not options["force"]:
                self.stdout.write(f"state file present: {state_file()}")
            else:
                runtime.write_state(runtime.desired_keys() or list(active_keys()))
                self.stdout.write(f"state file written: {state_file()}")
            runtime.mark_applied()

        if options["set"]:
            wishes: dict[str, bool] = {}
            for raw in options["set"]:
                if "=" in raw:
                    key, _, value = raw.partition("=")
                    if value.strip().lower() not in {"on", "off", "true", "false", "1", "0", "yes", "no"}:
                        raise CommandError(f"cannot read {raw!r}; use module=on or module=off")
                    wishes[key.strip()] = value.strip().lower() in {"on", "true", "1", "yes"}
                else:
                    wishes[raw.strip()] = True
            for key, enabled in wishes.items():
                if key not in runtime.catalog_dict():
                    raise CommandError(f"unknown module: {key}")
                runtime.set_enabled(key, enabled)
            self.stdout.write("updated, " + (
                "restart required" if runtime.pending_restart() else "already active"
            ))

        if options["list"] or not (options["init"] or options["set"]):
            active = set(runtime.active())
            for entry in runtime.catalog():
                mark = "on " if entry.key in active else "off"
                star = "" if entry.toggleable else "*"
                self.stdout.write(f"  [{mark}]{star} {entry.key:<14} {entry.title}")
            self.stdout.write(self.style.SUCCESS(f"active: {', '.join(sorted(active))}"))
            self.stdout.write(f"state file: {state_file()}")
