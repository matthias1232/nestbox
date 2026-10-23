"""One error envelope for the whole product.

REST, SSE and WebSocket all emit ``{"error": {"code", "message", "details", "request_id"}}``.
The ``code`` is the stable contract; the ``message`` is an English developer string that the UI
never displays verbatim (it translates the code instead).
"""
from __future__ import annotations

import uuid

from rest_framework import exceptions, status
from rest_framework.response import Response
from rest_framework.views import exception_handler as drf_exception_handler


class NestboxError(Exception):
    """Domain error with a stable code."""

    default_code = "internal_error"
    default_status = status.HTTP_400_BAD_REQUEST

    def __init__(self, message: str, *, code: str | None = None, details: dict | None = None):
        super().__init__(message)
        self.code = code or self.default_code
        self.message = message
        self.details = details or {}


class RuntimeUnavailable(NestboxError):
    default_code = "runtime.unavailable"
    default_status = status.HTTP_503_SERVICE_UNAVAILABLE


class NotFound(NestboxError):
    default_code = "not_found"
    default_status = status.HTTP_404_NOT_FOUND


class Conflict(NestboxError):
    default_code = "conflict"
    default_status = status.HTTP_409_CONFLICT


_CODE_BY_EXCEPTION = [
    (exceptions.ValidationError, "validation_error"),
    (exceptions.NotAuthenticated, "not_authenticated"),
    (exceptions.AuthenticationFailed, "authentication_failed"),
    (exceptions.PermissionDenied, "permission_denied"),
    (exceptions.NotFound, "not_found"),
    (exceptions.MethodNotAllowed, "method_not_allowed"),
    (exceptions.Throttled, "throttled"),
    (exceptions.UnsupportedMediaType, "unsupported_media_type"),
]


def _code_for(exc: Exception) -> str:
    if isinstance(exc, NestboxError):
        return exc.code
    for klass, code in _CODE_BY_EXCEPTION:
        if isinstance(exc, klass):
            return code
    return "internal_error"


def _payload(code: str, message: str, details: dict, request) -> dict:  # noqa: ANN001
    request_id = getattr(request, "request_id", None) or uuid.uuid4()
    return {
        "error": {
            "code": code,
            "message": message,
            "details": details or {},
            "request_id": str(request_id),
        }
    }


def exception_handler(exc, context):  # noqa: ANN001
    request = context.get("request")
    if isinstance(exc, NestboxError):
        return Response(
            _payload(exc.code, exc.message, exc.details, request),
            status=exc.default_status,
        )

    response = drf_exception_handler(exc, context)
    if response is None:
        return Response(
            _payload("internal_error", "An unexpected error occurred.", {}, request),
            status=status.HTTP_500_INTERNAL_SERVER_ERROR,
        )

    detail = response.data
    if isinstance(detail, dict) and "detail" in detail and len(detail) == 1:
        message = str(detail["detail"])
        details: dict = {}
    elif isinstance(detail, dict):
        message = "Request validation failed."
        details = detail
    else:
        message = str(detail)
        details = {}

    response.data = _payload(_code_for(exc), message, details, request)
    return response
