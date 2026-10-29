import os
import tarfile
import tempfile
import unittest

from fastapi import HTTPException

from app import backups


class ArchiveTest(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.mkdtemp()
        self.root = os.path.join(self.tmp, "root")
        os.makedirs(os.path.join(self.root, "sub"))
        with open(os.path.join(self.root, "a.txt"), "w") as handle:
            handle.write("alpha")
        with open(os.path.join(self.root, "sub", "b.txt"), "w") as handle:
            handle.write("beta")

    def test_a_round_trip_restores_every_file(self):
        archive = os.path.join(self.tmp, "x.tar.gz")
        self.assertGreater(backups.create_archive(self.root, archive), 0)
        target = os.path.join(self.tmp, "restored")
        os.makedirs(target)
        self.assertGreaterEqual(backups.extract_archive(archive, os.path.realpath(target)), 3)
        with open(os.path.join(target, "sub", "b.txt")) as handle:
            self.assertEqual(handle.read(), "beta")

    def test_the_checksum_changes_when_the_archive_does(self):
        archive = os.path.join(self.tmp, "x.tar.gz")
        backups.create_archive(self.root, archive)
        first = backups.sha256_of(archive)
        with open(archive, "ab") as handle:
            handle.write(b"x")
        self.assertNotEqual(first, backups.sha256_of(archive))

    def test_an_entry_that_escapes_the_root_is_refused(self):
        archive = os.path.join(self.tmp, "evil.tar.gz")
        with tarfile.open(archive, "w:gz") as tar:
            info = tarfile.TarInfo("../outside.txt")
            info.size = 0
            tar.addfile(info)
        target = os.path.realpath(os.path.join(self.tmp, "safe"))
        os.makedirs(target)
        with self.assertRaises(HTTPException):
            backups.extract_archive(archive, target)
        self.assertFalse(os.path.exists(os.path.join(self.tmp, "outside.txt")))

    def test_a_link_that_leaves_the_root_is_refused(self):
        archive = os.path.join(self.tmp, "link.tar.gz")
        with tarfile.open(archive, "w:gz") as tar:
            info = tarfile.TarInfo("escape")
            info.type = tarfile.SYMTYPE
            info.linkname = "/etc"
            tar.addfile(info)
        target = os.path.realpath(os.path.join(self.tmp, "safe2"))
        os.makedirs(target)
        with self.assertRaises(HTTPException):
            backups.extract_archive(archive, target)

    def test_a_backup_id_must_be_a_uuid_and_a_root_must_be_under_the_base(self):
        with self.assertRaises(HTTPException):
            backups._archive("../../etc/passwd")
        with self.assertRaises(HTTPException):
            backups._root("/etc")


if __name__ == "__main__":
    unittest.main()
