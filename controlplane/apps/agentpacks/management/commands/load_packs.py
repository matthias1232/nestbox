from django.core.management.base import BaseCommand

from apps.agentpacks.registry import load_packs


class Command(BaseCommand):
    help = "Load or refresh agent packs from packs/*.yaml"

    def handle(self, *args, **options):  # noqa: ANN002, ANN003
        created, updated, problems = load_packs()
        self.stdout.write(self.style.SUCCESS(f"packs: {created} created, {updated} updated"))
        for problem in problems:
            self.stdout.write(self.style.WARNING(f"  {problem}"))
