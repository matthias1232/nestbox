"""Policy editor UI."""
from __future__ import annotations

from django.contrib import messages
from django.contrib.auth.decorators import login_required
from django.shortcuts import get_object_or_404, redirect, render

from apps.policies.models import Policy
from apps.policies.services import validate_policy_yaml

SAMPLE = """\
filesystem_policy:
  include_workdir: true
  read_only: ["/usr", "/lib", "/etc/ssl/certs"]
  read_write: ["/sandbox"]

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
        binaries: [{path: /usr/local/bin/claude}]
    binaries: [{path: /usr/local/bin/claude}]
"""


@login_required
def policy_list(request):  # noqa: ANN001
    return render(
        request,
        "policies/list.html",
        {"policies": Policy.objects.select_related("sandbox").order_by("name")},
    )


@login_required
def policy_detail(request, pk: int):  # noqa: ANN001
    policy = get_object_or_404(Policy, pk=pk)
    validation = None
    if request.method == "POST":
        if request.POST.get("action") == "validate":
            validation = validate_policy_yaml(request.POST.get("yaml_text", ""))
        else:
            policy.revisions.create(version=policy.version, yaml_text=policy.yaml_text)
            policy.yaml_text = request.POST.get("yaml_text", policy.yaml_text)
            policy.version += 1
            policy.save()
            messages.success(request, f"Policy saved as v{policy.version}.")
            return redirect("policy-detail", pk=policy.pk)
    return render(
        request,
        "policies/detail.html",
        {
            "policy": policy,
            "yaml_text": request.POST.get("yaml_text", policy.yaml_text) or SAMPLE,
            "validation": validation,
            "revisions": policy.revisions.all()[:10],
        },
    )
