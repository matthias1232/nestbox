"""ISO build UI."""
from __future__ import annotations

from django.contrib import messages
from django.contrib.auth.decorators import login_required
from django.shortcuts import get_object_or_404, redirect, render

from apps.builds.models import BuildJob, BuildRecipe
from apps.builds.services import render_seed


@login_required
def build_list(request):  # noqa: ANN001
    if request.method == "POST":
        recipe = BuildRecipe.objects.filter(pk=request.POST.get("recipe")).first()
        if recipe:
            job = BuildJob.objects.create(recipe=recipe, state="queued")
            messages.success(request, f"Build #{job.pk} queued for {recipe.name}.")
        return redirect("builds")
    return render(
        request,
        "builds/list.html",
        {
            "recipes": BuildRecipe.objects.all(),
            "jobs": BuildJob.objects.select_related("recipe")[:25],
        },
    )


@login_required
def build_detail(request, pk: int):  # noqa: ANN001
    job = get_object_or_404(BuildJob, pk=pk)
    seed = render_seed(job.recipe)
    return render(request, "builds/detail.html", {"job": job, "seed": seed})
