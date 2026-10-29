"""Live package catalogues and post-build verification.

The image builder must be able to offer what a distribution actually ships, and
it must be able to prove that a built image contains what the recipe asked for.
Both questions are answered by the distribution itself, inside a container of
the baseline image, because the control API runs in an unrelated image and a
scraped web page is not the same catalogue that `apt`/`dnf`/`apk` would use.

Two rules shape this file:

* **The command is fixed.** A query is data: it is validated against a package
  name pattern and passed to the container as `NESTBOX_QUERY`, never spliced
  into a shell string. The script per package manager is a constant here.
* **The container is indistinguishable from a build.** The same network and the
  same image allowlist, no capabilities, no mounts, a memory and PID limit, and
  removal on every path. A catalogue query is a read of a public index; it must
  not become a way to run something.
"""
from __future__ import annotations

import re
import time
from typing import Any

import docker
from docker.errors import APIError, ImageNotFound
from fastapi import APIRouter, HTTPException
from pydantic import BaseModel

from app.images import image_allowed

router = APIRouter()
_CLIENT: docker.DockerClient | None = None


def client() -> docker.DockerClient:
    """The Docker client, created on first use.

    Lazy so the parsing rules can be tested without a socket: importing this
    module must not need a container runtime.
    """
    global _CLIENT
    if _CLIENT is None:
        _CLIENT = docker.from_env()
    return _CLIENT

# Deliberately the same shape a package name has; the trailing `*` for dnf is
# added inside the fixed script, not by the caller.
QUERY = re.compile(r"^[A-Za-z0-9][A-Za-z0-9.+:_@/-]{0,80}$")
MANAGERS = {"apt", "dnf", "apk"}
APK_LINE = re.compile(r"^(?P<name>.+?)-(?P<version>\d[^-]*(?:-r\d+)?)$")

# A catalogue is a fact about a remote index, so a short cache is a courtesy to
# the index, never control state. Ten minutes is shorter than any release cycle.
CACHE_TTL_SECONDS = 600
CACHE_LIMIT = 128
_CACHE: dict[tuple[str, str, str], tuple[float, dict[str, Any]]] = {}

SEARCH_SCRIPT = {
    # `APT::Sandbox::User=root` keeps apt from chowning its lists to `_apt`,
    # which needs CAP_CHOWN. The container has every capability dropped, and a
    # catalogue read is not worth handing one back for.
    "apt": (
        "export DEBIAN_FRONTEND=noninteractive; "
        'apt-get -o APT::Sandbox::User=root update -qq >/dev/null 2>&1 || exit 3; '
        'apt-cache search --names-only "$NESTBOX_QUERY"'
    ),
    "dnf": 'dnf -q --refresh list --available "$NESTBOX_QUERY*"',
    "apk": 'apk update -q >/dev/null 2>&1 || exit 3; apk search -v "$NESTBOX_QUERY"',
}

# `dpkg-query`, `rpm` and `apk` all answer "which packages does this image
# contain" without a network call, which is what makes verification meaningful:
# it reads the image, not the index it was built from.
LIST_SCRIPT = {
    "apt": "dpkg-query -W -f='${binary:Package} ${Version}\\n'",
    "dnf": "rpm -qa --qf '%{NAME} %{VERSION}\\n'",
    # Names only: `apk info -v` prints `name-version`, and a version-suffixed
    # name would never equal the package the recipe asked for.
    "apk": "apk info",
}


class CatalogueBody(BaseModel):
    image: str
    package_manager: str
    query: str


class VerifyBody(BaseModel):
    tag: str
    package_manager: str
    packages: list[str] = []


def _allowed(image: str) -> str:
    if not image_allowed(image):
        raise HTTPException(422, f"image '{image}' is not in the approved catalogue")
    return image


def _run(image: str, script: str, environment: dict[str, str], timeout: int = 180) -> tuple[int, str]:
    """Run one fixed script in a throwaway container of `image`.

    Returns the exit code and stdout. The container is removed on every path,
    including a timeout, so a failed query cannot leave a process behind.
    """
    def create() -> Any:
        return client().containers.create(
            _allowed(image),
            command=["sh", "-c", script],
            environment=environment,
            network_mode="bridge",
            mem_limit="1g",
            pids_limit=256,
            cap_drop=["ALL"],
            security_opt=["no-new-privileges"],
            labels={"nestbox.catalogue": "1"},
            detach=True,
        )

    try:
        container = create()
    except ImageNotFound:
        # The first search on a baseline pulls its image once; every later
        # search and every build then finds it locally.
        try:
            client().images.pull(image)
        except APIError as exc:
            raise HTTPException(422, f"image '{image}' could not be pulled: {exc}") from exc
        try:
            container = create()
        except APIError as exc:
            raise HTTPException(503, f"the runtime refused the container: {exc}") from exc
    except APIError as exc:
        raise HTTPException(503, f"the runtime refused the container: {exc}") from exc
    try:
        container.start()
        result = container.wait(timeout=timeout)
        stdout = container.logs(stdout=True, stderr=False)
        return int(result.get("StatusCode", -1)), stdout.decode("utf-8", "replace")
    except Exception as exc:  # docker-py raises a bare Exception on a timeout
        try:
            container.kill()
        except Exception:
            pass
        raise HTTPException(504, f"the container did not finish within {timeout}s: {exc}") from exc
    finally:
        try:
            container.remove(force=True)
        except Exception:
            pass


