"""Docker execution adapter for the phase-1 Nestbox development runtime.

This is deliberately an adapter boundary: the control API never talks to Docker directly.
The adapter exposes observable OCI sandbox facts now and can be replaced by the official
OpenShell SDK adapter without changing the public control-plane contract.
"""
from __future__ import annotations

import logging
import os
from datetime import UTC, datetime
from typing import Any

import docker
from docker.errors import DockerException, NotFound
from fastapi import FastAPI, HTTPException
from pydantic import BaseModel, Field

from app import inventory as inventory_probe
from app import processes as process_table
from app import stats as resource_stats
from app import terminal as terminal_transport

app = FastAPI(title="Nestbox Sandbox Runtime", version="1.0.0")
client = docker.from_env()

# Admin Center adapters. Each module owns its routes and its own Docker client,
# so the packages that fill them in can change one file without touching the
# request path that already ships. `terminal` answers 1011 until the PTY pump
# exists; `inventory`, `processes`, and `stats` answer real data today.
app.include_router(resource_stats.router)
app.include_router(process_table.router)
app.include_router(inventory_probe.router)
app.include_router(terminal_transport.router)
NETWORK = os.getenv("NESTBOX_RUNTIME_NETWORK", "nestbox-runtime")
# The only way out of a sandbox. It is joined to the sandbox network by this
# adapter rather than by compose, because this adapter is what creates that
# network; see `attach_egress_proxy`.
EGRESS_PROXY_CONTAINER = os.getenv("NESTBOX_EGRESS_PROXY_CONTAINER", "nestbox-egress-proxy-1")
# The name a sandbox resolves the proxy under, and the port it listens on. The
# control API writes `http://<host>:<port>` into the sandbox environment, so the
# two defaults have to agree; `NESTBOX_EGRESS_PROXY_URL` there is the other half.
EGRESS_PROXY_HOST = os.getenv("NESTBOX_EGRESS_PROXY_HOST", "egress-proxy")
EGRESS_PROXY_PORT = 3128
# Ports a tool would try when it ignores the proxy environment. Redirecting them
# inside the proxy container can only narrow what a sandbox reaches, never widen
# it, which is why a failure to install the rules is reported and not fatal.
REDIRECT_PORTS = (80, 443, 8080)
ALLOWED_IMAGES = set(
    os.getenv(
        "NESTBOX_ALLOWED_IMAGES",
        "ubuntu:24.04,nestbox-ubuntu-dev:local,nestbox-hermes:local,nestbox-openclaw:local",
    ).split(",")
)
# Images the runner built itself from a recipe are always allowed: they carry
# this prefix and a content hash, and the Dockerfile came from the control
# plane. Anything else must be on the allowlist.
RECIPE_IMAGE_PREFIX = "nestbox-recipe/"
# The only directory tree a bind mount may come from. Validated as a real path
# so `..` and symlinks cannot escape it. Empty disables mounts entirely.
PERSISTENT_BASE = os.path.realpath(os.getenv("NESTBOX_PERSISTENT_BASE", "/var/lib/nestbox/persistent"))
HOST_PROC = os.getenv("NESTBOX_HOST_PROC", "/proc")


def image_allowed(image: str) -> bool:
    return image in ALLOWED_IMAGES or image.startswith(RECIPE_IMAGE_PREFIX)


class MountSpec(BaseModel):
    source: str
    target: str = Field(pattern=r"^/.+")
    read_only: bool = False


def validate_mount(mount: MountSpec) -> dict[str, Any]:
    """Refuse any source outside the persistent base.

    The control plane computed the source from a validated slug; this is the
    second line, for the case where the control plane is wrong or compromised.
    """
    real = os.path.realpath(mount.source)
    if not PERSISTENT_BASE or real != PERSISTENT_BASE and not real.startswith(PERSISTENT_BASE + os.sep):
        raise HTTPException(422, f"mount source is outside the persistent base: {mount.source}")
    if ".." in mount.target.split("/"):
        raise HTTPException(422, "mount target must not contain '..'")
    os.makedirs(real, exist_ok=True)
    return {"type": "bind", "source": real, "target": mount.target, "read_only": mount.read_only}


