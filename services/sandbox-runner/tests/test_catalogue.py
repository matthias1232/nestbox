"""The catalogue's parsing and validation rules, without Docker and without FastAPI.

`parse_search` is the part that turns a package manager's output into hits, and
it is where a wrong split would quietly invent a package name. `QUERY` is the
boundary that keeps a search fragment from becoming shell text; both are tested
here so the endpoint's container path stays a thin adapter.
"""
from __future__ import annotations

import unittest

from app import catalogue


class QueryValidation(unittest.TestCase):
    def test_a_package_fragment_is_accepted(self) -> None:
        for query in ("nginx", "python3", "libstdc++", "@angular/core", "postgresql-16"):
            self.assertIsNotNone(catalogue.QUERY.match(query), query)

    def test_shell_text_is_refused(self) -> None:
        for query in ("nginx; id", "$(id)", "a b", "`id`", "nginx|curl", ""):
            self.assertIsNone(catalogue.QUERY.match(query), query)


class Parsing(unittest.TestCase):
    def test_apt_lines_carry_a_summary(self) -> None:
        hits = catalogue.parse_search("apt", "nginx - small, powerful web server\njq - JSON processor\n")
        self.assertEqual([hit["name"] for hit in hits], ["nginx", "jq"])
        self.assertEqual(hits[0]["summary"], "small, powerful web server")

    def test_dnf_lines_drop_the_architecture_and_keep_the_version(self) -> None:
        hits = catalogue.parse_search(
            "dnf",
            "Available Packages\nnginx.x86_64        1.24.0-1.fc41    fedora\n",
        )
        self.assertEqual(hits, [{"name": "nginx", "version": "1.24.0-1.fc41", "summary": "fedora"}])

    def test_apk_lines_split_name_and_version(self) -> None:
        hits = catalogue.parse_search("apk", "nginx-1.26.2-r0\n")
        self.assertEqual(hits[0]["name"], "nginx")
        self.assertEqual(hits[0]["version"], "1.26.2-r0")

    def test_the_limit_is_respected(self) -> None:
        output = "\n".join(f"package-{index} - x" for index in range(200))
        self.assertEqual(len(catalogue.parse_search("apt", output, limit=10)), 10)


if __name__ == "__main__":
    unittest.main()
