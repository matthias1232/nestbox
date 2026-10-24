"""Desired policy association for a nest.

The section is intentionally small: it records *which* policy a nest should use. It does not claim
that the gateway has received the policy — application and its result live in the policies API.

The field is an integer, not a database-backed choice built at import time. ``NestSection.fields`` is
evaluated while modules load, where querying ``Policy`` would be wrong (and would fail during
migrations). The existence check runs when the dynamic form validates, on a live database.
"""
from __future__ import annotations

from django import forms
from django.core.exceptions import ValidationError

from apps.core.nest_sections import NestSection


class ExistingPolicyField(forms.IntegerField):
    """A primary key that must point at a policy that still exists.

    Keeping this an integer means the value stored in ``Sandbox.spec`` is JSON-friendly and does
    not become a stale model instance if the admin form is reused.
    """

    default_error_messages = {
        "unknown": "Select an existing policy. Create the policy first, then choose it here.",
    }

    def __init__(self, *args, **kwargs):  # noqa: ANN002, ANN003
        kwargs.setdefault("min_value", 1)
        kwargs.setdefault("required", False)
        super().__init__(*args, **kwargs)

    def validate(self, value):  # noqa: ANN001
        super().validate(value)
        if value in self.empty_values:
            return
        # Imported lazily so importing this module never touches the app registry or the database.
        from apps.policies.models import Policy

        if not Policy.objects.filter(pk=value).exists():
            raise ValidationError(self.error_messages["unknown"], code="policy_missing")


SECTIONS = [
    NestSection(
        key="policy",
        label="Policy",
        help_text=(
            "Choose the policy document this nest should use. This is desired state: saving it "
            "does not prove that a running gateway has applied the policy. Apply it from the "
            "policy page and check the result there."
        ),
        fields={
            "policy": ExistingPolicyField(
                required=False,
                help_text=(
                    "Primary key of an existing policy. Leave empty to let the template or the "
                    "global default decide; changes are only a request until applied."
                ),
            ),
        },
    ),
]
