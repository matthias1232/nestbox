"""Production settings for the appliance image."""
import os

from .base import *  # noqa: F403

DEBUG = False
SECRET_KEY = os.environ["NESTBOX_SECRET_KEY"]

DATABASES = {
    "default": {
        "ENGINE": "django.db.backends.postgresql",
        "NAME": os.environ.get("NESTBOX_DB_NAME", "nestbox"),
        "USER": os.environ.get("NESTBOX_DB_USER", "nestbox"),
        "PASSWORD": os.environ.get("NESTBOX_DB_PASSWORD", ""),
        "HOST": os.environ.get("NESTBOX_DB_HOST", "127.0.0.1"),
        "PORT": os.environ.get("NESTBOX_DB_PORT", "5432"),
        "CONN_MAX_AGE": 60,
    }
}

SECURE_PROXY_SSL_HEADER = ("HTTP_X_FORWARDED_PROTO", "https")
SESSION_COOKIE_SECURE = True
CSRF_COOKIE_SECURE = True
SECURE_HSTS_SECONDS = 31536000
SECURE_HSTS_INCLUDE_SUBDOMAINS = True
SECURE_HSTS_PRELOAD = True
SECURE_CONTENT_TYPE_NOSNIFF = True
X_FRAME_OPTIONS = "DENY"
