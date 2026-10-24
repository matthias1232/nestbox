from django import forms

from apps.core.nest_sections import NestSection

SECTIONS = [NestSection(
    key="openshell", label="Sandbox resources", help_text="Resource requests for the gateway; availability depends on the host.",
    fields={
        "cpu": forms.IntegerField(min_value=1, max_value=256, required=False),
        "memory_mb": forms.IntegerField(min_value=256, max_value=1048576, required=False),
        "gpu": forms.IntegerField(min_value=0, max_value=16, required=False),
    },
)]
