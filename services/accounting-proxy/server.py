"""Nestbox accounting proxy — agent model traffic, accounted and privacy-checked.

The agent talks to this service as if it were the model provider; this service
talks to the provider. Every request is inspected before it leaves, and every
answer is turned into one metadata row. Two facts make the design what it is:

* **Credentials are forwarded, never stored.** The client's own `Authorization`
  (or `x-api-key`) header is passed on untouched. The proxy never logs in, never
  mints a token, and holds no provider key of its own.
* **Content is never kept.** Only a SHA-256 digest of the request body, the
  usage counters, the status, and the latency leave this process. Prompts and
  completions are not written to a row, not logged, and not sent anywhere except
  the upstream the agent chose.

The three pieces, in the order they happen:

1. **Routing** picks an upstream from the path (`/v1/messages` is Anthropic,
   `/v1/chat/completions` is OpenAI, `/v1beta/models/...` is Gemini-style) and
   lets an `X-Nestbox-Upstream` header override the choice outright.
2. **Inspection** asks the control API (`POST /api/v1/internal/privacy/inspect`)
   about the request text. A refusal answers `403` and the request is not
   forwarded; a rewrite is written back into the JSON body before forwarding.
3. **Accounting** posts one row to `POST /api/v1/internal/accounting/record`
   after the client has its answer.

A body that is not inspectable JSON (binary uploads, pre-encoded streams) is
forwarded unchanged and marked `not_inspectable` — the honest label, because
"we did not look" and "we looked and it was clean" are different facts.

Standard library only, for the same reason as `services/tool-images/mcp-hub`:
this image has no package manager step, so a registry outage or a hijacked
dependency cannot change what ships.
"""
from __future__ import annotations

import hashlib
import json
import os
import time
from dataclasses import dataclass
from http.client import HTTPConnection, HTTPSConnection
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from typing import Any
from urllib.parse import urlsplit

# Path families. A path that is in none of them still goes to the default
# upstream, because an agent may use a provider we have not heard of yet.
ANTHROPIC_PATHS = frozenset({"/v1/messages", "/v1/complete"})
OPENAI_PATHS = frozenset(
    {"/v1/chat/completions", "/v1/responses", "/v1/embeddings"}
)
GEMINI_PREFIX = "/v1beta/models/"

INSPECT_PATH = "/api/v1/internal/privacy/inspect"
ACCOUNT_PATH = "/api/v1/internal/accounting/record"

UPSTREAM_HEADER = "x-nestbox-upstream"

# Headers that belong to one hop and must not be copied across either way.
HOP_BY_HOP = frozenset(
    {
        "connection",
        "keep-alive",
        "proxy-authenticate",
        "proxy-authorization",
        "te",
        "trailer",
        "trailers",
        "transfer-encoding",
        "upgrade",
    }
)

# Our own headers: the routing hint is ours, and `host`/`content-length` are
# recomputed for the upstream connection rather than copied.
REQUEST_DROP = HOP_BY_HOP | {"host", "content-length", UPSTREAM_HEADER}
RESPONSE_DROP = HOP_BY_HOP | {"content-length"}

CHUNK = 65536
MAX_REQUEST_BYTES = 32 * 1024 * 1024