class SandboxRequest(BaseModel):
    name: str = Field(pattern=r"^[a-z0-9-]{1,63}$")
    image: str = "nestbox-ubuntu-dev:local"
    command: list[str] = Field(default_factory=lambda: ["sleep", "infinity"])
    runtime: str = "docker-development"
    pack_key: str = "ubuntu-dev"
    tool_profile: str = "development-default"
    environment: dict[str, str] = Field(default_factory=dict)
    memory_limit: str = "1g"
    cpu_limit: float = Field(default=1.0, gt=0, le=8)
    pids_limit: int = Field(default=256, ge=32, le=4096)
    ports: dict[str, int] = Field(default_factory=dict)
    mounts: list[MountSpec] = Field(default_factory=list)
    gpu_count: int = Field(default=0, ge=0, le=8)


def container_name(name: str) -> str:
    return f"nestbox-sandbox-{name}"


def ensure_network() -> Any:
    """Return the sandbox network, or refuse one that has a route out.

    Docker has no per-container "no default route", so the network carries the
    property instead: `internal: true` means the bridge has no gateway, no NAT
    and no DNS beyond container names. A container on it gets a route to its own
    subnet and nothing else — no default route, no way to reach the host, and an
    external name answers `SERVFAIL`.

    A network that already exists with `Internal=false` is exactly the state
    this replaces: every sandbox on it has a default route and full egress. It is
    refused with 503 rather than reused, because a create that quietly succeeds
    on an open network is the failure that made the old setup look policed while
    nothing was. `internal` cannot be flipped on an existing bridge, so the fix
    is an operator action (remove the network, let this adapter recreate it) and
    it is named in the error.
    """
    try:
        network = client.networks.get(NETWORK)
    except NotFound:
        return client.networks.create(
            NETWORK,
            driver="bridge",
            internal=True,
            labels={"nestbox.managed": "true"},
        )
    if not network.attrs.get("Internal", False):
        raise HTTPException(
            503,
            f"sandbox network '{NETWORK}' has a route out (internal=false). "
            "Remove it and create a sandbox again so the adapter recreates it "
            "with no default route; nothing on an open network is policed.",
        )
    return network


def find_egress_proxy() -> Any | None:
    """The node's egress proxy container, by configured name or compose label."""
    try:
        return client.containers.get(EGRESS_PROXY_CONTAINER)
    except NotFound:
        pass
    try:
        containers = client.containers.list(
            all=True, filters={"label": "com.docker.compose.service=egress-proxy"}
        )
    except DockerException:
        return None
    return containers[0] if containers else None


def egress_proxy_endpoint(network: Any, name: str) -> dict[str, Any] | None:
    """The network endpoint of a container, or `None` when it is not attached."""
    try:
        network.reload()
    except DockerException:
        return None
    for endpoint in (network.attrs.get("Containers") or {}).values():
        if endpoint.get("Name") == name:
            return endpoint
    return None


def attach_egress_proxy(network: Any, proxy: Any) -> str:
    """Put the proxy on the sandbox network under the name the env names it.

    The proxy is the only container besides this adapter that belongs on the
    sandbox network, and it is joined here because this adapter is what creates
    the network. The alias is not cosmetic: a manual `connect` gives the
    container only its container name, so a sandbox asking for
    `http://egress-proxy:3128` would not resolve and every request would fail
    with a proxy that is right there. `aliases` is what makes the name the
    control API writes into the environment true.

    Idempotent, and a failure is reported rather than fatal: an unreachable proxy
    leaves the sandbox isolated, which is the safe direction.
    """
    endpoint = egress_proxy_endpoint(network, proxy.name)
    if endpoint is not None:
        if EGRESS_PROXY_HOST in (endpoint.get("Aliases") or []):
            return "attached"
        # Present without the name a sandbox asks for: reconnect so the alias
        # exists. Envoy keeps its listener; only the endpoint is re-added.
        try:
            network.disconnect(proxy)
        except DockerException:
            pass
    try:
        network.connect(proxy, aliases=[EGRESS_PROXY_HOST])
        return "attached-now"
    except DockerException as exc:
        if "already exists" in str(exc) or "already attached" in str(exc):
            return "attached"
        return f"failed: {exc}"


