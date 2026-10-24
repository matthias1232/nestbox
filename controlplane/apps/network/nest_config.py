from django import forms

from apps.core.nest_sections import NestSection

SECTIONS = [NestSection(
    key="network", label="Network", help_text="Desired hostname and exposure. A running gateway must apply these settings.",
    fields={
        "hostname": forms.CharField(required=False, max_length=253, help_text="DNS name for this nest."),
        "public": forms.BooleanField(required=False, help_text="Request public exposure; does not open a port by itself."),
    },
)]
