"""Certificates, DNS records, proxy routes and the network profile."""
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
    ("pending", "pending"),
    ("failed", "failed"),
]

CERT_SOURCES = [
    ("internal_ca", "internal_ca"),
    ("letsencrypt", "letsencrypt"),
    ("upload", "upload"),
]

NETWORK_MODES = [
    ("dhcp", "dhcp"),
    ("static", "static"),
]


class Certificate(models.Model):
    """A certificate pair. The private key is stored encrypted."""

    fqdn = models.CharField(max_length=255, unique=True)
    cert_pem = models.TextField(blank=True)
    chain_pem = models.TextField(blank=True)
    key_pem_encrypted = models.TextField(blank=True)
    source = models.CharField(max_length=20, choices=CERT_SOURCES, default="internal_ca")
    issuer = models.CharField(max_length=120, blank=True)
    status = models.CharField(max_length=10, choices=CERT_STATUS, default="pending")
    expires_at = models.DateTimeField(null=True, blank=True)
    last_error = models.CharField(max_length=400, blank=True)
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
            models.UniqueConstraint(fields=["zone", "name", "rtype"], name="uniq_dns_record"),
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


class NetworkProfile(models.Model):
    """Desired host network state.

    Changes are applied through a safe-apply service that reverts automatically when the
    controller stops answering, because a wrong address or gateway makes the box unreachable.
    """

    name = models.CharField(max_length=100, unique=True)
    mode = models.CharField(max_length=10, choices=NETWORK_MODES, default="dhcp")
    interface = models.CharField(max_length=40, default="all-en")
    address = models.CharField(max_length=64, blank=True)
    gateway = models.CharField(max_length=64, blank=True)
    dns_servers = models.JSONField(default=list, blank=True)
    dns_domain = models.CharField(max_length=255, blank=True)
    dns_search = models.JSONField(default=list, blank=True)
    ntp_enabled = models.BooleanField(default=True)
    ntp_servers = models.JSONField(default=list, blank=True)
    is_active = models.BooleanField(default=True)
    last_applied_at = models.DateTimeField(null=True, blank=True)
    last_good_yaml = models.TextField(blank=True)
    created_at = models.DateTimeField(auto_now_add=True)
    updated_at = models.DateTimeField(auto_now=True)

    class Meta:
        ordering = ["name"]

    def __str__(self) -> str:
        return self.name