def redirect_rule_status(proxy: Any) -> str:
    """Install the NAT redirect for ports a proxy-unaware tool would use.

    Idempotent: each port is checked with `-C` before `-A`, so a restart of this
    adapter does not stack duplicate rules. Installed *inside* the proxy
    container because that is the one component with `NET_ADMIN` (deploy README);
    a sandbox has every capability dropped and cannot install anything.

    This is a convenience, never the boundary. A sandbox has no default route, so
    a raw socket is already unroutable; the rule only helps traffic that does
    reach the proxy container — a client that addresses the proxy as a
    gateway-like host on 80/443/8080 instead of 3128. That is why an image
    without `iptables` is reported as `unavailable` and not treated as a hole:
    the sandbox is exactly as isolated either way.
    """
    try:
        for port in REDIRECT_PORTS:
            match = [
                "-t", "nat", "-p", "tcp", "--dport", str(port),
                "-j", "REDIRECT", "--to-ports", str(EGRESS_PROXY_PORT),
            ]
            check = proxy.exec_run(["iptables", "-C", *match], demux=False)
            if check.exit_code == 0:
                continue
            if check.exit_code == 127:
                return "unavailable: the proxy image has no iptables"
            added = proxy.exec_run(["iptables", "-A", *match], demux=False)
            if added.exit_code == 127:
                return "unavailable: the proxy image has no iptables"
            if added.exit_code != 0:
                detail = (added.output or b"").decode(errors="replace").strip()
                return f"failed: {detail or added.exit_code}"
    except DockerException as exc:
        return f"failed: {exc}"
    return f"prepared: {','.join(str(port) for port in REDIRECT_PORTS)} -> {EGRESS_PROXY_PORT}"


def prepare_egress(network: Any) -> dict[str, str]:
    """Make the proxy reachable from the sandbox network, and say what happened.

    Called before a container is created, so the proxy is on the network the
    moment the first sandbox is. Every value is a fact — `attached`, `missing`,
    `unavailable` — because this is what an operator has to check when a sandbox
    "has no internet", and a cheerful `ok` would hide the interesting case.
    """
    proxy = find_egress_proxy()
    if proxy is None:
        return {
            "proxy": "missing",
            "attached": "no",
            "redirects": "no proxy container",
        }
    return {
        "proxy": proxy.name,
        "attached": attach_egress_proxy(network, proxy),
        "redirects": redirect_rule_status(proxy),
    }



def utc(value: str | None) -> str | None:
    if not value or value.startswith("0001-"):
        return None
    return value


def details(item: Any) -> dict[str, Any]:
    item.reload()
    attrs = item.attrs
    state = attrs.get("State", {})
    networks = attrs.get("NetworkSettings", {}).get("Networks", {})
    labels = attrs.get("Config", {}).get("Labels") or {}
    ports = attrs.get("NetworkSettings", {}).get("Ports") or {}
    return {
        "id": item.id,
        "name": item.name.removeprefix("nestbox-sandbox-"),
        "container_name": item.name,
        "status": item.status,
        "runtime": labels.get("nestbox.runtime", "docker-development"),
        "pack_key": labels.get("nestbox.pack", ""),
        "tool_profile": labels.get("nestbox.tool_profile", ""),
        "image": attrs.get("Config", {}).get("Image", ""),
        "created_at": utc(attrs.get("Created")),
        "started_at": utc(state.get("StartedAt")),
        "finished_at": utc(state.get("FinishedAt")),
        "uptime_seconds": max(0, int((datetime.now(UTC) - datetime.fromisoformat(state["StartedAt"].replace("Z", "+00:00"))).total_seconds())) if state.get("Running") and state.get("StartedAt") else 0,
        "exit_code": state.get("ExitCode"),
        "health": (state.get("Health") or {}).get("Status", "unknown"),
        "error": state.get("Error", ""),
        "network": [
            {"name": key, "ip": value.get("IPAddress", ""), "gateway": value.get("Gateway", ""), "aliases": value.get("Aliases", [])}
            for key, value in networks.items()
        ],
        "ports": ports,
        "resources": {
            "memory_limit": attrs.get("HostConfig", {}).get("Memory", 0),
            "nano_cpus": attrs.get("HostConfig", {}).get("NanoCpus", 0),
            "pids_limit": attrs.get("HostConfig", {}).get("PidsLimit", 0),
        },
        "labels": {key: value for key, value in labels.items() if key.startswith("nestbox.")},
    }


