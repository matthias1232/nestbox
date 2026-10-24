"""Policy: the AI firewall, stored twice on purpose.

Structured rows drive the form editors. The YAML column holds the exact OpenShell document,
including comments, and is the only thing the adapter ever submits. Policies are never
reconstructed from rows, so what the UI shows and what the gateway enforces cannot drift.
"""
from __future__ import annotations

import hashlib

from django.db import models

POLICY_SCOPES = [
    ("sandbox", "sandbox"),
    ("global", "global"),
]

NETWORK_ACCESS = [
    ("allow", "allow"),
    ("deny", "deny"),
]

NETWORK_ENFORCEMENT = [
    ("enforced", "enforced"),
    ("audit", "audit"),
]

FILESYSTEM_MODES = [
    ("read_only", "read_only"),
    ("read_write", "read_write"),
]

LANDLOCK_COMPATIBILITY = [
    ("best_effort", "best_effort"),
    ("strict", "strict"),
]

#: ``structured`` writes the document from the rule rows; ``raw`` takes the document as written and
#: only derives rows for display. The YAML column is always what the gateway receives.
YAML_SOURCES = [
    ("structured", "structured"),
    ("raw", "raw"),
]


class Policy(models.Model):
    name = models.CharField(max_length=100, unique=True)
    scope = models.CharField(max_length=10, choices=POLICY_SCOPES, default="sandbox")
    sandbox = models.ForeignKey(
        "sandboxes.Sandbox",
        null=True,
        blank=True,
        on_delete=models.CASCADE,
        related_name="policies",
    )
    yaml_source = models.CharField(max_length=12, choices=YAML_SOURCES, default="structured")
    yaml_text = models.TextField(blank=True)
    structured = models.JSONField(default=dict, blank=True)
    version = models.PositiveIntegerField(default=1)
    content_hash = models.CharField(max_length=64, blank=True)
    is_active = models.BooleanField(default=True)
    created_at = models.DateTimeField(auto_now_add=True)
    updated_at = models.DateTimeField(auto_now=True)

    class Meta:
        ordering = ["name"]
        constraints = [
            models.UniqueConstraint(
                fields=["sandbox", "name"], name="uniq_policy_name_per_sandbox"
            ),
        ]

    def __str__(self) -> str:
        return self.name

    def save(self, *args, **kwargs):  # noqa: ANN002, ANN003
        self.content_hash = hashlib.sha256(self.yaml_text.encode("utf-8")).hexdigest()
        super().save(*args, **kwargs)


class PolicyRevision(models.Model):
    policy = models.ForeignKey(Policy, on_delete=models.CASCADE, related_name="revisions")
    version = models.PositiveIntegerField()
    yaml_text = models.TextField()
    author = models.ForeignKey(
        "auth.User", null=True, blank=True, on_delete=models.SET_NULL
    )
    note = models.CharField(max_length=300, blank=True)
    created_at = models.DateTimeField(auto_now_add=True, db_index=True)

    class Meta:
        ordering = ["-created_at"]
        constraints = [
            models.UniqueConstraint(fields=["policy", "version"], name="uniq_revision_version"),
        ]


class NetworkRule(models.Model):
    policy = models.ForeignKey(Policy, on_delete=models.CASCADE, related_name="network_rules")
    block = models.CharField(max_length=120, default="default")
    endpoint_host = models.CharField(max_length=255)
    endpoint_port = models.PositiveIntegerField(default=443)
    protocol = models.CharField(max_length=10, default="rest")
    access = models.CharField(max_length=10, choices=NETWORK_ACCESS, default="allow")
    enforcement = models.CharField(max_length=10, choices=NETWORK_ENFORCEMENT, default="enforced")
    binaries = models.JSONField(default=list, blank=True)
    methods = models.JSONField(default=list, blank=True)
    position = models.PositiveIntegerField(default=0)

    class Meta:
        ordering = ["policy", "block", "position"]
        indexes = [models.Index(fields=["endpoint_host"])]

    def __str__(self) -> str:
        return f"{self.access} {self.endpoint_host}:{self.endpoint_port}"


class FilesystemRule(models.Model):
    policy = models.ForeignKey(Policy, on_delete=models.CASCADE, related_name="filesystem_rules")
    path = models.CharField(max_length=500)
    mode = models.CharField(max_length=20, choices=FILESYSTEM_MODES, default="read_only")

    class Meta:
        ordering = ["policy", "path"]

    def __str__(self) -> str:
        return f"{self.mode} {self.path}"


class ProcessRule(models.Model):
    policy = models.ForeignKey(Policy, on_delete=models.CASCADE, related_name="process_rules")
    run_as_user = models.CharField(max_length=60, blank=True)
    run_as_group = models.CharField(max_length=60, blank=True)

    def __str__(self) -> str:
        return f"process({self.run_as_user})"


class LandlockRule(models.Model):
    """How strictly the sandbox is fenced in; set at nest creation, not hot-reloadable."""

    policy = models.OneToOneField(Policy, on_delete=models.CASCADE, related_name="landlock")
    compatibility = models.CharField(
        max_length=20, choices=LANDLOCK_COMPATIBILITY, default="best_effort"
    )
    include_workdir = models.BooleanField(
        default=True, help_text="Den Arbeitsordner des Agenten mit einschließen."
    )

    def __str__(self) -> str:
        return f"landlock({self.compatibility})"


class PolicyTemplate(models.Model):
    """A starting point for people who do not want to author a policy from nothing."""

    name = models.CharField(max_length=100, unique=True)
    description = models.CharField(max_length=300, blank=True)
    document = models.JSONField(
        default=dict, blank=True, help_text="Vollständiges Policy-Dokument als JSON."
    )
    is_builtin = models.BooleanField(default=False)
    created_at = models.DateTimeField(auto_now_add=True)

    class Meta:
        ordering = ["name"]

    def __str__(self) -> str:
        return self.name
