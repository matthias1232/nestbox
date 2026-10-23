"""UI for nests."""
from __future__ import annotations

import json
import time

from django.contrib import messages
from django.contrib.auth.decorators import login_required
from django.http import StreamingHttpResponse
from django.shortcuts import get_object_or_404, redirect, render
from django.views.decorators.http import require_GET

from apps.agentpacks.models import AgentPack
from apps.api.errors import NestboxError
from apps.sandboxes.models import Sandbox, SandboxTemplate
from apps.sandboxes.services import create_sandbox, stop_sandbox


@login_required
def sandbox_list(request):  # noqa: ANN001
    return render(
        request,
        "sandboxes/list.html",
        {
            "sandboxes": Sandbox.objects.select_related("template").order_by("name"),
            "packs": AgentPack.objects.all(),
        },
    )


@login_required
def sandbox_create(request):  # noqa: ANN001
    packs = AgentPack.objects.all()
    templates = SandboxTemplate.objects.all()
    if request.method == "POST":
        name = (request.POST.get("name") or "").strip()
        template_id = request.POST.get("template") or None
        pack_key = request.POST.get("agent_pack_key") or ""
        template = SandboxTemplate.objects.filter(pk=template_id).first() if template_id else None
        try:
            sandbox = create_sandbox(
                user=request.user,
                name=name,
                template=template,
                agent_pack_key=pack_key,
            )
        except NestboxError as exc:
            messages.error(request, f"{exc.code}: {exc.message}")
            return render(
                request,
                "sandboxes/form.html",
                {"packs": packs, "templates": templates, "form": request.POST},
            )
        messages.success(request, f"Nest {sandbox.name} created.")
        return redirect("sandbox-detail", name=sandbox.name)
    return render(request, "sandboxes/form.html", {"packs": packs, "templates": templates})


@login_required
def sandbox_detail(request, name: str):  # noqa: ANN001
    sandbox = get_object_or_404(Sandbox, name=name)
    if request.method == "POST":
        action = request.POST.get("action")
        if action == "stop":
            stop_sandbox(name)
            messages.success(request, f"Nest {name} stopped.")
        elif action == "delete":
            sandbox.delete()
            messages.success(request, f"Nest {name} deleted.")
            return redirect("sandboxes")
        return redirect("sandbox-detail", name=name)
    return render(
        request,
        "sandboxes/detail.html",
        {
            "sandbox": sandbox,
            "policies": sandbox.policies.all(),
            "pack": AgentPack.objects.filter(key=sandbox.agent_pack_key).first(),
        },
    )


@require_GET
@login_required
def sandbox_stream(request):  # noqa: ANN001
    def event_stream():
        last = ""
        for _ in range(150):
            payload = list(Sandbox.objects.values("name", "state", "agent_pack_key"))
            snapshot = json.dumps(payload, sort_keys=True)
            if snapshot != last:
                last = snapshot
                yield f"event: sandboxes\ndata: {snapshot}\n\n"
            else:
                yield ": keep-alive\n\n"
            time.sleep(3)

    response = StreamingHttpResponse(event_stream(), content_type="text/event-stream")
    response["Cache-Control"] = "no-cache"
    response["X-Accel-Buffering"] = "no"
    return response
