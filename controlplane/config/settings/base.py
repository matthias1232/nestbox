"""Base settings for Nestbox.

Every deployment-specific value comes from the environment; the defaults are safe for a
developer machine and deliberately not safe for production. See ``dev.py`` and ``prod.py``.
"""
from __future__ import annotations

import os
from pathlib import Path

from config.api_docs import SPECTACULAR_SETTINGS

BASE_DIR = Path(__file__).resolve().parent.parent.parent
REPO_DIR = BASE_DIR.parent

SECRET_KEY = os.environ.get("NESTBOX_SECRET_KEY", "dev-insecure-key-change-me")
DEBUG = False
ALLOWED_HOSTS = [h for h in os.environ.get("NESTBOX_ALLOWED_HOSTS", "*").split(",") if h]
CSRF_TRUSTED_ORIGINS = [
    o for o in os.environ.get("NESTBOX_CSRF_TRUSTED_ORIGINS", "").split(",") if o
]

# The module catalog decides which apps are installed at all. The enabled set comes from
# /var/lib/nestbox/active-modules.json (written by first boot and by the admin) with
# NESTBOX_MODULES as the seed value. See config/modules.py.
from config.modules import CATALOG, active_keys  # noqa: E402

NESTBOX_ACTIVE_MODULES = active_keys()
NESTBOX_MODULE_APPS = [CATALOG[key].app for key in NESTBOX_ACTIVE_MODULES]

INSTALLED_APPS = [
    "unfold",
    "unfold.contrib.filters",
    "unfold.contrib.forms",
    "unfold.contrib.inlines",
    "django.contrib.admin",
    "django.contrib.auth",
    "django.contrib.contenttypes",
    "django.contrib.sessions",
    "django.contrib.messages",
    "django.contrib.staticfiles",
    "django.contrib.sites",
    "django.contrib.humanize",
    "rest_framework",
    "rest_framework.authtoken",
    "drf_spectacular",
    "allauth",
    "allauth.account",
    "allauth.mfa",
    "axes",
    *NESTBOX_MODULE_APPS,
]

# --- admin theme ------------------------------------------------------------------------------
UNFOLD = {
    "SITE_TITLE": "Nestbox",
    "SITE_HEADER": "Nestbox",
    "SITE_SUBHEADER": "AI agent appliance",
    "SITE_URL": "/",
    "SITE_SYMBOL": "deployed_code",
    "SHOW_HISTORY": True,
    "SHOW_VIEW_ON_SITE": True,
    "DASHBOARD_CALLBACK": "apps.core.admin_dashboard.dashboard_callback",
    "BORDER_RADIUS": "10px",
    "COLORS": {
        "primary": {
            "50": "236 254 255",
            "100": "207 250 254",
            "200": "165 243 252",
            "300": "103 232 249",
            "400": "34 211 238",
            "500": "6 182 212",
            "600": "8 145 178",
            "700": "14 116 144",
            "800": "21 94 117",
            "900": "22 78 99",
            "950": "8 51 68",
        },
    },
    "STYLES": ["css/admin.css"],
    "NAVIGATION": [
        {"title": "Operate", "separator": True, "items": [
            {"title": "Dashboard", "icon": "dashboard", "link": "/"},
            {"title": "Containers", "icon": "deployed_code", "link": "/admin/core/container/"},
            {"title": "Nests", "icon": "inventory_2", "link": "/admin/sandboxes/sandbox/"},
            {"title": "Templates", "icon": "dashboard_customize", "link": "/admin/sandboxes/sandboxtemplate/"},
        ]},
        {"title": "Govern", "separator": True, "items": [
            {"title": "Policies", "icon": "policy", "link": "/admin/policies/policy/"},
            {"title": "Network rules", "icon": "lan", "link": "/admin/policies/networkrule/"},
            {"title": "Certificates", "icon": "verified_user", "link": "/admin/network/certificate/"},
            {"title": "Network profiles", "icon": "settings_ethernet", "link": "/admin/network/networkprofile/"},
        ]},
        {"title": "Build", "separator": True, "items": [
            {"title": "Recipes", "icon": "build", "link": "/admin/builds/buildrecipe/"},
            {"title": "Jobs", "icon": "pending_actions", "link": "/admin/builds/buildjob/"},
            {"title": "Agent packs", "icon": "extension", "link": "/admin/agentpacks/agentpack/"},
        ]},
        {"title": "Platform", "separator": True, "items": [
            {"title": "Gateway", "icon": "hub", "link": "/admin/openshell/gateway/"},
            {"title": "Backups", "icon": "backup", "link": "/admin/backups/backuptarget/"},
            {"title": "Audit", "icon": "history", "link": "/admin/identity/auditevent/"},
            {"title": "Users", "icon": "group", "link": "/admin/auth/user/"},
            {"title": "API docs", "icon": "api", "link": "/api/docs/"},
        ]},
    ],
}