@dataclass(frozen=True)
class Config:
    """Everything this service knows, read from the environment once.

    Frozen so that a request handler cannot talk itself into a different
    upstream halfway through a request.
    """

    anthropic_upstream: str
    openai_upstream: str
    default_upstream: str
    control_api: str
    accounting_token: str
    sandbox: str
    agent_instance: str
    privacy_fail_closed: bool
    port: int
    upstream_timeout: float
    control_timeout: float

    @classmethod
    def from_env(cls, env: dict[str, str] | None = None) -> "Config":
        """Read the configuration. Every knob is an env var, none is a file."""
        source = os.environ if env is None else env
        return cls(
            anthropic_upstream=source.get(
                "NESTBOX_ANTHROPIC_UPSTREAM", "https://api.anthropic.com"
            ).rstrip("/"),
            openai_upstream=source.get(
                "NESTBOX_OPENAI_UPSTREAM", "https://api.openai.com"
            ).rstrip("/"),
            default_upstream=source.get("NESTBOX_DEFAULT_UPSTREAM", "").rstrip("/"),
            control_api=source.get(
                "NESTBOX_CONTROL_API", "http://127.0.0.1:8080"
            ).rstrip("/"),
            accounting_token=source.get("NESTBOX_ACCOUNTING_TOKEN", ""),
            sandbox=source.get("NESTBOX_SANDBOX", ""),
            agent_instance=source.get("NESTBOX_AGENT_INSTANCE", ""),
            privacy_fail_closed=source.get("NESTBOX_PRIVACY_FAIL_CLOSED", "0")
            in {"1", "true", "yes", "on"},
            port=int(source.get("NESTBOX_ACCOUNTING_PROXY_PORT", "8099")),
            upstream_timeout=float(
                source.get("NESTBOX_UPSTREAM_TIMEOUT", "120")
            ),
            control_timeout=float(source.get("NESTBOX_CONTROL_TIMEOUT", "5")),
        )


CONFIG = Config.from_env()


class ControlError(Exception):
    """The control API could not be reached or answered with something unusable.

    The message carries a path and a status, never a body: a body here could
    contain the very content we are inspecting.
    """


class PrivacyBlocked(Exception):
    """The control API refused the request. The detector names the reason."""

    def __init__(self, detector: str | None) -> None:
        super().__init__(detector or "privacy")
        self.detector = detector


class UpstreamUnavailable(Exception):
    """No usable upstream for this path, with the error code to answer with."""

    def __init__(self, message: str, code: str) -> None:
        super().__init__(message)
        self.code = code


@dataclass
class Outcome:
    """What inspection decided about one request body."""

    action: str
    detector: str | None
    body: bytes


# -- routing ---------------------------------------------------------------


def dialect_of(path: str) -> str:
    """The API family a path belongs to: `anthropic`, `openai`, `gemini`, `other`.

    Only the path decides. The `X-Nestbox-Upstream` header moves the request,
    it does not change what dialect the body is written in.
    """
    clean = urlsplit(path).path
    if clean in ANTHROPIC_PATHS:
        return "anthropic"
    if clean in OPENAI_PATHS:
        return "openai"
    if clean.startswith(GEMINI_PREFIX) or clean.startswith("/v1beta/"):
        return "gemini"
    return "other"


def upstream_for(config: Config, path: str, override: str | None) -> str:
    """The base URL a request goes to.

    An explicit `X-Nestbox-Upstream` wins over the path, because an operator
    running a gateway or a local model server needs to say so per request. The
    override is validated, not trusted: only `http` and `https` are forwarded,
    so the header cannot turn the proxy into a `file://` reader.
    """
    if override:
        parts = urlsplit(override.strip())
        if parts.scheme not in {"http", "https"} or not parts.netloc:
            raise UpstreamUnavailable(
                "X-Nestbox-Upstream must be an http(s) URL", "upstream.invalid"
            )
        return override.strip().rstrip("/")
    dialect = dialect_of(path)
    if dialect == "anthropic":
        return config.anthropic_upstream
    if dialect == "openai":
        return config.openai_upstream
    if config.default_upstream:
        return config.default_upstream
    raise UpstreamUnavailable(
        "no default upstream is configured", "upstream.unconfigured"
    )


# -- reading the text out of a request -------------------------------------


# A slot is (holder, key) addressing one string in the parsed body, so a rewrite
# can be written back exactly where it came from. Nothing is rebuilt from a
# template: an unknown body shape simply yields no slots.
Slot = tuple[Any, Any]


def _content_slots(value: Any, holder: Any, key: Any, slots: list[Slot]) -> None:
    """Collect the text of a `content`/`parts` value: a string, or a block list."""
    if isinstance(value, str):
        slots.append((holder, key))
        return
    if not isinstance(value, list):
        return
    for part in value:
        if isinstance(part, dict) and isinstance(part.get("text"), str):
            slots.append((part, "text"))


