"""First-run bootstrap.

Idempotent: safe to run on every container start. Creates the role groups, the first admin (from
environment or a generated password printed once), the local node, the built-in templates, a
starting policy and a default build recipe.
"""
from __future__ import annotations

import secrets

from django.conf import settings
from django.contrib.auth import get_user_model
from django.contrib.auth.models import Group
from django.core.management.base import BaseCommand
from django.db import transaction

from apps.builds.models import BuildRecipe
from apps.core.models import Node
from apps.identity.models import ROLES, Profile
from apps.policies.models import Policy
from apps.sandboxes.models import SandboxTemplate

DEFAULT_POLICY = """\
filesystem_policy:
  include_workdir: true
  read_only: ["/usr", "/lib", "/lib64", "/etc/ssl/certs"]
  read_write: ["/sandbox", "/tmp"]

landlock:
  compatibility: best_effort

process:
  run_as_user: sandbox
  run_as_group: sandbox

network_policies:
  model-api:
    endpoints:
      - host: api.anthropic.com
        port: 443
        protocol: rest
        enforcement: enforced
        access: allow
    binaries:
      - path: /usr/local/bin/claude
"""

TEMPLATES = [
    ("Claude Code", "claude-code", "claude-code", 2.0, 4096, 0, "coding-agent"),
    ("Hermes Agent", "hermes-agent", "hermes-agent", 2.0, 4096, 0, "coding-agent"),
    ("OpenCode", "opencode", "opencode", 2.0, 4096, 0, "coding-agent"),
    ("Local Nemotron", "local-nemotron", "local-nemotron", 8.0, 32768, 1, "model-server"),
]


class Command(BaseCommand):
    help = "Create roles, the first admin, default templates, policy and build recipe."

    @transaction.atomic
    def handle(self, *args, **options):  # noqa: ANN002, ANN003
        for role in ROLES:
            Group.objects.get_or_create(name=role)
        self.stdout.write(self.style.SUCCESS(f"roles: {', '.join(ROLES)}"))

        User = get_user_model()
        email = settings.NESTBOX_BOOTSTRAP_ADMIN_EMAIL
        password = settings.NESTBOX_BOOTSTRAP_ADMIN_PASSWORD
        generated = False
        if email:
            user = User.objects.filter(email=email).first()
            if user is None:
                if not password:
                    password = secrets.token_urlsafe(16)
                    generated = True
                user = User.objects.create_superuser(
                    username=email.split("@")[0], email=email, password=password
                )
                user.groups.add(Group.objects.get(name="admin"))
                self.stdout.write(self.style.SUCCESS(f"admin created: {email}"))
                if generated:
                    self.stdout.write(self.style.WARNING(f"generated password: {password}"))
            else:
                self.stdout.write("admin exists, password unchanged")
            Profile.objects.get_or_create(user=user)
        else:
            self.stdout.write(
                self.style.WARNING(
                    "NESTBOX_BOOTSTRAP_ADMIN_EMAIL is not set; no admin was created. "
                    "Run `manage.py createsuperuser` or set the variable and re-run."
                )
            )

        for user in User.objects.all():
            Profile.objects.get_or_create(user=user)

        node, _ = Node.objects.get_or_create(name="local", defaults={"is_local": True})
        self.stdout.write(f"node: {node.name}")

        for name, slug, pack_key, cpu, memory, gpu, profile in TEMPLATES:
            SandboxTemplate.objects.update_or_create(
                slug=slug,
                defaults={
                    "name": name,
                    "agent_pack_key": pack_key,
                    "cpu": cpu,
                    "memory_mb": memory,
                    "gpu": gpu,
                    "policy_profile": profile,
                    "is_builtin": True,
                },
            )
        self.stdout.write(f"templates: {SandboxTemplate.objects.count()}")

        Policy.objects.get_or_create(
            name="coding-agent-default",
            defaults={"scope": "sandbox", "yaml_text": DEFAULT_POLICY, "structured": {}, "is_active": True},
        )

        BuildRecipe.objects.get_or_create(
            name="default-slim",
            defaults={
                "ubuntu_series": "24.04",
                "profile": "slim",
                "network_mode": "dhcp",
                "hostname": "nestbox",
                "username": "nestbox",
                "dns": ["1.1.1.1", "9.9.9.9"],
            },
        )
        self.stdout.write(self.style.SUCCESS("bootstrap complete"))
