"""The images this node may start, and nothing else.

One module, because two callers need the same answer: the sandbox adapter and
the package catalogue, which starts a baseline image to ask its package index a
question. A second copy of this rule would be a second allowlist to keep in
step, and the one that drifted would be the hole.
"""
from __future__ import annotations

import os

ALLOWED_IMAGES = set(
    os.getenv(
        "NESTBOX_ALLOWED_IMAGES",
        "ubuntu:24.04,debian:12-slim,rockylinux:9,fedora:41,alpine:3.21,"
        "nestbox-ubuntu-dev:local,nestbox-hermes:local,nestbox-openclaw:local",
    ).split(",")
)

# Images the runner built itself from a recipe are always allowed: they carry
# this prefix and a content hash, and the Dockerfile came from the control plane.
RECIPE_IMAGE_PREFIX = "nestbox-recipe/"


def image_allowed(image: str) -> bool:
    """Whether `image` may be started on this node."""
    return image in ALLOWED_IMAGES or image.startswith(RECIPE_IMAGE_PREFIX)