def _response_input_slots(value: Any, holder: Any, key: Any, slots: list[Slot]) -> None:
    """Collect `/v1/responses` input: a string, or items that carry content."""
    if isinstance(value, str):
        slots.append((holder, key))
        return
    if not isinstance(value, list):
        return
    for index, item in enumerate(value):
        if isinstance(item, str):
            slots.append((value, index))
            continue
        if not isinstance(item, dict):
            continue
        if isinstance(item.get("text"), str):
            slots.append((item, "text"))
            continue
        if "content" in item:
            _content_slots(item["content"], item, "content", slots)


def text_slots(body: dict[str, Any]) -> list[Slot]:
    """Every string in a request body that a model would read as a prompt.

    Shapes covered: Anthropic `messages`/`system`, OpenAI chat `messages`, OpenAI
    responses `input`/`instructions`, legacy `prompt`, embeddings `input`, and
    Gemini `contents`. An unrecognised body has no slots.
    """
    slots: list[Slot] = []

    messages = body.get("messages")
    if isinstance(messages, list):
        for message in messages:
            if isinstance(message, dict) and "content" in message:
                _content_slots(message["content"], message, "content", slots)

    system = body.get("system")
    if isinstance(system, str):
        slots.append((body, "system"))
    elif isinstance(system, list):
        for block in system:
            if isinstance(block, dict) and isinstance(block.get("text"), str):
                slots.append((block, "text"))

    if "input" in body:
        _response_input_slots(body["input"], body, "input", slots)

    instructions = body.get("instructions")
    if isinstance(instructions, str):
        slots.append((body, "instructions"))

    prompt = body.get("prompt")
    if isinstance(prompt, str):
        slots.append((body, "prompt"))
    elif isinstance(prompt, list):
        for index, item in enumerate(prompt):
            if isinstance(item, str):
                slots.append((prompt, index))

    contents = body.get("contents")
    if isinstance(contents, list):
        for content in contents:
            if isinstance(content, dict) and "parts" in content:
                _content_slots(content["parts"], content, "parts", slots)

    return slots


def _dump(body: dict[str, Any]) -> bytes:
    return json.dumps(body, ensure_ascii=False, separators=(",", ":")).encode()


def _detector_of(answer: dict[str, Any]) -> str | None:
    """The first detector a finding names, or None when there is no finding."""
    findings = answer.get("findings")
    if not isinstance(findings, list):
        return None
    for finding in findings:
        if isinstance(finding, dict) and isinstance(finding.get("detector"), str):
            return finding["detector"]
    return None


# -- the control API -------------------------------------------------------


def _connection(base: str, timeout: float) -> HTTPConnection:
    parts = urlsplit(base)
    host = parts.hostname or ""
    port = parts.port
    if parts.scheme == "https":
        return HTTPSConnection(host, port, timeout=timeout)
    if parts.scheme != "http":
        raise ControlError(f"unsupported control API scheme: {parts.scheme}")
    return HTTPConnection(host, port, timeout=timeout)


def control_post(
    config: Config, path: str, payload: dict[str, Any]
) -> dict[str, Any]:
    """One JSON POST to the control API, or `ControlError`.

    The token is `NESTBOX_ACCOUNTING_TOKEN`, a node token for the *internal*
    endpoints — it is not a provider credential and never leaves this container
    except to the control API.
    """
    body = json.dumps(payload).encode()
    headers = {
        "Content-Type": "application/json",
        "Content-Length": str(len(body)),
    }
    if config.accounting_token:
        headers["Authorization"] = f"Bearer {config.accounting_token}"
    base = urlsplit(config.control_api)
    target = (base.path.rstrip("/") or "") + path
    connection = None
    try:
        connection = _connection(config.control_api, config.control_timeout)
        connection.request("POST", target, body=body, headers=headers)
        response = connection.getresponse()
        data = response.read()
        status = response.status
    except (OSError, ValueError) as exc:
        raise ControlError(f"{path} unreachable: {type(exc).__name__}") from None
    finally:
        if connection is not None:
            connection.close()
    if status // 100 != 2:
        raise ControlError(f"{path} answered {status}")
    if not data:
        # `204 No Content` is the honest answer for a row that was stored.
        return {}
    try:
        answer = json.loads(data)
    except ValueError:
        raise ControlError(f"{path} answered non-JSON") from None
    if not isinstance(answer, dict):
        raise ControlError(f"{path} answered a non-object")
    return answer


