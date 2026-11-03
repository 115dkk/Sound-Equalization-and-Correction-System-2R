"""Unit tests for gallery_close.py (python -m unittest, no dependencies)."""
import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

import gallery_close as g

HERE = Path(__file__).resolve().parent


def issue(number, title):
    return {"number": number, "title": title}


class GalleryCloseTest(unittest.TestCase):
    def test_reviews_of_pull_requests_that_are_no_longer_open_close(self):
        issues = [
            issue(157, "화면 갤러리 리뷰: PR #156"),
            issue(161, "화면 갤러리 리뷰: PR #159"),
            issue(149, "화면 갤러리 리뷰: PR #148"),
            # A manual run's review and unrelated titles are never touched.
            issue(12, "화면 갤러리 리뷰: 수동 실행 (main)"),
            issue(13, "화면 갤러리 리뷰: PR #15 다시"),
            issue(14, "PR #3"),
        ]
        self.assertEqual(g.stale_reviews(issues, {159}), [(149, 148), (157, 156)])
        self.assertEqual(g.stale_reviews(issues, {156, 148, 159, 15, 3}), [])

    def test_cli_prints_one_issue_and_pull_request_per_line(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / "issues.json").write_text(json.dumps([issue(154, "화면 갤러리 리뷰: PR #153")]), encoding="utf-8")
            (root / "prs.json").write_text(json.dumps([{"number": 159}]), encoding="utf-8")
            out = subprocess.run(
                [sys.executable, "-I", str(HERE / "gallery_close.py"),
                 "--issues", str(root / "issues.json"), "--open-prs", str(root / "prs.json")],
                capture_output=True, text=True, check=True, encoding="utf-8")
            self.assertEqual(out.stdout, "154 153\n")

    def test_workflow_runs_when_a_pull_request_closes_and_closes_with_the_script(self):
        workflow = (HERE.parent / "workflows" / "ui-gallery-close.yml").read_text(encoding="utf-8")
        self.assertIn("types: [closed]", workflow)
        # Fork pull requests get a read-only token; their run would only fail.
        self.assertIn("github.event.pull_request.head.repo.full_name == github.repository", workflow)
        self.assertIn("issues: write", workflow)
        self.assertIn("gallery_close.py", workflow)
        self.assertIn("gh issue close", workflow)


if __name__ == "__main__":
    unittest.main()