@app.get("/healthz")
def healthz() -> dict[str, str]:
    """Docker reachable, and what the way out of a sandbox currently looks like."""
    try:
        client.ping()
    except DockerException as exc:
        raise HTTPException(503, f"Docker runtime unavailable: {exc}") from exc
    try:
        internal = client.networks.get(NETWORK).attrs.get("Internal", False)
        network_state = "internal" if internal else "open"
    except NotFound:
        network_state = "absent"
    except DockerException:
        network_state = "unknown"
    proxy = find_egress_proxy()
    return {
        "status": "ok",
        "adapter": "docker-development",
        "network": NETWORK,
        "network_isolation": network_state,
        "egress_proxy": proxy.name if proxy else "missing",
    }


@app.get("/v1/egress")
def egress() -> dict[str, Any]:
    """The sandbox path out, as facts.

    Read-only on purpose: it inspects the network and the proxy and reports what
    it found. The redirect check asks the proxy container whether each rule is
    present (`iptables -C`) instead of installing it, because a GET that changes
    firewall state is a GET an operator cannot trust to be harmless.
    """
    try:
        network = client.networks.get(NETWORK)
    except NotFound:
        return {
            "network": NETWORK,
            "network_isolation": "absent",
            "proxy": None,
            "attached": False,
            "alias": EGRESS_PROXY_HOST,
            "redirects": "no network",
        }
    except DockerException as exc:
        raise HTTPException(503, f"Docker runtime unavailable: {exc}") from exc
    proxy = find_egress_proxy()
    endpoint = egress_proxy_endpoint(network, proxy.name) if proxy else None
    missing = redirect_missing(proxy) if endpoint is not None else "proxy not attached"
    return {
        "network": NETWORK,
        "network_isolation": "internal" if network.attrs.get("Internal", False) else "open",
        "proxy": proxy.name if proxy else None,
        "attached": endpoint is not None,
        "alias": EGRESS_PROXY_HOST,
        "alias_present": bool(endpoint and EGRESS_PROXY_HOST in (endpoint.get("Aliases") or [])),
        "proxy_url": f"http://{EGRESS_PROXY_HOST}:{EGRESS_PROXY_PORT}",
        "redirects": "prepared" if not missing else missing,
    }


def redirect_missing(proxy: Any) -> str:
    """The first port with no redirect rule, or an empty string when all exist.

    The mirror of `redirect_rule_status` for a read-only report: it only asks,
    never appends.
    """
    for port in REDIRECT_PORTS:
        match = [
            "-t", "nat", "-p", "tcp", "--dport", str(port),
            "-j", "REDIRECT", "--to-ports", str(EGRESS_PROXY_PORT),
        ]
        try:
            check = proxy.exec_run(["iptables", "-C", *match], demux=False)
        except DockerException as exc:
            return f"unavailable ({exc})"
        if check.exit_code != 0:
            return f"port {port} has no redirect"
    return ""


@app.post("/v1/sandboxes", status_code=201)
def create_sandbox(body: SandboxRequest) -> dict[str, Any]:
    if not image_allowed(body.image):
        raise HTTPException(422, "image is not in the approved phase-1 catalog")
    network = ensure_network()
    name = container_name(body.name)
    try:
        return details(client.containers.get(name))
    except NotFound:
        pass
    mounts = [validate_mount(m) for m in body.mounts]
    extra: dict[str, Any] = {}
    if mounts:
        from docker.types import Mount as DockerMount

        extra["mounts"] = [
            DockerMount(target=m["target"], source=m["source"], type="bind", read_only=m["read_only"])
            for m in mounts
        ]
    if body.gpu_count > 0:
        if not container_gpu_available():
            raise HTTPException(422, "this node cannot give a container a GPU: no NVIDIA container runtime")
        from docker.types import DeviceRequest

        extra["device_requests"] = [DeviceRequest(count=body.gpu_count, capabilities=[["gpu"]])]
    # Before the container exists, so the proxy is on the network the moment a
    # sandbox can send its first request. The result is logged and served by
    # `/v1/egress`; it is not fatal, because a sandbox without a reachable proxy
    # is isolated rather than exposed.
    egress_state = prepare_egress(network)
    logger = logging.getLogger("nestbox.runner")
    if egress_state["proxy"] == "missing" or not egress_state["attached"].startswith("attached"):
        logger.warning("egress proxy not reachable from the sandbox network: %s", egress_state)
    else:
        logger.info("egress ready for %s: %s", body.name, egress_state)
    try:
        container = client.containers.run(
            body.image,
            body.command,
            name=name,
            detach=True,
            network=NETWORK,
            hostname=body.name,
            environment=body.environment,
            mem_limit=body.memory_limit,
            nano_cpus=int(body.cpu_limit * 1_000_000_000),
            pids_limit=body.pids_limit,
            cap_drop=["ALL"],
            security_opt=["no-new-privileges:true"],
            **extra,
            labels={
                "nestbox.managed": "true",
                "nestbox.runtime": body.runtime,
                "nestbox.pack": body.pack_key,
                "nestbox.tool_profile": body.tool_profile,
            },
        )
        return details(container)
    except DockerException as exc:
        raise HTTPException(400, f"sandbox creation failed: {exc}") from exc


