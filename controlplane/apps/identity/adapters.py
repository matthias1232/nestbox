"""Account adapter: registration is closed on an appliance."""
from __future__ import annotations

from allauth.account.adapter import DefaultAccountAdapter


class ClosedSignupAdapter(DefaultAccountAdapter):
    """Only invited users exist. There is no public signup on a private appliance."""

    def is_open_for_signup(self, request):  # noqa: ANN001
        return False
