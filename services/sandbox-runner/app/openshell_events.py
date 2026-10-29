"""The connection decisions of OpenShell sandboxes, as events.

Each supervisor logs every connection it mediates as an OCSF line:

    [1793267740.277] [sandbox] [OCSF ] [ocsf] NET:OPEN [MED] DENIED /usr/bin/curl(0) -> example.com:443 [reason:transparent_tcp_policy_denied]
    [1793267749.402] [sandbox] [OCSF ] [ocsf] NET:OPEN [INFO] ALLOWED /usr/bin/claude(0) -> 10.0.0.5:20128 [policy:nb0_model engine:opa]
    [1793267749.429] [sandbox] [OCSF ] [ocsf] NET:REFUSE [LOW] DENIED mcp-hub [reason:policy_dns_upstream_error_response]

This module reads those lines (`openshell logs`) and returns them as plain events for
the control API, which owns storage, redaction and the sandbox ids. Parsing is a pure
function so the mapping is tested without a gateway.
"""
from __future__ import annotations

import re
from typing import Any

LINE = re.compile(
    r"^\[(?P<ts>\d+\.\d+)\].*?NET:(?P<kind>OPEN|REFUSE)\s+\[(?P<severity>\w+)\]\s+"
    r"(?P<verdict>ALLOWED|DENIED)\s+(?:(?P<binary>\S+?)\(\d+\)\s+->\s+)?"
    r"(?P<host>[^\s:\[]+)(?::(?P<port>\d+))?(?P<rest>.*)$"
)
POLICY = re.compile(r"policy:(\S+)")
REASON = re.compile(r"reason:([^\]\s]+)")


def parse_line(line: str) -> dict[str, Any] | None:
    """One log line as an event, or None when it is not a network decision."""
    match = LINE.match(line.strip())
    if not match:
        return None
    rest = match.group("rest") or ""
    policy = POLICY.search(rest)
    reason = REASON.search(rest)
    return {
        "ts": float(match.group("ts")),
        "kind": "dns" if match.group("kind") == "REFUSE" else "connection",
        "decision": "allow" if match.group("verdict") == "ALLOWED" else "deny",
        "binary": match.group("binary"),
        "host": match.group("host"),
        "port": int(match.group("port")) if match.group("port") else None,
        "policy": policy.group(1) if policy else None,
        "reason": reason.group(1) if reason else None,
        "severity": match.group("severity").lower(),
    }


def parse(text: str) -> list[dict[str, Any]]:
    return [event for event in (parse_line(line) for line in text.splitlines()) if event]
