"""Unit tests for integrity.py (run by ci.yml with the other script tests)."""
from __future__ import annotations

import hashlib
import io
import tempfile
import unittest
from contextlib import redirect_stderr, redirect_stdout
from pathlib import Path

import integrity


def write(root: Path, relative: str, data: bytes) -> None:
    path = root / relative
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(data)


class TreeHashes(unittest.TestCase):
    def test_hashes_every_file_by_relative_posix_path(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            write(root, "a.yml", b"one")
            write(root, "filters/b.wav", b"two")
            self.assertEqual(
                integrity.tree_hashes(root),
                {
                    "a.yml": hashlib.sha256(b"one").hexdigest(),
                    "filters/b.wav": hashlib.sha256(b"two").hexdigest(),
                },
            )

    def test_large_file_is_hashed_in_chunks(self):
        with tempfile.TemporaryDirectory() as tmp:
            data = bytes(range(256)) * (integrity.CHUNK // 256 * 3 + 7)
            write(Path(tmp), "big.wav", data)
            self.assertEqual(
                integrity.file_hash(Path(tmp) / "big.wav"), hashlib.sha256(data).hexdigest()
            )


class Differences(unittest.TestCase):
    def test_reports_missing_extra_and_different_in_path_order(self):
        reference = {"a": "1" * 64, "b": "2" * 64, "c": "3" * 64}
        other = {"b": "2" * 64, "c": "4" * 64, "d": "5" * 64}
        self.assertEqual(
            integrity.differences(reference, other),
            [
                "missing  a",
                "differs  c  333333333333 != 444444444444",
                "extra    d",
            ],
        )

    def test_identical_trees_have_no_differences(self):
        tree = {"a": "1" * 64}
        self.assertEqual(integrity.differences(tree, dict(tree)), [])


class Compare(unittest.TestCase):
    def run_compare(self, reference: Path, others: list[Path]) -> tuple[int, str]:
        out = io.StringIO()
        with redirect_stdout(out), redirect_stderr(out):
            code = integrity.compare(reference, others)
        return code, out.getvalue()

    def test_same_bytes_pass(self):
        with tempfile.TemporaryDirectory() as tmp:
            one, two = Path(tmp, "1"), Path(tmp, "2")
            for root in (one, two):
                write(root, "camilla.yml", b"same")
            code, text = self.run_compare(one, [two])
            self.assertEqual(code, 0)
            self.assertIn("1 files identical", text)

    def test_one_changed_byte_fails(self):
        with tempfile.TemporaryDirectory() as tmp:
            one, two, three = Path(tmp, "1"), Path(tmp, "2"), Path(tmp, "3")
            write(one, "camilla.yml", b"same")
            write(two, "camilla.yml", b"same")
            write(three, "camilla.yml", b"samf")
            code, text = self.run_compare(one, [two, three])
            self.assertEqual(code, 1)
            self.assertIn("differs  camilla.yml", text)

    def test_empty_reference_fails(self):
        with tempfile.TemporaryDirectory() as tmp:
            one, two = Path(tmp, "1"), Path(tmp, "2")
            one.mkdir()
            write(two, "x", b"x")
            code, text = self.run_compare(one, [two])
            self.assertEqual(code, 1)
            self.assertIn("no files", text)


if __name__ == "__main__":
    unittest.main()