MIDDLEWARE = [
    "django.middleware.security.SecurityMiddleware",
    "whitenoise.middleware.WhiteNoiseMiddleware",
    "django.contrib.sessions.middleware.SessionMiddleware",
    "django.middleware.locale.LocaleMiddleware",
    "django.middleware.common.CommonMiddleware",
    "django.middleware.csrf.CsrfViewMiddleware",
    "django.contrib.auth.middleware.AuthenticationMiddleware",
    "django.contrib.messages.middleware.MessageMiddleware",
    "django.middleware.clickjacking.XFrameOptionsMiddleware",
    "allauth.account.middleware.AccountMiddleware",
    "axes.middleware.AxesMiddleware",
]

ROOT_URLCONF = "config.urls"
WSGI_APPLICATION = "config.wsgi.application"
ASGI_APPLICATION = "config.asgi.application"

TEMPLATES = [
    {
        "BACKEND": "django.template.backends.django.DjangoTemplates",
        "DIRS": [BASE_DIR / "templates"],
        "APP_DIRS": True,
        "OPTIONS": {
            "context_processors": [
                "django.template.context_processors.debug",
                "django.template.context_processors.request",
                "django.contrib.auth.context_processors.auth",
                "django.contrib.messages.context_processors.messages",
            ],
        },
    },
]

DATABASES = {
    "default": {
        "ENGINE": "django.db.backends.sqlite3",
        "NAME": os.environ.get("NESTBOX_DB_PATH", BASE_DIR / "db.sqlite3"),
        "OPTIONS": {"timeout": 20},
    }
}

AUTHENTICATION_BACKENDS = [
    "axes.backends.AxesStandaloneBackend",
    "django.contrib.auth.backends.ModelBackend",
    "allauth.account.auth_backends.AuthenticationBackend",
]

AUTH_PASSWORD_VALIDATORS = [
    {"NAME": "django.contrib.auth.password_validation.UserAttributeSimilarityValidator"},
    {"NAME": "django.contrib.auth.password_validation.MinimumLengthValidator"},
    {"NAME": "django.contrib.auth.password_validation.CommonPasswordValidator"},
    {"NAME": "django.contrib.auth.password_validation.NumericPasswordValidator"},
]

PASSWORD_HASHERS = [
    "django.contrib.auth.hashers.Argon2PasswordHasher",
    "django.contrib.auth.hashers.PBKDF2PasswordHasher",
]

SITE_ID = 1

# --- i18n -------------------------------------------------------------------------------------
# English is the source language: there is no locale/en catalog, msgids are English.
LANGUAGE_CODE = "en"
LANGUAGES = [("en", "English")]
LOCALE_PATHS = [BASE_DIR / "locale"]
USE_I18N = True
USE_TZ = True
TIME_ZONE = "UTC"
LANGUAGE_COOKIE_NAME = "nestbox_language"
LANGUAGE_COOKIE_AGE = 60 * 60 * 24 * 30
LANGUAGE_COOKIE_SAMESITE = "Lax"

