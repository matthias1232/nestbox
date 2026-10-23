"""Unfold dashboard widgets.

Real numbers from the controller, not decorative tiles: a dashboard that lies about the state of
the box is worse than no dashboard.
"""
from __future__ import annotations

from django.utils import timezone

from apps.backups.models import BackupSnapshot
from apps.builds.models import BuildJob
from apps.containers.models import Container
from apps.identity.models import AuditEvent
from apps.network.models import Certificate
from apps.sandboxes.models import Sandbox


def dashboard_callback(request, context):  # noqa: ANN001, ARG001
    certificates = list(Certificate.objects.all()[:5])
    expiring = [c for c in certificates if c.expires_at and (c.expires_at - timezone.now()).days <= 30]
    context.update(
        {
            "nestbox_cards": [
                {"label": "Containers", "value": Container.objects.count(),
                 "hint": f"{Container.objects.filter(state='running').count()} running"},
                {"label": "Nests", "value": Sandbox.objects.count(),
                 "hint": f"{Sandbox.objects.filter(state='running').count()} running"},
                {"label": "Builds", "value": BuildJob.objects.count(),
                 "hint": f"{BuildJob.objects.filter(state='queued').count()} queued"},
                {"label": "Certificates", "value": Certificate.objects.count(),
                 "hint": f"{len(expiring)} expiring soon"},
                {"label": "Snapshots", "value": BackupSnapshot.objects.count(), "hint": "restic"},
            ],
            "nestbox_expiring": expiring,
            "nestbox_recent": AuditEvent.objects.select_related("actor")[:8],
            "nestbox_builds": BuildJob.objects.select_related("recipe")[:5],
        }
    )
    return context
