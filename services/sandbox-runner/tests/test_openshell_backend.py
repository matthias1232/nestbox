import unittest

from app import openshell_backend as osh
from app.workload import openshell_name


class MemoryQuantityTest(unittest.TestCase):
    def test_bytes_stay_bytes(self):
        self.assertEqual(osh.memory_quantity("1073741824b"), "1073741824")

    def test_docker_units_become_binary_quantities(self):
        self.assertEqual(osh.memory_quantity("1g"), "1Gi")
        self.assertEqual(osh.memory_quantity("512m"), "512Mi")


class WorkloadNameTest(unittest.TestCase):
    def test_the_sandbox_name_is_read_from_the_workload_container(self):
        name = "openshell-default--my-box-4e1c180d-bd76-4315-aaf6-caf4fdcd98ff"
        self.assertEqual(openshell_name(name), "my-box")

    def test_the_supervisor_and_foreign_containers_are_not_sandboxes(self):
        self.assertIsNone(openshell_name("openshell-default--my-box-4e1c180d-bd76-4315-aaf6-caf4fdcd98ff-supervisor"))
        self.assertIsNone(openshell_name("nestbox-sandbox-claude-01"))


if __name__ == "__main__":
    unittest.main()