# --- static files -----------------------------------------------------------------------------
STATIC_URL = "/static/"
STATIC_ROOT = BASE_DIR / "staticfiles"
STATICFILES_DIRS = [BASE_DIR / "static"]
STORAGES = {
    "default": {"BACKEND": "django.core.files.storage.FileSystemStorage"},
    "staticfiles": {"BACKEND": "whitenoise.storage.CompressedManifestStaticFilesStorage"},
}
MEDIA_URL = "/media/"
MEDIA_ROOT = Path(os.environ.get("NESTBOX_MEDIA_ROOT", BASE_DIR / "media"))

DEFAULT_AUTO_FIELD = "django.db.models.BigAutoField"

# --- REST framework ---------------------------------------------------------------------------
REST_FRAMEWORK = {
    "DEFAULT_AUTHENTICATION_CLASSES": [
        "rest_framework.authentication.TokenAuthentication",
        "rest_framework.authentication.SessionAuthentication",
    ],
    "DEFAULT_PERMISSION_CLASSES": ["rest_framework.permissions.IsAuthenticated"],
    "DEFAULT_SCHEMA_CLASS": "drf_spectacular.openapi.AutoSchema",
    "DEFAULT_PAGINATION_CLASS": "apps.api.pagination.StandardPagination",
    "PAGE_SIZE": 50,
    "DEFAULT_THROTTLE_CLASSES": [
        "rest_framework.throttling.ScopedRateThrottle",
    ],
    "DEFAULT_THROTTLE_RATES": {"destructive": "30/min", "stream": "120/min"},
    "EXCEPTION_HANDLER": "apps.api.errors.exception_handler",
}

SPECTACULAR_SETTINGS = SPECTACULAR_SETTINGS

# --- allauth / auth ---------------------------------------------------------------------------
ACCOUNT_LOGIN_METHODS = {"email"}
ACCOUNT_SIGNUP_FIELDS = ["email*", "password1*", "password2*"]
ACCOUNT_EMAIL_VERIFICATION = "none"
ACCOUNT_ADAPTER = "apps.identity.adapters.ClosedSignupAdapter"
LOGIN_REDIRECT_URL = "/"
LOGOUT_REDIRECT_URL = "/"
LOGIN_URL = "/accounts/login/"

AXES_ENABLED = True
AXES_FAILURE_LIMIT = 5
AXES_COOLOFF_TIME = 1
AXES_LOCKOUT_PARAMETERS = [["username", "ip_address"]]

# --- Nestbox domain settings ------------------------------------------------------------------
NESTBOX_BOOTSTRAP_ADMIN_EMAIL = os.environ.get("NESTBOX_BOOTSTRAP_ADMIN_EMAIL", "")
NESTBOX_BOOTSTRAP_ADMIN_PASSWORD = os.environ.get("NESTBOX_BOOTSTRAP_ADMIN_PASSWORD", "")
NESTBOX_DOCKER_SOCKET = os.environ.get("NESTBOX_DOCKER_SOCKET", "unix:///var/run/docker.sock")
NESTBOX_OPENSHELL_ENDPOINT = os.environ.get("NESTBOX_OPENSHELL_ENDPOINT", "127.0.0.1:50051")
NESTBOX_OPENSHELL_ADAPTER = os.environ.get("NESTBOX_OPENSHELL_ADAPTER", "auto")
NESTBOX_DATA_DIR = Path(os.environ.get("NESTBOX_DATA_DIR", "/var/lib/nestbox"))
NESTBOX_CONTROL_CONTAINER = os.environ.get("NESTBOX_CONTROL_CONTAINER", "nestbox-controller")
NESTBOX_VERSION = "0.1.0"

# --- logging -----------------------------------------------------------------------------------
LOGGING = {
    "version": 1,
    "disable_existing_loggers": False,
    "formatters": {
        "plain": {"format": "%(asctime)s %(levelname)-7s %(name)s: %(message)s"},
    },
    "handlers": {
        "console": {"class": "logging.StreamHandler", "formatter": "plain"},
    },
    "root": {"handlers": ["console"], "level": os.environ.get("NESTBOX_LOG_LEVEL", "INFO")},
    "loggers": {
        "django.request": {"level": "WARNING"},
        "apps": {"level": os.environ.get("NESTBOX_LOG_LEVEL", "INFO")},
    },
}
