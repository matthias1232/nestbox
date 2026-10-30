"""Reading the numbers a served model reports.

Kept apart from `app.main` so the parser can be tested without a Docker socket:
`main` builds its client at import time, and a unit test of a pure function
should not need a daemon.
"""

from __future__ import annotations

import math


def parse_prometheus(text: str) -> dict[str, float]:
    """Turn a Prometheus exposition body into `{series: value}`.

    Only the latest sample of each series is kept. Histogram buckets keep their
    `le` label, because they are what a latency graph needs; comment and
    metadata lines are ignored. NaN and infinity are dropped: they are not
    measurements, and a chart that drew them would show a gap as data.
    """
    metrics: dict[str, float] = {}
    for line in text.splitlines():
        line = line.strip()
        if not line or line.startswith("#"):
            continue
        name, _, value = line.rpartition(" ")
        try:
            number = float(value)
        except ValueError:
            continue
        if math.isnan(number) or math.isinf(number):
            continue
        metrics[name] = number
    return metrics
