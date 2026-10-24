"""Unfold dashboard widgets.

Real numbers from the controller, not decorative tiles: a dashboard that lies about the state of
the box is worse than no dashboard. Every module is optional, so each block imports its models
lazily and only when the module is active.
"""
from __future__ import annotations

from django.urls import reverse
from django.utils import timezone

from apps.core.modules import is_active, pending_restart


def _container_cards(cards: list) -> None:
    if not is_active("containers"):
        return
    from apps.containers.models import Container

    cards.append(
        {
            "label": "Containers",
            "value": Container.objects.count(),
            "hint": f"{Container.objects.filter(state='running').count()} running",
            "url": reverse("admin:containers_container_changelist"),
        }
    )


def _sandbox_cards(cards: list) -> None:
    if not is_active("sandboxes"):
        return
    from apps.sandboxes.models import Sandbox

    cards.append(
        {
            "label": "Nests",
            "value": Sandbox.objects.count(),
            "hint": f"{Sandbox.objects.filter(state='running').count()} running",
            "url": reverse("admin:sandboxes_sandbox_changelist"),
        }
    )


def _build_cards(cards: list) -> None:
    if not is_active("builds"):
        return
    from apps.builds.models import BuildJob

    cards.append(
        {
            "label": "Build jobs",
            "value": BuildJob.objects.count(),
            "hint": f"{BuildJob.objects.filter(state='queued').count()} queued",
            "url": reverse("admin:builds_buildjob_changelist"),
        }
    )


def _backup_cards(cards: list) -> None:
    if not is_active("backups"):
        return
    from apps.backups.models import BackupSnapshot

    cards.append(
        {
            "label": "Backup snapshots",
            "value": BackupSnapshot.objects.count(),
            "hint": "Latest snapshots across all targets",
            "url": reverse("admin:backups_backupsnapshot_changelist"),
        }
    )


def _certificate_cards(cards: list, expiring: list) -> None:
    if not is_active("network"):
        return
    from apps.network.models import Certificate

    certificates = list(Certificate.objects.all()[:8])
    expiring.extend(
        cert
        for cert in certificates
        if cert.expires_at and (cert.expires_at - timezone.now()).days <= 30
    )
    cards.append(
        {
            "label": "Certificates",
            "value": Certificate.objects.count(),
            "hint": f"{len(expiring)} expiring within 30 days",
            "url": reverse("admin:network_certificate_changelist"),
        }
    )


def dashboard_callback(request, context):  # noqa: ANN001, ANN201
    """Add live cards to the Unfold dashboard.

    Imports stay inside the helper functions. A disabled module is not installed, so importing
    its models at module import time would break the whole admin on a minimal image.
    """
    cards: list[dict] = []
    expiring: list = []

    _container_cards(cards)
    _sandbox_cards(cards)
    _build_cards(cards)
    _backup_cards(cards)
    _certificate_cards(cards, expiring)

    context.update(
        {
            "nestbox_cards": cards,
            "nestbox_expiring": expiring,
            "nestbox_pending_restart": pending_restart(),
        }
    )
    return context