def _hit(name: str, version: str | None, summary: str | None) -> dict[str, Any]:
    return {"name": name, "version": version, "summary": summary}


def parse_search(manager: str, output: str, limit: int = 60) -> list[dict[str, Any]]:
    """Turn one manager's search output into package hits.

    Each format is parsed as it is documented rather than by a generic split: an
    apt line carries a summary, a dnf line is a name, version and repository,
    and an apk line is `name-version`.
    """
    hits: list[dict[str, Any]] = []
    for raw in output.splitlines():
        line = raw.strip()
        if not line:
            continue
        if manager == "apt":
            name, _, summary = line.partition(" - ")
            hits.append(_hit(name.strip(), None, summary.strip() or None))
        elif manager == "dnf":
            parts = line.split()
            if len(parts) < 2 or parts[0] == "Available":
                continue
            name = parts[0].rsplit(".", 1)[0] if "." in parts[0] else parts[0]
            hits.append(_hit(name, parts[1], parts[2] if len(parts) > 2 else None))
        else:
            # `apk search -v` prints `name-version - description`; the version
            # starts at the first `-<digit>`, because both parts contain hyphens.
            head, _, summary = line.partition(" - ")
            match = APK_LINE.match(head.strip())
            if match:
                hits.append(
                    _hit(match.group("name"), match.group("version"), summary.strip() or None)
                )
            else:
                hits.append(_hit(head.strip(), None, summary.strip() or None))
        if len(hits) >= limit:
            break
    return hits


@router.post("/v1/catalogue")
def catalogue(body: CatalogueBody) -> dict[str, Any]:
    """Search one baseline's native package index."""
    if body.package_manager not in MANAGERS:
        raise HTTPException(422, "package_manager must be apt, dnf or apk")
    if not QUERY.match(body.query):
        raise HTTPException(422, "the query is not a package-name fragment")
    key = (body.image, body.package_manager, body.query)
    now = time.time()
    cached = _CACHE.get(key)
    if cached and now - cached[0] < CACHE_TTL_SECONDS:
        return {**cached[1], "cached": True}
    code, output = _run(body.image, SEARCH_SCRIPT[body.package_manager], {"NESTBOX_QUERY": body.query})
    if code != 0:
        answer = {
            "items": [],
            "available": False,
            "detail": f"the {body.package_manager} index answered with exit code {code}",
        }
    else:
        answer = {
            "items": parse_search(body.package_manager, output),
            "available": True,
            "detail": f"live {body.package_manager} index of {body.image}",
        }
    if len(_CACHE) >= CACHE_LIMIT:
        _CACHE.clear()
    _CACHE[key] = (now, answer)
    return {**answer, "cached": False}


@router.post("/v1/images/verify")
def verify(body: VerifyBody) -> dict[str, Any]:
    """Check the packages a recipe asked for are in the built image.

    The image's own package database answers, so a Dockerfile that ran but
    installed nothing is caught here rather than by an agent that finds a tool
    missing later.
    """
    if body.package_manager not in MANAGERS:
        raise HTTPException(422, "package_manager must be apt, dnf or apk")
    wanted = []
    for package in body.packages:
        # A pin (`name=1.2`) is about the index, not about what is installed.
        name = package.split("=", 1)[0].strip()
        if name and name not in wanted:
            wanted.append(name)
    code, output = _run(body.tag, LIST_SCRIPT[body.package_manager], {})
    if code != 0:
        return {
            "present": [],
            "missing": wanted,
            "note": f"the image's package database could not be read (exit code {code})",
        }
    installed: set[str] = set()
    for line in output.splitlines():
        line = line.strip()
        if not line:
            continue
        name = line.split()[0]
        installed.add(name.split(":", 1)[0])
        installed.add(name)
        installed.add(name.split("=", 1)[0] if "=" in name else name)
    present = [name for name in wanted if name in installed]
    missing = [name for name in wanted if name not in installed]
    return {"present": present, "missing": missing, "note": None}
