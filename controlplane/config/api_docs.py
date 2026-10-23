"""drf-spectacular configuration.

The OpenAPI schema is a product surface, not an afterthought: the schema is frozen in CI, the
generated client is built from it, and Swagger UI / Redoc are shipped with the appliance.
"""
from __future__ import annotations

from pathlib import Path

REPO_DIR = Path(__file__).resolve().parent.parent.parent

_OVERVIEW = REPO_DIR / "docs" / "api" / "overview.md"
DESCRIPTION = (
    _OVERVIEW.read_text(encoding="utf-8")
    if _OVERVIEW.exists()
    else "Nestbox control plane API."
)

SPECTACULAR_SETTINGS = {
    "TITLE": "Nestbox API",
    "DESCRIPTION": DESCRIPTION,
    "VERSION": "1.0.0",
    "LICENSE": {
        "name": "Apache-2.0",
        "url": "https://www.apache.org/licenses/LICENSE-2.0",
    },
    "CONTACT": {"name": "Matthias Binder", "url": "https://mmbits.com"},
    "TAGS": [
        {"name": "system", "description": "Health, version, capability probe."},
        {"name": "identity", "description": "Tokens, current user, workspaces, languages."},
        {"name": "containers", "description": "Container inventory and lifecycle."},
        {"name": "sandboxes", "description": "OpenShell sandboxes (nests): lifecycle, exec, services."},
        {"name": "templates", "description": "Reusable sandbox templates."},
        {"name": "policies", "description": "Filesystem, process and network policy (the AI firewall)."},
        {"name": "agent-packs", "description": "Installable agent definitions."},
        {"name": "providers", "description": "Inference and service-provider profiles."},
        {"name": "secrets", "description": "Encrypted secret slots."},
        {"name": "network", "description": "Certificates, DNS records, overlay peers, proxy routes."},
        {"name": "builds", "description": "ISO build recipes and jobs."},
        {"name": "backups", "description": "Backup targets, snapshots and restores."},
        {"name": "audit", "description": "Audit trail."},
    ],
    "SERVE_PUBLIC": True,
    "SERVE_INCLUDE_SCHEMA": False,
    "SERVE_AUTHENTICATION": [
        "rest_framework.authentication.TokenAuthentication",
        "rest_framework.authentication.SessionAuthentication",
    ],
    "SCHEMA_PATH_PREFIX": r"/api/v1",
    "SCHEMA_PATH_PREFIX_TRIM": False,
    "SORT_OPERATIONS": True,
    "SORT_OPERATION_PARAMETERS": True,
    "COMPONENT_SPLIT_REQUEST": True,
    "COMPONENT_SPLIT_PATCH": True,
    "ENUM_NAME_OVERRIDES": {
        "ContainerStateEnum": "apps.containers.models.CONTAINER_STATES",
        "ContainerRoleEnum": "apps.containers.models.CONTAINER_ROLES",
        "SandboxStateEnum": "apps.sandboxes.models.SANDBOX_STATES",
        "BuildStateEnum": "apps.builds.models.BUILD_STATES",
        "BuildProfileEnum": "apps.builds.models.BUILD_PROFILES",
        "NetworkModeEnum": "apps.builds.models.NETWORK_MODES",
        "PolicyScopeEnum": "apps.policies.models.POLICY_SCOPES",
        "NetworkAccessEnum": "apps.policies.models.NETWORK_ACCESS",
        "NetworkEnforcementEnum": "apps.policies.models.NETWORK_ENFORCEMENT",
        "FilesystemModeEnum": "apps.policies.models.FILESYSTEM_MODES",
        "BackupKindEnum": "apps.backups.models.BACKUP_KINDS",
        "AuditActionEnum": "apps.identity.models.AUDIT_ACTIONS",
    },
    "POSTPROCESSING_HOOKS": ["apps.api.schema_hooks.annotate_operations"],
    "APPEND_COMPONENTS": {
        "schemas": {
        "ErrorEnvelope": {
            "type": "object",
            "required": ["error"],
            "properties": {
                "error": {
                    "type": "object",
                    "required": ["code", "message"],
                    "properties": {
                        "code": {
                            "type": "string",
                            "description": "Stable machine-readable identifier. The UI translates this.",
                            "example": "sandbox.not_found",
                        },
                        "message": {
                            "type": "string",
                            "description": "English developer message. Never translated.",
                        },
                        "details": {"type": "object", "additionalProperties": True},
                        "request_id": {"type": "string", "format": "uuid"},
                    },
                }
            },
        }
        }
    },
}

SPECTACULAR_SWAGGER_UI_SETTINGS = {
    "persistAuthorization": True,
    "tryItOutEnabled": True,
    "displayRequestDuration": True,
    "docExpansion": "list",
    "defaultModelsExpandDepth": 1,
    "filter": True,
}
SPECTACULAR_REDOC_SETTINGS = {
    "expandResponses": "200,201",
    "jsonSampleExpandLevel": 2,
    "hideDownloadButton": False,
}
