"""Development settings: SQLite, DEBUG, no whitenoise manifest."""
import os
from pathlib import Path

# Developers and the test suite see every module, whatever the appliance's state file says.
os.environ.setdefault("NESTBOX_MODULES", "all")
os.environ.setdefault("NESTBOX_STATE_DIR", str(Path(__file__).resolve().parents[2] / "var"))

from .base import *  # noqa: F403

DEBUG = True
ALLOWED_HOSTS = ["*"]
SECRET_KEY = "dev-insecure-key-change-me"

# Manifest hashing requires collectstatic; in development that is pure friction.
STORAGES = {
    "default": {"BACKEND": "django.core.files.storage.FileSystemStorage"},
    "staticfiles": {"BACKEND": "django.contrib.staticfiles.storage.StaticFilesStorage"},
}
