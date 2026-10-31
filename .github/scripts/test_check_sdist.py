"""Unit tests for check_sdist.py (python -m unittest, no dependencies)."""
import io
import tarfile
import tempfile
import unittest
from pathlib import Path

import check_sdist as c

GOOD_INFO = (
    "Metadata-Version: 2.4\nName: secs2r\nVersion: 0.9.0\n"
    "License-Expression: AGPL-3.0-only\nLicense-File: LICENSE\n"
)


def sdist(directory, files):
    path = Path(directory) / "secs2r-0.9.0.tar.gz"
    with tarfile.open(path, "w:gz") as archive:
        for name, text in files.items():
            data = text.encode("utf-8")
            entry = tarfile.TarInfo(f"secs2r-0.9.0/{name}")
            entry.size = len(data)
            archive.addfile(entry, io.BytesIO(data))
    return str(path)


class CheckSdistTest(unittest.TestCase):
    def test_a_licensed_sdist_passes(self):
        with tempfile.TemporaryDirectory() as directory:
            path = sdist(directory, {"PKG-INFO": GOOD_INFO, "LICENSE": "AGPL", "pyproject.toml": ""})
            self.assertEqual(c.problems(path), [])
            self.assertEqual(c.main([path]), 0)

    def test_missing_licence_file_and_metadata_fail(self):
        with tempfile.TemporaryDirectory() as directory:
            path = sdist(directory, {"PKG-INFO": "Name: secs2r\nLicense-Expression: GPL-3.0-only\n"})
            found = c.problems(path)
            self.assertIn("secs2r-0.9.0/LICENSE is missing", found)
            self.assertTrue(any("License-Expression: AGPL-3.0-only" in p for p in found))
            self.assertTrue(any("License-File: LICENSE" in p for p in found))
            self.assertEqual(c.main([path]), 1)

    def test_missing_pkg_info_fails(self):
        with tempfile.TemporaryDirectory() as directory:
            path = sdist(directory, {"LICENSE": "AGPL"})
            self.assertIn("secs2r-0.9.0/PKG-INFO is missing", c.problems(path))


if __name__ == "__main__":
    unittest.main()
