"""Unit tests for release_notes.py (python -m unittest, no dependencies)."""
import io
import os
import tempfile
import unittest
from contextlib import redirect_stdout
from pathlib import Path

import release_notes as n

CHANGELOG = """# 변경 기록

설명.

## 0.1.0 - 2026-10-20

### 새 기능

- 보정 엔진.

## 0.0.1 - 2026-10-13

### 빌드·설정

- 첫 사전 출시판.
"""


class SectionTest(unittest.TestCase):
    def test_picks_the_version_section_only(self):
        self.assertEqual(n.changelog_section(CHANGELOG, "0.1.0"), "### 새 기능\n\n- 보정 엔진.")
        self.assertEqual(n.changelog_section(CHANGELOG, "0.0.1"), "### 빌드·설정\n\n- 첫 사전 출시판.")

    def test_missing_or_empty_section_is_none(self):
        self.assertIsNone(n.changelog_section(CHANGELOG, "0.2.0"))
        self.assertIsNone(n.changelog_section("## 0.3.0 - x\n\n## 0.2.0\n", "0.3.0"))

    def test_section_only_prints_the_section_or_nothing(self):
        with tempfile.TemporaryDirectory() as folder:
            Path(folder, "CHANGELOG.md").write_text(CHANGELOG, encoding="utf-8")
            cwd = os.getcwd()
            os.chdir(folder)
            try:
                for version, expected in (("0.1.0", n.changelog_section(CHANGELOG, "0.1.0") + "\n"),
                                          ("0.2.0", "\n")):
                    out = io.StringIO()
                    with redirect_stdout(out):
                        self.assertEqual(n.main(["--version", version, "--section-only"]), 0)
                    self.assertEqual(out.getvalue(), expected)
            finally:
                os.chdir(cwd)

    def test_header_says_prerelease_for_zero_major(self):
        self.assertIn("사전 출시판", n.header("0.0.1", "a" * 40, "Beta 1.2.0"))
        self.assertIn("정식 판", n.header("1.0.0", "a" * 40, "Beta 1.2.0"))
        self.assertIn("SECS 2 Beta 1.2.0 기준", n.header("1.0.0", "a" * 40, "Beta 1.2.0"))


if __name__ == "__main__":
    unittest.main()
