"""The metric reader: what a Prometheus body becomes for the console."""

from __future__ import annotations

import unittest

from app.model_metrics import parse_prometheus


class ParsePrometheusTests(unittest.TestCase):
    def test_comments_and_metadata_are_ignored(self):
        body = "\n".join(
            [
                "# HELP llamacpp:prompt_tokens_total Prompt tokens",
                "# TYPE llamacpp:prompt_tokens_total counter",
                "llamacpp:prompt_tokens_total 120",
            ]
        )
        self.assertEqual(parse_prometheus(body), {"llamacpp:prompt_tokens_total": 120.0})

    def test_labels_stay_part_of_the_series_name(self):
        body = 'llamacpp:request_duration_seconds_bucket{le="0.1"} 7'
        self.assertEqual(
            parse_prometheus(body),
            {'llamacpp:request_duration_seconds_bucket{le="0.1"}': 7.0},
        )

    def test_a_non_numeric_value_is_skipped_rather_than_zeroed(self):
        body = "llamacpp:broken NaN\nllamacpp:requests_processing 1"
        self.assertEqual(parse_prometheus(body), {"llamacpp:requests_processing": 1.0})


if __name__ == "__main__":
    unittest.main()
