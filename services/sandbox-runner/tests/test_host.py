"""The arithmetic behind the host load figures, without Docker."""
from __future__ import annotations

import sys
import types
import unittest

# `app.host` imports docker and FastAPI at module scope; the functions under test
# use neither, so minimal stand-ins let the real file be imported anywhere.
for name, attrs in {
    "docker": {"from_env": lambda: None},
    "docker.errors": {"APIError": Exception, "NotFound": Exception},
    "fastapi": {"APIRouter": lambda: types.SimpleNamespace(get=lambda *a, **k: (lambda f: f), patch=lambda *a, **k: (lambda f: f)), "HTTPException": Exception},
    "pydantic": {"BaseModel": object, "Field": lambda *a, **k: None},
}.items():
    if name not in sys.modules:
        module = types.ModuleType(name)
        module.__dict__.update(attrs)
        sys.modules[name] = module

from app import host  # noqa: E402


class CpuTest(unittest.TestCase):
    STAT = "cpu  100 0 100 700 100 0 0 0 0 0\ncpu0 1 2 3 4"

    def test_busy_and_total_exclude_idle_and_iowait(self) -> None:
        self.assertEqual(host.parse_cpu_times(self.STAT), (200, 1000))

    def test_percent_between_two_samples(self) -> None:
        self.assertEqual(host.cpu_percent((200, 1000), (300, 1200)), 50.0)

    def test_no_time_passing_is_zero_not_a_division_error(self) -> None:
        self.assertEqual(host.cpu_percent((1, 1), (1, 1)), 0.0)

    def test_garbage_is_none(self) -> None:
        self.assertIsNone(host.parse_cpu_times("nothing"))


class MemTest(unittest.TestCase):
    def test_kilobytes_become_bytes(self) -> None:
        info = host.parse_meminfo("MemTotal:   1000 kB\nMemAvailable:   250 kB\nHugePages_Total: 0")
        self.assertEqual(info["MemTotal"], 1_024_000)
        self.assertEqual(info["MemAvailable"], 256_000)


if __name__ == "__main__":
    unittest.main()
