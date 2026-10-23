"""Certificate lifecycle: internal CA, ACME, uploaded pairs.

Two honest constraints shape this module:

* a certificate cannot be issued at image build time, because the machine has no address and the
  DNS zone is unknown — issuance happens on first boot or on demand;
* Let's Encrypt needs either a reachable port 80 (HTTP-01) or DNS API access (DNS-01), and a
  failure is usually a DNS record that has not propagated yet. The UI must say so instead of
  showing a generic error.
"""
from __future__ import annotations

import datetime as dt
import logging
import shutil
import subprocess
from dataclasses import dataclass

from cryptography import x509
from cryptography.hazmat.primitives import hashes, serialization
from cryptography.hazmat.primitives.asymmetric import rsa
from cryptography.x509.oid import NameOID
from django.utils import timezone

from apps.core.crypto import decrypt, encrypt
from apps.network.models import Certificate

logger = logging.getLogger(__name__)

RENEWAL_WINDOW_DAYS = 30


@dataclass(frozen=True)
class IssuedPair:
    cert_pem: str
    key_pem: str
    expires_at: dt.datetime


def issue_self_signed(fqdn: str, *, days: int = 3650, organisation: str = "Nestbox internal CA") -> IssuedPair:
    """Generate a self-signed pair. Instant, offline, no DNS dependency."""
    key = rsa.generate_private_key(public_exponent=65537, key_size=2048)
    subject = issuer = x509.Name(
        [
            x509.NameAttribute(NameOID.ORGANIZATION_NAME, organisation),
            x509.NameAttribute(NameOID.COMMON_NAME, fqdn),
        ]
    )
    now = dt.datetime.now(dt.UTC)
    san = x509.SubjectAlternativeName([x509.DNSName(fqdn)])
    cert = (
        x509.CertificateBuilder()
        .subject_name(subject)
        .issuer_name(issuer)
        .public_key(key.public_key())
        .serial_number(x509.random_serial_number())
        .not_valid_before(now - dt.timedelta(minutes=5))
        .not_valid_after(now + dt.timedelta(days=days))
        .add_extension(san, critical=False)
        .add_extension(x509.BasicConstraints(ca=True, path_length=0), critical=True)
        .sign(key, hashes.SHA256())
    )
    return IssuedPair(
        cert_pem=cert.public_bytes(serialization.Encoding.PEM).decode(),
        key_pem=key.private_bytes(
            serialization.Encoding.PEM,
            serialization.PrivateFormat.TraditionalOpenSSL,
            serialization.NoEncryption(),
        ).decode(),
        expires_at=cert.not_valid_after_utc,
    )


def store_certificate(
    fqdn: str,
    *,
    source: str,
    cert_pem: str,
    key_pem: str,
    chain_pem: str = "",
    issuer: str = "",
) -> Certificate:
    cert, _ = Certificate.objects.update_or_create(
        fqdn=fqdn,
        defaults={
            "cert_pem": cert_pem,
            "chain_pem": chain_pem,
            "key_pem_encrypted": encrypt(key_pem),
            "source": source,
            "issuer": issuer,
            "status": "valid",
            "expires_at": parse_expiry(cert_pem),
            "last_error": "",
        },
    )
    return cert


def parse_expiry(cert_pem: str) -> dt.datetime | None:
    try:
        cert = x509.load_pem_x509_certificate(cert_pem.encode())
        return cert.not_valid_after_utc
    except Exception:  # noqa: BLE001
        return None


def refresh_status(cert: Certificate) -> Certificate:
    if cert.expires_at is None:
        cert.expires_at = parse_expiry(cert.cert_pem)
    if cert.expires_at:
        days = (cert.expires_at - timezone.now()).days
        if days < 0:
            cert.status = "expired"
        elif days <= RENEWAL_WINDOW_DAYS:
            cert.status = "expiring"
        else:
            cert.status = "valid"
        cert.save(update_fields=["status", "expires_at", "updated_at"])
    return cert


def days_left(cert: Certificate) -> int | None:
    if not cert.expires_at:
        return None
    return (cert.expires_at - timezone.now()).days