def inspect_payload(config: Config, content: str, destination: str) -> dict[str, Any]:
    """The inspect request body: the text, never a whole request body."""
    return {
        "sandbox": config.sandbox,
        "direction": "outbound",
        "content": content,
        "destination": destination,
    }


def inspect_request(
    config: Config, body: bytes, destination: str
) -> Outcome:
    """Inspect one request body and say what may be forwarded.

    Raises `PrivacyBlocked` when the request must not go on, and
    `ControlError` only when `NESTBOX_PRIVACY_FAIL_CLOSED` is on — otherwise an
    unreachable control API is recorded as `not_inspected` and the request is
    forwarded, because a proxy that is down for inspection should not take the
    agent down with it unless an operator said that it should.
    """
    try:
        parsed = json.loads(body)
    except (ValueError, UnicodeDecodeError):
        return Outcome("not_inspectable", None, body)
    if not isinstance(parsed, dict):
        return Outcome("not_inspectable", None, body)

    slots = text_slots(parsed)
    if not slots:
        return Outcome("not_inspectable", None, body)

    texts = [holder[key] for holder, key in slots]
    joined = "\n".join(texts)

    try:
        answer = control_post(
            config, INSPECT_PATH, inspect_payload(config, joined, destination)
        )
    except ControlError:
        if config.privacy_fail_closed:
            raise
        return Outcome("not_inspected", None, body)

    if not answer.get("permitted", False):
        raise PrivacyBlocked(_detector_of(answer))

    detector = _detector_of(answer)
    action = str(answer.get("action") or "allowed")
    if action != "replaced":
        return Outcome("allowed" if action == "allowed" else action, detector, body)

    transformed = answer.get("transformed_content")
    if not isinstance(transformed, str):
        # `replaced` without a replacement is not a rewrite we can perform. Fail
        # closed on the *decision*: the control API asked for a change we cannot
        # carry out, and forwarding the original would ignore it.
        raise PrivacyBlocked(detector)

    if len(slots) == 1:
        holder, key = slots[0]
        holder[key] = transformed
        return Outcome("replaced", detector, _dump(parsed))

    # More than one text field: the combined answer cannot be put back into the
    # individual messages, so each one is inspected on its own with the same
    # call. A refusal on any single message blocks the whole request.
    for (holder, key), text in zip(slots, texts):
        try:
            one = control_post(
                config, INSPECT_PATH, inspect_payload(config, text, destination)
            )
        except ControlError:
            if config.privacy_fail_closed:
                raise
            return Outcome("not_inspected", None, body)
        if not one.get("permitted", False):
            raise PrivacyBlocked(_detector_of(one) or detector)
        detector = detector or _detector_of(one)
        replacement = one.get("transformed_content")
        if one.get("action") == "replaced" and isinstance(replacement, str):
            holder[key] = replacement
    return Outcome("replaced", detector, _dump(parsed))


# -- usage -----------------------------------------------------------------


def _as_int(value: Any) -> int | None:
    return value if isinstance(value, int) and not isinstance(value, bool) else None