def container_gpu_available() -> bool:
    """Whether the Docker daemon has an NVIDIA runtime registered."""
    try:
        runtimes = client.info().get("Runtimes") or {}
    except DockerException:
        return False
    return "nvidia" in runtimes


def read_text(path: str) -> str:
    try:
        with open(path, encoding="utf-8", errors="replace") as handle:
            return handle.read()
    except OSError:
        return ""


def run_capture(args: list[str], timeout: float = 5.0) -> str | None:
    """Run a fixed argument vector and return stdout, or None if it is absent."""
    import subprocess

    try:
        done = subprocess.run(args, capture_output=True, text=True, timeout=timeout, check=False)
    except (OSError, subprocess.SubprocessError):
        return None
    if done.returncode != 0:
        return None
    return done.stdout


@app.get("/v1/hardware")
def hardware() -> dict[str, Any]:
    """What this host has, as raw text for the control plane to parse.

    Raw rather than parsed on purpose: the parsers are tested Rust, and a
    second implementation here would be a second opinion. ``/proc`` is read
    from ``NESTBOX_HOST_PROC`` so a containerised runner can be given the host's
    ``/proc`` and report the machine, not itself.
    """
    cpuinfo = read_text(os.path.join(HOST_PROC, "cpuinfo"))
    meminfo = read_text(os.path.join(HOST_PROC, "meminfo"))
    csv = run_capture([
        "nvidia-smi",
        "--query-gpu=index,name,memory.total,memory.free",
        "--format=csv,noheader,nounits",
    ])
    header = run_capture(["nvidia-smi"]) if csv is not None else None
    note = None
    if csv is None:
        note = "nvidia-smi is not available on this node; GPU models need a GPU host."
    return {
        "hostname": read_text("/etc/hostname").strip() or None,
        "cpuinfo": cpuinfo,
        "meminfo": meminfo,
        "nvidia_smi_csv": csv,
        "nvidia_smi_header": header,
        "container_gpu": container_gpu_available(),
        "note": note,
    }


class BuildRequest(BaseModel):
    tag: str = Field(pattern=r"^nestbox-recipe/[a-z0-9-]{1,63}:[a-f0-9]{8,64}$")
    dockerfile: str = Field(min_length=1, max_length=65536)
    files: list[tuple[str, str]] = Field(default_factory=list)


@app.post("/v1/builds")
def build_image(body: BuildRequest) -> dict[str, Any]:
    """Build an image from a generated Dockerfile.

    The tag pattern is the only way a build can name itself, so a recipe image
    always carries the prefix the allowlist trusts. Extra files are written
    into the build context by relative name; a name with a path separator is
    refused.
    """
    import io
    import tarfile

    buffer = io.BytesIO()
    with tarfile.open(fileobj=buffer, mode="w") as tar:
        def add(name: str, content: str) -> None:
            data = content.encode()
            info = tarfile.TarInfo(name=name)
            info.size = len(data)
            info.mode = 0o644
            tar.addfile(info, io.BytesIO(data))

        add("Dockerfile", body.dockerfile)
        for name, content in body.files:
            if "/" in name or name.startswith(".") or not name:
                raise HTTPException(422, f"invalid build file name: {name!r}")
            add(name, content)
    buffer.seek(0)

    log_lines: list[str] = []
    try:
        image, stream = client.images.build(fileobj=buffer, custom_context=True, tag=body.tag, rm=True, forcerm=True)
        for chunk in stream:
            if isinstance(chunk, dict) and "stream" in chunk:
                log_lines.append(chunk["stream"])
        return {"tag": body.tag, "image_id": image.id, "log": "".join(log_lines)[-65536:], "success": True}
    except docker.errors.BuildError as exc:
        for chunk in exc.build_log:
            if isinstance(chunk, dict) and "stream" in chunk:
                log_lines.append(chunk["stream"])
        return {"tag": body.tag, "image_id": None, "log": ("".join(log_lines) + str(exc))[-65536:], "success": False}
    except DockerException as exc:
        raise HTTPException(400, f"build failed: {exc}") from exc


