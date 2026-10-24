"""Network admin: certificates, DNS records, proxy routes and the network profile."""
from __future__ import annotations

from django.contrib import admin
from django.utils.html import format_html
from unfold.admin import ModelAdmin

from apps.network.models import Certificate, DnsRecord, NetworkProfile, ProxyRoute


@admin.register(Certificate)
class CertificateAdmin(ModelAdmin):
    list_display = ["fqdn", "source", "status", "issuer", "expires_at", "updated_at"]
    list_filter = ["source", "status"]
    search_fields = ["fqdn", "issuer"]
    readonly_fields = ["created_at", "updated_at"]
    list_filter_submit = True
    fieldsets = [
        ("Certificate", {"fields": ["fqdn", "source", "issuer", "status", "expires_at"]}),
        ("Material", {"fields": ["cert_pem", "chain_pem", "key_pem_encrypted"]}),
        ("Status", {"fields": ["last_error", "created_at", "updated_at"]}),
    ]


@admin.register(DnsRecord)
class DnsRecordAdmin(ModelAdmin):
    list_display = ["zone", "name", "rtype", "value", "ttl", "created_at"]
    list_filter = ["zone", "rtype"]
    search_fields = ["zone", "name", "value"]
    readonly_fields = ["created_at"]
    list_filter_submit = True


@admin.register(ProxyRoute)
class ProxyRouteAdmin(ModelAdmin):
    list_display = ["match", "target", "tls_profile", "is_active", "created_at"]
    list_filter = ["is_active", "tls_profile"]
    search_fields = ["match", "target"]
    readonly_fields = ["created_at"]
    list_filter_submit = True


@admin.register(NetworkProfile)
class NetworkProfileAdmin(ModelAdmin):
    list_display = ["name", "mode", "interface", "is_active", "last_applied_at", "updated_at"]
    list_filter = ["mode", "is_active"]
    search_fields = ["name", "interface", "address"]
    readonly_fields = ["last_applied_at", "last_good_yaml", "created_at", "updated_at"]
    list_filter_submit = True
    fieldsets = [
        ("Profile", {"fields": ["name", "mode", "interface", "is_active"]}),
        ("Addressing", {"fields": ["address", "gateway"]}),
        ("DNS", {"fields": ["dns_servers", "dns_domain", "dns_search"]}),
        ("Time", {"fields": ["ntp_enabled", "ntp_servers"]}),
        ("Last applied", {"fields": ["last_applied_at", "last_good_yaml"]}),
        ("Metadata", {"fields": ["created_at", "updated_at"]}),
    ]


# Kept for callers that used the helper in older code; the admin no longer renders a bespoke
# theme, so status colour is expressed with Unfold's boolean display instead.
def active_badge(is_active: bool) -> str:
    return format_html("{}", "Active" if is_active else "Inactive")