def extract_usage(answer: Any) -> dict[str, int | None] | None:
    """Normalise a provider's counters into one shape.

    OpenAI `prompt_tokens`/`completion_tokens`/`prompt_tokens_details.cached_tokens`,
    Anthropic `input_tokens`/`output_tokens`/`cache_read_input_tokens`, and
    Gemini `promptTokenCount`/`candidatesTokenCount` all become
    `prompt_tokens`/`completion_tokens`/`cached_tokens`/`total_tokens`, so the
    accounting table does not need a column per provider.
    """
    if not isinstance(answer, dict):
        return None
    usage = answer.get("usage")
    if not isinstance(usage, dict):
        usage = answer.get("usageMetadata")
    if not isinstance(usage, dict):
        return None

    prompt = _as_int(usage.get("prompt_tokens"))
    completion = _as_int(usage.get("completion_tokens"))
    cached = None
    total = _as_int(usage.get("total_tokens"))

    if prompt is None and completion is None:
        prompt = _as_int(usage.get("input_tokens"))
        completion = _as_int(usage.get("output_tokens"))
        cached = _as_int(usage.get("cache_read_input_tokens"))
    else:
        details = usage.get("prompt_tokens_details")
        if isinstance(details, dict):
            cached = _as_int(details.get("cached_tokens"))

    if prompt is None and completion is None:
        prompt = _as_int(usage.get("promptTokenCount"))
        completion = _as_int(usage.get("candidatesTokenCount"))
        cached = _as_int(usage.get("cachedContentTokenCount"))
        total = _as_int(usage.get("totalTokenCount"))

    if total is None and prompt is not None and completion is not None:
        total = prompt + completion
    if prompt is None and completion is None:
        return None
    return {
        "prompt_tokens": prompt,
        "completion_tokens": completion,
        "cached_tokens": cached,
        "total_tokens": total,
    }


def model_of(answer: Any, request_body: Any) -> str | None:
    """The model the response names, else the one the request asked for."""
    if isinstance(answer, dict):
        for key in ("model", "modelVersion"):
            value = answer.get(key)
            if isinstance(value, str) and value:
                return value
    if isinstance(request_body, dict):
        value = request_body.get("model")
        if isinstance(value, str) and value:
            return value
    return None


def wants_stream(request_body: Any, path: str) -> bool:
    """Whether the client asked for an incremental answer."""
    if isinstance(request_body, dict) and request_body.get("stream") is True:
        return True
    # Gemini spells it in the path rather than in the body.
    return urlsplit(path).path.endswith(":streamGenerateContent")


def record_row(
    config: Config,
    *,
    provider: str,
    model: str | None,
    endpoint: str,
    status: int,
    latency_ms: int,
    usage: dict[str, int | None] | None,
    content_digest: str,
    privacy_action: str,
    privacy_detector: str | None,
) -> None:
    """Post one metadata row. A failure changes the log, never the answer."""
    row = {
        "sandbox": config.sandbox,
        "agent_instance": config.agent_instance,
        "provider": provider,
        "model": model,
        "direction": "outbound",
        "endpoint": endpoint,
        "status": status,
        "latency_ms": latency_ms,
        "usage": usage,
        "content_digest": content_digest,
        "privacy_action": privacy_action,
        "privacy_detector": privacy_detector,
    }
    try:
        control_post(config, ACCOUNT_PATH, row)
    except ControlError as exc:
        # Metadata can be lost; the agent's answer cannot be. No content here.
        print(f"accounting row dropped: {exc}", flush=True)


def digest_of(body: bytes) -> str:
    """The lowercase hex SHA-256 of the bytes that were forwarded."""
    return hashlib.sha256(body).hexdigest()


# -- the HTTP surface ------------------------------------------------------


