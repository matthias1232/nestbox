"""Backups of persistent roots: a gzip tar per backup, checked before it is trusted.

A root is a directory under the persistent base; a backup is one `<id>.tar.gz` under the
backup base. Two rules carry the safety:

* **Paths are validated against their base, twice.** The control plane derives them, and
  this module checks again that a root path is under the persistent base and an archive
  path is under the backup base, because a path that came over HTTP is a path to refuse
  when it points elsewhere.
* **A restore verifies, then extracts safely.** The SHA-256 recorded at backup time must
  match before anything is written, and the extraction refuses absolute names, `..`,
  links that leave the root and device files. With `replace`, what is there now is moved
  aside (never deleted) so a wrong restore can be undone by hand.
"""
from __future__ import annotations

import hashlib
import os
import shutil
import tarfile
import time
import uuid
from pathlib import Path

from fastapi import APIRouter, HTTPException
from pydantic import BaseModel, Field

router = APIRouter()

PERSISTENT_BASE = os.path.realpath(os.getenv("NESTBOX_PERSISTENT_BASE", "/var/lib/nestbox/persistent"))
BACKUP_BASE = os.path.realpath(os.getenv("NESTBOX_BACKUP_BASE", "/var/lib/nestbox/backups"))


def _inside(path: str, base: str) -> bool:
    real = os.path.realpath(path)
    return real == base or real.startswith(base + os.sep)


def _root(path: str) -> str:
    real = os.path.realpath(path)
    if real == PERSISTENT_BASE or not _inside(real, PERSISTENT_BASE):
        raise HTTPException(422, "the root is outside the persistent base")
    return real


def _archive(backup_id: str) -> str:
    try:
        uuid.UUID(backup_id)
    except ValueError as exc:
        raise HTTPException(422, "a backup id is a uuid") from exc
    return os.path.join(BACKUP_BASE, f"{backup_id}.tar.gz")


def sha256_of(path: str) -> str:
    digest = hashlib.sha256()
    with open(path, "rb") as handle:
        for block in iter(lambda: handle.read(1 << 20), b""):
            digest.update(block)
    return digest.hexdigest()


def create_archive(root: str, archive: str) -> int:
    """Write `root` into `archive`; returns the archive size."""
    os.makedirs(os.path.dirname(archive), exist_ok=True)
    partial = archive + ".partial"
    with tarfile.open(partial, "w:gz") as tar:
        for entry in sorted(os.listdir(root)):
            tar.add(os.path.join(root, entry), arcname=entry, recursive=True)
    os.replace(partial, archive)
    return os.path.getsize(archive)


def _safe(member: tarfile.TarInfo, root: str) -> bool:
    name = member.name
    if name.startswith("/") or ".." in Path(name).parts:
        return False
    if member.isdev() or member.isfifo():
        return False
    target = os.path.realpath(os.path.join(root, name))
    if not (target == root or target.startswith(root + os.sep)):
        return False
    if member.issym() or member.islnk():
        link = member.linkname
        resolved = os.path.realpath(os.path.join(os.path.dirname(target), link)) if not link.startswith("/") else os.path.realpath(link)
        if not (resolved == root or resolved.startswith(root + os.sep)):
            return False
    return True


def extract_archive(archive: str, root: str) -> int:
    """Extract into `root`, refusing anything that would leave it. Returns file count."""
    count = 0
    with tarfile.open(archive, "r:gz") as tar:
        members = tar.getmembers()
        for member in members:
            if not _safe(member, root):
                raise HTTPException(422, f"the archive holds an unsafe entry: {member.name}")
        for member in members:
            tar.extract(member, root, filter="data")
            count += 1
    return count


class BackupRequest(BaseModel):
    root_path: str
    backup_id: str


@router.post("/v1/backups")
def create_backup(body: BackupRequest) -> dict[str, object]:
    root = _root(body.root_path)
    if not os.path.isdir(root):
        raise HTTPException(404, "the root directory does not exist")
    archive = _archive(body.backup_id)
    try:
        size = create_archive(root, archive)
    except OSError as exc:
        raise HTTPException(500, f"the backup could not be written: {exc}") from exc
    return {"backup_id": body.backup_id, "bytes": size, "sha256": sha256_of(archive)}


class RestoreRequest(BaseModel):
    root_path: str
    sha256: str = Field(pattern=r"^[0-9a-f]{64}$")
    replace: bool = False


@router.post("/v1/backups/{backup_id}/restore")
def restore_backup(backup_id: str, body: RestoreRequest) -> dict[str, object]:
    archive = _archive(backup_id)
    if not os.path.isfile(archive):
        raise HTTPException(404, "the archive is not on this node")
    if sha256_of(archive) != body.sha256:
        raise HTTPException(409, "the archive does not match its recorded checksum; nothing was restored")
    root = _root(body.root_path)
    os.makedirs(root, exist_ok=True)
    moved_to = None
    if body.replace and os.listdir(root):
        moved_to = os.path.join(BACKUP_BASE, f"replaced-{int(time.time())}-{backup_id[:8]}")
        os.makedirs(moved_to, exist_ok=True)
        for entry in os.listdir(root):
            shutil.move(os.path.join(root, entry), os.path.join(moved_to, entry))
    try:
        files = extract_archive(archive, root)
    except HTTPException:
        raise
    except (tarfile.TarError, OSError) as exc:
        raise HTTPException(500, f"the restore failed: {exc}") from exc
    return {"restored": files, "previous_content_kept_at": moved_to}


@router.delete("/v1/backups/{backup_id}")
def delete_backup(backup_id: str) -> dict[str, object]:
    archive = _archive(backup_id)
    existed = os.path.isfile(archive)
    if existed:
        os.unlink(archive)
    return {"deleted": existed}
