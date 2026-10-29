import unittest

from app.openshell_events import parse, parse_line


class ParseTest(unittest.TestCase):
    def test_a_denied_connection(self):
        event = parse_line(
            "[1793267740.277] [sandbox] [OCSF ] [ocsf] NET:OPEN [MED] DENIED /usr/bin/curl(0) -> example.com:443 "
            "[reason:transparent_tcp_policy_denied]"
        )
        self.assertEqual(event["decision"], "deny")
        self.assertEqual((event["host"], event["port"], event["binary"]), ("example.com", 443, "/usr/bin/curl"))
        self.assertEqual(event["reason"], "transparent_tcp_policy_denied")
        self.assertEqual(event["kind"], "connection")

    def test_an_allowed_connection_names_its_policy(self):
        event = parse_line(
            "[1793267749.402] [sandbox] [OCSF ] [ocsf] NET:OPEN [INFO] ALLOWED /home/n/claude.exe(0) -> 10.90.92.30:20128 "
            "[policy:nb0_inference_provider__omniroute engine:opa]"
        )
        self.assertEqual(event["decision"], "allow")
        self.assertEqual(event["policy"], "nb0_inference_provider__omniroute")
        self.assertEqual(event["port"], 20128)

    def test_a_dns_refusal_has_no_binary_and_no_port(self):
        event = parse_line("[1793267749.429] [sandbox] [OCSF ] [ocsf] NET:REFUSE [LOW] DENIED mcp-hub [reason:policy_dns_upstream_error_response]")
        self.assertEqual(event["kind"], "dns")
        self.assertIsNone(event["binary"])
        self.assertIsNone(event["port"])

    def test_other_lines_are_ignored(self):
        self.assertEqual(parse("[1.0] [gateway] [INFO ] something else\n\nSSH:OPEN ALLOWED"), [])


if __name__ == "__main__":
    unittest.main()
