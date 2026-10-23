"""Create the nests an image recipe preseeded.

The ISO (and the Brev setup script) carry ``/etc/nestbox/recipe.json``; the controller mounts that
directory read-only. Turning the recipe into rows here means the dashboard already lists the nests
with their ports on the first sign-in, instead of the operator retyping what the wizard collected.
"""
from __future__ import annotations

import json
import os
from pathlib import Path

from django.core.management.base import BaseCommand
from django.utils.text import slugify

from apps.sandboxes.models import Sandbox, SandboxTemplate


class Command(BaseCommand):
    help = "Create the nests listed in the recipe's preseed_nests."

    def add_arguments(self, parser):  # noqa: ANN001
        parser.add_argument(
            "--recipe",
            default=os.environ.get("NESTBOX_RECIPE", "/etc/nestbox/recipe.json"),
            help="Path to recipe.json (default: /etc/nestbox/recipe.json)",
        )

    def handle(self, *args, **options):  # noqa: ANN002, ANN003
        path = Path(options["recipe"])
        if not path.is_file():
            self.stdout.write(f"no recipe at {path}, nothing to preseed")
            return

        try:
            recipe = json.loads(path.read_text(encoding="utf-8"))
        except json.JSONDecodeError as exc:
            self.stderr.write(f"recipe is not valid JSON: {exc}")
            return

        nests = recipe.get("preseed_nests") or []
        if not nests:
            self.stdout.write("recipe lists no preseeded nests")
            return

        for item in nests:
            name = slugify(item.get("name") or "")
            if not name:
                self.stderr.write("skipping a preseeded nest without a name")
                continue
            pack = (item.get("pack") or "").strip()
            try:
                port = int(item.get("port") or 0)
            except (TypeError, ValueError):
                port = 0
            template = SandboxTemplate.objects.filter(agent_pack_key=pack).first() if pack else None

            sandbox, created = Sandbox.objects.get_or_create(
                name=name,
                defaults={
                    "agent_pack_key": pack,
                    "template": template,
                    "port": port,
                    "state": "pending",
                },
            )
            if created:
                self.stdout.write(self.style.SUCCESS(f"nest {name} on port {port or 'auto'}"))
                continue

            changed = []
            if port and sandbox.port != port:
                sandbox.port = port
                changed.append("port")
            if pack and sandbox.agent_pack_key != pack:
                sandbox.agent_pack_key = pack
                changed.append("agent_pack_key")
            if template and sandbox.template_id != template.id:
                sandbox.template = template
                changed.append("template")
            if changed:
                sandbox.save(update_fields=changed)
                self.stdout.write(f"nest {name} updated: {', '.join(changed)}")
            else:
                self.stdout.write(f"nest {name} already present")
