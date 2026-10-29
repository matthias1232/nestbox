"""The mapping from Envoy's access-log line to the control API's traffic entry."""
from __future__ import annotations

import json
import unittest

from app import telemetry_shipper as ship

SANDBOX = "f98aed0f-cc4d-44be-b877-4363808c8cac"


def line(**fields) -> str:
    base = {
        "ts": "2026-10-29T01:14:23.240Z",
        "host": "api.anthropic.com:443",
        "target_host": "api.anthropic.com",
        "target_port": "443",
        "method": "CONNECT",
        "connect": "true",
        "status": "200",
        "sandbox": SANDBOX,
        "decision": "allow",
        "reason": "rule",
        "rule_id": "-",
        "bytes_in": "10",
        "bytes_out": "20",
        "duration_ms": "12",
    }
    base.update(fields)
    return "2026-10-29T01:14:23.241Z " + json.dumps(base)


class RecordTest(unittest.TestCase):
    def test_maps_the_fields_the_control_api_reads(self) -> None:
        record = ship.record_from_line(line())
        self.assertEqual(record["sandbox_id"], SANDBOX)
        self.assertEqual(record["host"], "api.anthropic.com")
        self.assertEqual(record["port"], 443)
        self.assertEqual(record["decision"], "allow")
        self.assertEqual(record["status"], 200)
        self.assertEqual(record["bytes_out"], 20)

    def test_envoys_dash_means_no_value_and_is_dropped(self) -> None:
        record = ship.record_from_line(line(rule_id="-", reason="-", path="-"))
        self.assertNotIn("rule_id", record)
        self.assertNotIn("reason", record)
        self.assertNotIn("path", record)

    def test_a_line_that_is_not_an_access_record_is_skipped(self) -> None:
        self.assertIsNone(ship.record_from_line("[2026-10-29 01:00:00][1][info][main] starting"))
        self.assertIsNone(ship.record_from_line("2026-10-29T01:00:00Z {not json"))
        self.assertIsNone(ship.record_from_line('2026-10-29T01:00:00Z {"unrelated": 1}'))

    def test_the_host_comes_from_the_authority_when_the_target_is_missing(self) -> None:
        record = ship.record_from_line(line(target_host="-", target_port="-", host="example.com:8443"))
        self.assertEqual((record["host"], record["port"]), ("example.com", 8443))

    def test_a_sandbox_that_is_not_a_uuid_is_not_invented(self) -> None:
        record = ship.record_from_line(line(sandbox="hermes-01"))
        self.assertNotIn("sandbox_id", record)

    def test_split_host(self) -> None:
        self.assertEqual(ship.split_host("a.b:80"), ("a.b", 80))
        self.assertEqual(ship.split_host("a.b"), ("a.b", None))
        self.assertEqual(ship.split_host("[2001:db8::1]:443"), ("2001:db8::1", 443))
        self.assertEqual(ship.split_host("-"), (None, None))


if __name__ == "__main__":
    unittest.main()
