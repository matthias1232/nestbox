"""Server-rendered UI for the appliance."""
from __future__ import annotations

import json
import time

from django.contrib.auth.decorators import login_required
from django.http import StreamingHttpResponse
from django.shortcuts import get_object_or_404, render
from django.views.decorators.http import require_GET

from apps.api.errors import RuntimeUnavailable
from apps.core.models import Container
from apps.core.runtime import runtime
from apps.identity.models import AuditEvent
from apps.sandboxes.models import Sandbox


@login_required
def dashboard(request):  # noqa: ANN001
    containers = Container.objects.select_related("node").order_by("role", "name")
    if not containers.exists() and runtime.available():
        try:
            runtime.sync()
            containers = Container.objects.select_related("node").order_by("role", "name")
        except RuntimeUnavailable:
            pass

    runtime_ok = runtime.available()
    stats = {
        "containers": containers.count(),
        "running": containers.filter(state="running").count(),
        "stopped": containers.exclude(state="running").count(),
        "sandboxes": Sandbox.objects.count(),
        "sandboxes_running": Sandbox.objects.filter(state="running").count(),
    }
    return render(
        request,
        "core/dashboard.html",
        {
            "containers": containers,
            "stats": stats,
            "runtime_ok": runtime_ok,
            "recent_events": AuditEvent.objects.select_related("actor")[:8],
        },
    )


@login_required
def container_list(request):  # noqa: ANN001
    containers = Container.objects.select_related("node").order_by("role", "name")
    return render(
        request,
        "core/containers.html",
        {"containers": containers, "runtime_ok": runtime.available()},
    )


@login_required
def container_detail(request, name: str):  # noqa: ANN001
    container = get_object_or_404(Container, name=name)
    log_excerpt = ""
    try:
        client = runtime.client()
        log_excerpt = client.containers.get(name).logs(tail=120, timestamps=True).decode(
            "utf-8", errors="replace"
        )
    except Exception as exc:  # noqa: BLE001
        log_excerpt = f"Logs unavailable: {exc}"
    return render(
        request,
        "core/container_detail.html",
        {"container": container, "log_excerpt": log_excerpt},
    )


@require_GET
@login_required
def container_stream(request):  # noqa: ANN001
    """SSE: emit the full container inventory whenever a state changes."""

    def event_stream():
        last = ""
        for _ in range(150):
            payload = list(
                Container.objects.values("name", "state", "role", "health", "image")
            )
            snapshot = json.dumps(payload, sort_keys=True)
            if snapshot != last:
                last = snapshot
                yield f"event: containers\ndata: {snapshot}\n\n"
            else:
                yield ": keep-alive\n\n"
            time.sleep(2)

    response = StreamingHttpResponse(event_stream(), content_type="text/event-stream")
    response["Cache-Control"] = "no-cache"
    response["X-Accel-Buffering"] = "no"
    return response
