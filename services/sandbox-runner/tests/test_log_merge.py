import unittest

from app import log_merge


class MergeTest(unittest.TestCase):
    def test_the_agent_and_the_supervisor_read_as_one_log_in_time_order(self):
        agent = "2026-10-29T13:55:02.500000000Z hello from the agent\n"
        shell = (
            "[1793282101.250] [sandbox] [OCSF ] [ocsf] NET:OPEN [INFO] ALLOWED /usr/bin/curl(0) -> a.example:443\n"
            "[1793282103.000] [gateway] [INFO ] [openshell_server] later line\n"
        )
        lines = log_merge.merge(agent, shell, 50).splitlines()
        self.assertEqual(len(lines), 3)
        self.assertIn("[sandbox]", lines[0])
        self.assertIn("[agent] hello from the agent", lines[1])
        self.assertIn("[gateway]", lines[2])
        self.assertTrue(lines[0] < lines[1] < lines[2], "ordered by time")

    def test_only_the_last_lines_are_kept(self):
        shell = "\n".join(f"[{1793282000 + i}.000] [sandbox] [INFO ] line {i}" for i in range(10))
        lines = log_merge.merge("", shell, 3).splitlines()
        self.assertEqual(len(lines), 3)
        self.assertTrue(lines[-1].endswith("line 9"))

    def test_a_wrapped_agent_line_stays_with_its_line(self):
        agent = "2026-10-29T13:55:02.000000000Z Traceback\n  File x\n2026-10-29T13:55:03.000000000Z next\n"
        lines = log_merge.merge(agent, "", 10).splitlines()
        self.assertEqual(len(lines), 2)
        self.assertIn("Traceback File x", lines[0])

    def test_garbage_is_ignored_not_fatal(self):
        self.assertEqual(log_merge.merge("", "no timestamp here\n\n", 5), "")


if __name__ == "__main__":
    unittest.main()