@app.get("/v1/sandboxes")
def list_sandboxes() -> list[dict[str, Any]]:
    return [details(item) for item in client.containers.list(all=True, filters={"label": "nestbox.managed=true"})]


@app.get("/v1/sandboxes/{name}")
def inspect_sandbox(name: str) -> dict[str, Any]:
    try:
        return details(client.containers.get(container_name(name)))
    except NotFound as exc:
        raise HTTPException(404, "sandbox not found") from exc


# Registered before the `/{name}/{action}` lifecycle route on purpose: FastAPI
# matches routes in declaration order, and the catch-all would otherwise claim
# `/exec` and answer "unsupported lifecycle action".

class ExecRequest(BaseModel):
    """One command to run inside a sandbox.

    The command is an argument vector, never a shell string. A caller that
    wants a shell asks for ``["bash", "-lc", "..."]`` explicitly, which is
    visible in the audit record. The output is capped so a runaway command
    cannot fill the API's memory with a single response.
    """

    command: list[str] = Field(min_length=1, max_length=64)
    workdir: str | None = None
    timeout_seconds: int = Field(default=60, ge=1, le=600)
    max_output_bytes: int = Field(default=65536, ge=1024, le=1_048_576)


@app.post("/v1/sandboxes/{name}/exec")
def exec_in_sandbox(name: str, body: ExecRequest) -> dict[str, Any]:
    """Run one command in a running sandbox and return its output.

    Phase 1 is a request/response exec rather than an interactive PTY: it is
    what the smoke test, the console's "run" box, and a scheduled health probe
    need, and it keeps the transport to plain JSON. A streaming PTY is a
    separate, later endpoint.
    """
    for part in body.command:
        if "\x00" in part:
            raise HTTPException(422, "command arguments must not contain NUL")
    try:
        container = client.containers.get(container_name(name))
    except NotFound as exc:
        raise HTTPException(404, "sandbox not found") from exc
    if container.status != "running":
        raise HTTPException(409, "sandbox is not running")

    import threading

    result: dict[str, Any] = {}

    def run() -> None:
        try:
            exit_code, output = container.exec_run(
                body.command,
                workdir=body.workdir,
                demux=False,
                tty=False,
                environment=None,
            )
            result["exit_code"] = exit_code
            result["output"] = output or b""
        except DockerException as exc:  # pragma: no cover - surfaced below
            result["error"] = str(exc)

    worker = threading.Thread(target=run, daemon=True)
    worker.start()
    worker.join(body.timeout_seconds)
    if worker.is_alive():
        raise HTTPException(504, f"command did not finish within {body.timeout_seconds}s")
    if "error" in result:
        raise HTTPException(400, f"exec failed: {result['error']}")

    raw: bytes = result.get("output", b"")
    truncated = len(raw) > body.max_output_bytes
    text = raw[: body.max_output_bytes].decode(errors="replace")
    return {
        "name": name,
        "exit_code": result.get("exit_code"),
        "output": text,
        "truncated": truncated,
    }


@app.post("/v1/sandboxes/{name}/{action}")
def lifecycle(name: str, action: str) -> dict[str, Any]:
    if action not in {"start", "stop", "restart", "remove"}:
        raise HTTPException(422, "unsupported lifecycle action")
    try:
        container = client.containers.get(container_name(name))
        if action == "remove":
            container.remove(force=True)
            return {"name": name, "status": "deleted"}
        getattr(container, action)()
        return details(container)
    except NotFound as exc:
        raise HTTPException(404, "sandbox not found") from exc
    except DockerException as exc:
        raise HTTPException(400, f"lifecycle action failed: {exc}") from exc


@app.get("/v1/sandboxes/{name}/logs")
def logs(name: str, tail: int = 200) -> dict[str, str | int]:
    if not 1 <= tail <= 1000:
        raise HTTPException(422, "tail must be between 1 and 1000")
    try:
        container = client.containers.get(container_name(name))
        return {"name": name, "tail": tail, "logs": container.logs(tail=tail, timestamps=True).decode(errors="replace")}
    except NotFound as exc:
        raise HTTPException(404, "sandbox not found") from exc
