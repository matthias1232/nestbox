"""Render a recipe's artefacts to disk, for CI and for the ISO builder."""
from __future__ import annotations

import json
from pathlib import Path

from django.core.management.base import BaseCommand, CommandError

from apps.builds.models import BuildRecipe
from apps.builds.services import recipe_warnings, render_all, render_seed


class Command(BaseCommand):
    help = "Render every artefact of a recipe into a directory."

    def add_arguments(self, parser):  # noqa: ANN001
        parser.add_argument("recipe", help="Recipe name")
        parser.add_argument("--out", required=True, help="Output directory")
        parser.add_argument("--seed-only", action="store_true", help="Write only the NoCloud seed")

    def handle(self, *args, **options):  # noqa: ANN002, ANN003
        recipe = BuildRecipe.objects.filter(name=options["recipe"]).first()
        if recipe is None:
            raise CommandError(f"Unknown recipe: {options['recipe']}")
        out = Path(options["out"])
        seed_dir = out / "seed"
        seed_dir.mkdir(parents=True, exist_ok=True)

        seed = render_seed(recipe)
        for name in ("autoinstall.yaml", "user-data", "meta-data"):
            (seed_dir / name).write_text(seed[name], encoding="utf-8")

        warnings = recipe_warnings(recipe)
        if not options["seed_only"]:
            artefacts = render_all(recipe)
            for name in ("install.sh", "brev-setup.sh", "brev-command.txt", "proxmox-plan.sh", "manifest.json"):
                (out / name).write_text(artefacts[name], encoding="utf-8")
            (out / "warnings.json").write_text(json.dumps(warnings, indent=2), encoding="utf-8")

        self.stdout.write(self.style.SUCCESS(f"rendered {recipe.name} -> {out}"))
        for warning in warnings:
            self.stdout.write(self.style.WARNING(f"  warning: {warning}"))