class Handler(BaseHTTPRequestHandler):
    """One request in, one request out, one row written."""

    protocol_version = "HTTP/1.1"
    server_version = "nestbox-accounting-proxy/0.1"
    # Set as soon as a status line is on the wire, so the last line of defence
    # knows whether a `500` can still be sent without corrupting the framing.
    _responded = False

    def log_message(self, format: str, *args: Any) -> None:  # noqa: A002 - stdlib
        # Path, status, duration. Never headers, never bodies.
        print(f"{self.address_string()} {format % args}", flush=True)

    # -- small helpers --------------------------------------------------

    def _send_json(self, status: int, payload: dict[str, Any]) -> None:
        body = json.dumps(payload).encode()
        self.send_response(status)
        self._responded = True
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        if self.command != "HEAD":
            self.wfile.write(body)

    def _read_body(self) -> bytes:
        try:
            length = int(self.headers.get("content-length") or 0)
        except ValueError:
            return b""
        if length <= 0:
            return b""
        if length > MAX_REQUEST_BYTES:
            raise ValueError(f"body exceeds {MAX_REQUEST_BYTES} bytes")
        return self.rfile.read(length)

    # -- the pipeline ---------------------------------------------------

    def do_GET(self) -> None:  # noqa: N802 - stdlib naming
        self._handle()

    def do_POST(self) -> None:  # noqa: N802 - stdlib naming
        self._handle()

    def do_PUT(self) -> None:  # noqa: N802 - stdlib naming
        self._handle()

    def do_PATCH(self) -> None:  # noqa: N802 - stdlib naming
        self._handle()

    def do_DELETE(self) -> None:  # noqa: N802 - stdlib naming
        self._handle()

    def do_HEAD(self) -> None:  # noqa: N802 - stdlib naming
        self._handle()

    def do_OPTIONS(self) -> None:  # noqa: N802 - stdlib naming
        self._handle()

    def _handle(self) -> None:
        """Run the pipeline. A bug is a `500`, never a dropped connection."""
        self._responded = False
        try:
            self._pipeline()
        except Exception as exc:  # noqa: BLE001 - last line of defence
            # Type and message only: an exception message here can name a JSON
            # field, never a prompt, and the body is not in this line.
            print(f"{self.path} failed: {type(exc).__name__}: {exc}", flush=True)
            if self._responded:
                self.close_connection = True
                return
            self._send_json(
                500,
                {"error": {"code": "proxy.failed", "message": "internal error"}},
            )

    def _pipeline(self) -> None:
        started = time.monotonic()
        config = CONFIG
        clean = urlsplit(self.path).path

        if clean == "/healthz":
            self._send_json(200, {"status": "ok", "service": "accounting-proxy"})
            return

        try:
            body = self._read_body()
        except ValueError as exc:
            self._send_json(
                413, {"error": {"code": "request.too_large", "message": str(exc)}}
            )
            return

        dialect = dialect_of(self.path)
        try:
            base = upstream_for(
                config, self.path, self.headers.get(UPSTREAM_HEADER)
            )
        except UpstreamUnavailable as exc:
            self._send_json(
                502 if exc.code == "upstream.unconfigured" else 400,
                {"error": {"code": exc.code, "message": str(exc)}},
            )
            return

        try:
            parsed_body: Any = json.loads(body) if body else None
        except (ValueError, UnicodeDecodeError):
            parsed_body = None
        destination = urlsplit(base).hostname or base

        # 1. Inspection, before anything leaves this process.
        try:
            outcome = inspect_request(config, body, destination)
        except PrivacyBlocked as exc:
            answer = {
                "error": {
                    "code": "privacy.blocked",
                    "message": "the content was refused by the privacy policy",
                    "detector": exc.detector,
                }
            }
            self._send_json(403, answer)
            record_row(
                config,
                provider=dialect,
                model=None,  # nothing was forwarded, so no model was used
                endpoint=clean,
                status=403,
                latency_ms=int((time.monotonic() - started) * 1000),
                usage=None,
                content_digest=digest_of(body),
                privacy_action="blocked",
                privacy_detector=exc.detector,
            )
            return
        except ControlError:
            self._send_json(
                503,
                {
                    "error": {
                        "code": "privacy.unavailable",
                        "message": "content inspection is unavailable and fail-closed is on",
                    }
                },
            )
            record_row(
                config,
                provider=dialect,
                model=None,
                endpoint=clean,
                status=503,
                latency_ms=int((time.monotonic() - started) * 1000),
                usage=None,
                content_digest=digest_of(body),
                privacy_action="not_inspected",
                privacy_detector=None,
            )
            return

        # 2. Forwarding, with the inspect verdict applied to the body.
        streaming = wants_stream(parsed_body, self.path)
        status, usage, model = self._forward(base, outcome.body, streaming)
        latency_ms = int((time.monotonic() - started) * 1000)

        # 3. Accounting, after the client has its answer.
        record_row(
            config,
            provider=dialect,
            model=model or model_of(None, parsed_body),
            endpoint=clean,
            status=status,
            latency_ms=latency_ms,
            usage=usage,
            content_digest=digest_of(outcome.body),
            privacy_action=outcome.action,
            privacy_detector=outcome.detector,
        )

    def _upstream_headers(self, body: bytes) -> dict[str, str]:
        """The client's headers, minus what belongs to this hop.

        `Authorization` and `x-api-key` are copied as they arrived: this service
        has no credential of its own to substitute.
        """
        headers: dict[str, str] = {}
        for name, value in self.headers.items():
            lower = name.lower()
            if lower in REQUEST_DROP:
                continue
            headers[name] = value
        headers["Content-Length"] = str(len(body))
        return headers

    def _forward(
        self, base: str, body: bytes, streaming: bool
    ) -> tuple[int, dict[str, int | None] | None, str | None]:
        """Send the request on, relay the answer, return (status, usage, model)."""
        config = CONFIG
        parts = urlsplit(base)
        path = urlsplit(self.path)
        target = (parts.path.rstrip("/") or "") + path.path
        if path.query:
            target += "?" + path.query

        connection = None
        try:
            if parts.scheme == "https":
                connection = HTTPSConnection(
                    parts.hostname, parts.port, timeout=config.upstream_timeout
                )
            else:
                connection = HTTPConnection(
                    parts.hostname, parts.port, timeout=config.upstream_timeout
                )
            connection.request(
                self.command,
                target,
                body=body if body else None,
                headers=self._upstream_headers(body),
            )
            response = connection.getresponse()
        except (OSError, ValueError):
            if connection is not None:
                connection.close()
            self._send_json(
                502,
                {
                    "error": {
                        "code": "upstream.unreachable",
                        "message": "the upstream did not answer",
                    }
                },
            )
            return 502, None, None

        try:
            return self._relay(response, streaming)
        finally:
            connection.close()

    def _relay(
        self, response: Any, streaming: bool
    ) -> tuple[int, dict[str, int | None] | None, str | None]:
        content_type = (response.getheader("content-type") or "").lower()
        is_stream = streaming or "text/event-stream" in content_type
        length_header = response.getheader("content-length")

        self.send_response(response.status)
        self._responded = True
        for name, value in response.getheaders():
            if name.lower() in RESPONSE_DROP:
                continue
            self.send_header(name, value)

        if is_stream:
            # No length is known: the connection itself delimits the body, and
            # it must close so the client sees the end.
            self.send_header("Connection", "close")
            self.close_connection = True
            self.end_headers()
            while True:
                chunk = response.read(CHUNK)
                if not chunk:
                    break
                self.wfile.write(chunk)
                self.wfile.flush()
            return response.status, None, None

        if length_header is not None:
            data = response.read()
            self.send_header("Content-Length", str(len(data)))
            self.end_headers()
            if self.command != "HEAD":
                self.wfile.write(data)
        else:
            # Upstream sent a close-delimited body; relay it the same way.
            self.send_header("Connection", "close")
            self.close_connection = True
            self.end_headers()
            while True:
                chunk = response.read(CHUNK)
                if not chunk:
                    break
                self.wfile.write(chunk)
                self.wfile.flush()
            return response.status, None, None

        usage: dict[str, int | None] | None = None
        model: str | None = None
        try:
            answer = json.loads(data) if data else None
        except (ValueError, UnicodeDecodeError):
            answer = None
        if isinstance(answer, dict):
            usage = extract_usage(answer)
            model = model_of(answer, None)
        return response.status, usage, model


def main() -> None:
    if not CONFIG.accounting_token:
        print(
            "NESTBOX_ACCOUNTING_TOKEN is unset: no inspection and no accounting "
            "will succeed",
            flush=True,
        )
    if CONFIG.privacy_fail_closed:
        print("privacy fail-closed is ON: an unreachable control API answers 503", flush=True)
    ThreadingHTTPServer(("0.0.0.0", CONFIG.port), Handler).serve_forever()


if __name__ == "__main__":
    main()