def acme_available() -> bool:
    return bool(shutil.which("certbot") or shutil.which("lego"))


def request_letsencrypt(cert: Certificate, *, email: str, challenge: str, webroot: str = "/var/www/acme") -> dict:
    """Issue or renew through ACME.

    Refuses honestly when no ACME client is installed, because a silent failure here produces a
    broken HTTPS site that looks like a code bug.
    """
    if not acme_available():
        cert.status = "failed"
        cert.last_error = "No ACME client installed (certbot or lego)."
        cert.save(update_fields=["status", "last_error", "updated_at"])
        return {"ok": False, "code": "tls.acme_client_missing", "hint": "Install certbot on the appliance."}

    if not cert.fqdn or "." not in cert.fqdn:
        cert.status = "failed"
        cert.last_error = "FQDN is not a public hostname."
        cert.save(update_fields=["status", "last_error", "updated_at"])
        return {"ok": False, "code": "tls.invalid_fqdn"}

    cmd = [
        "certbot", "certonly", "--non-interactive", "--agree-tos",
        "-d", cert.fqdn, "--email", email or f"admin@{cert.fqdn}",
    ]
    if challenge == "http01":
        cmd += ["--webroot", "-w", webroot]
    else:
        # DNS-01 needs a provider plugin and credentials; refuse rather than guess.
        cert.status = "failed"
        cert.last_error = "DNS-01 requires a provider plugin and credentials for this zone."
        cert.save(update_fields=["status", "last_error", "updated_at"])
        return {
            "ok": False,
            "code": "tls.dns01_not_configured",
            "hint": "Add the zone's API token, or switch to HTTP-01 and open port 80.",
        }

    try:
        result = subprocess.run(cmd, capture_output=True, text=True, timeout=180)  # noqa: S603
    except subprocess.TimeoutExpired:
        cert.status = "failed"
        cert.last_error = "ACME request timed out."
        cert.save(update_fields=["status", "last_error", "updated_at"])
        return {"ok": False, "code": "tls.acme_timeout"}

    if result.returncode != 0:
        cert.status = "failed"
        cert.last_error = (result.stderr or result.stdout)[-400:]
        cert.save(update_fields=["status", "last_error", "updated_at"])
        return {
            "ok": False,
            "code": "tls.acme_failed",
            "hint": "Check that the DNS record points at this host and port 80 is reachable.",
            "stderr": cert.last_error,
        }

    live = f"/etc/letsencrypt/live/{cert.fqdn}"
    try:
        cert_pem = open(f"{live}/fullchain.pem").read()  # noqa: PTH123, SIM115
        key_pem = open(f"{live}/privkey.pem").read()  # noqa: PTH123, SIM115
    except OSError as exc:
        cert.status = "failed"
        cert.last_error = str(exc)[:400]
        cert.save(update_fields=["status", "last_error", "updated_at"])
        return {"ok": False, "code": "tls.read_failed", "hint": str(exc)}

    store_certificate(cert.fqdn, source="letsencrypt", cert_pem=cert_pem, key_pem=key_pem,
                      issuer="Let's Encrypt")
    cert.refresh_from_db()
    return {"ok": True, "certificate": cert.fqdn, "expires_at": cert.expires_at.isoformat()}


def renew_due() -> list[dict]:
    """Called by the scheduler. Renews everything inside the renewal window."""
    renewed: list[dict] = []
    candidates = Certificate.objects.filter(source="letsencrypt")
    for cert in candidates:
        refresh_status(cert)
        if cert.status in {"expiring", "expired"}:
            renewed.append(request_letsencrypt(cert, email="", challenge="http01"))
    return renewed


def private_key_pem(cert: Certificate) -> str:
    return decrypt(cert.key_pem_encrypted)


def default_certificate_for(hostname: str) -> Certificate:
    cert = Certificate.objects.filter(fqdn=hostname).first()
    if cert:
        return cert
    pair = issue_self_signed(hostname)
    return store_certificate(hostname, source="internal_ca", cert_pem=pair.cert_pem,
                             key_pem=pair.key_pem, issuer="Nestbox internal CA")
