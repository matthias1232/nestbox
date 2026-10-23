"""Symmetric encryption for secrets at rest.

Uses Fernet with a key derived from the application secret, so a database dump alone does not
expose provider tokens, the bootstrap admin password, or certificate private keys.
"""
from __future__ import annotations

import base64
import hashlib

from cryptography.fernet import Fernet, InvalidToken
from django.conf import settings

_PREFIX = "enc:v1:"


def _fernet() -> Fernet:
    digest = hashlib.sha256(f"nestbox:{settings.SECRET_KEY}".encode()).digest()
    return Fernet(base64.urlsafe_b64encode(digest))


def encrypt(plaintext: str) -> str:
    if not plaintext:
        return ""
    return _PREFIX + _fernet().encrypt(plaintext.encode()).decode()


def decrypt(ciphertext: str) -> str:
    if not ciphertext:
        return ""
    if not ciphertext.startswith(_PREFIX):
        # Legacy plaintext value: return as-is so a migration can re-encrypt it.
        return ciphertext
    try:
        return _fernet().decrypt(ciphertext[len(_PREFIX):].encode()).decode()
    except InvalidToken:
        return ""


def is_encrypted(value: str) -> bool:
    return bool(value) and value.startswith(_PREFIX)
