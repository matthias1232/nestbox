"""Certificates, DNS records, proxy routes."""
from __future__ import annotations

from django.db import models

DNS_TYPES = [
    ("A", "A"),
    ("AAAA", "AAAA"),
    ("CNAME", "CNAME"),
    ("TXT", "TXT"),
]

CERT_STATUS = [
    ("valid", "valid"),
    ("expiring", "expiring"),
    ("expired", "expired"),
    ("revoked", "revoked"),
]


class Certificate(models.Model):
    """A certificate pair. The private key is stored encrypted by the application layer."""

    fqdn = models.CharField(max_length=255, unique=True)
    cert_pem = models.TextField()
    key_pem_encrypted = models.TextField()
    issuer = models.CharField(max_length=120, blank=True)
    status = models.CharField(max_length=10, choices=CERT_STATUS, default="valid")
    expires_at = models.DateTimeField(null=True, blank=True)
    created_at = models.DateTimeField(auto_now_add=True)
    updated_at = models.DateTimeField(auto_now=True)

    class Meta:
        ordering = ["fqdn"]

    def __str__(self) -> str:
        return self.fqdn


class DnsRecord(models.Model):
    zone = models.CharField(max_length=255)
    name = models.CharField(max_length=255)
    rtype = models.CharField(max_length=10, choices=DNS_TYPES, default="A")
    value = models.CharField(max_length=500)
    ttl = models.PositiveIntegerField(default=300)
    created_at = models.DateTimeField(auto_now_add=True)

    class Meta:
        ordering = ["zone", "name"]
        constraints = [
            models.UniqueConstraint(
                fields=["zone", "name", "rtype"], name="uniq_dns_record"
            ),
        ]

    def __str__(self) -> str:
        return f"{self.name}.{self.zone} {self.rtype}"


class ProxyRoute(models.Model):
    match = models.CharField(max_length=255, help_text="Host or path prefix, e.g. hermes-01.apps.lab")
    target = models.CharField(max_length=300, help_text="Upstream, e.g. 100.100.0.12:8080")
    tls_profile = models.CharField(max_length=60, default="default")
    is_active = models.BooleanField(default=True)
    created_at = models.DateTimeField(auto_now_add=True)

    class Meta:
        ordering = ["match"]

    def __str__(self) -> str:
        return f"{self.match} -> {self.target}"
