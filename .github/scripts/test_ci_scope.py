"""Unit tests for ci_scope.py (python -m unittest, no dependencies)."""
import json
import re
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

import ci_scope as s

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
WORKFLOW = (ROOT / ".github" / "workflows" / "ci.yml").read_text(encoding="utf-8")
HEAVY = s.heavy_inputs(WORKFLOW, ROOT)


def commit(subject, parents=1):
    return {"parents": parents, "subject": subject}


def changed(filename, previous=None):
    return {"filename": filename, "previous_filename": previous}


def decide(commits, files):
    return s.decide(commits, files, len(commits), len(files), HEAVY)


class SubjectTest(unittest.TestCase):
    def test_only_ci_and_docs_subjects_are_light(self):
        for subject in ("ci: 갤러리 이슈를 닫습니다", "docs: ADR 0032", "ci(release): x", "docs!: x"):
            self.assertTrue(s.light_subject(subject), subject)
        for subject in ("feat: x", "fix: x", "chore: x", "cifix: x", "docs x",
                        "Merge remote-tracking branch 'origin/main' into claude/x"):
            self.assertFalse(s.light_subject(subject), subject)


class HeavyInputsTest(unittest.TestCase):
    def test_files_a_heavy_job_runs_or_reads_are_heavy(self):
        for path in (
            ".github/workflows/ci.yml",
            ".github/scripts/integrity.py",
            ".github/scripts/macos_app_check.py",
            ".github/scripts/collect_bundles.py",
            ".github/scripts/check_feature_tests.py",
            # Named only by manage-e2e, which runs after bundle.
            ".github/scripts/virtual_audio.py",
            ".github/virtual-audio.toml",
            # Imported by camilla_engine.py.
            ".github/scripts/eqapo_engine.py",
            # Named by the scripts, not by ci.yml.
            ".github/blackhole.toml",
            ".github/reaper.toml",
        ):
            self.assertIn(path, HEAVY)

    def test_files_only_lint_or_other_workflows_use_stay_light(self):
        for path in (
            # lint always runs, so what only it runs needs no gated job.
            ".github/scripts/upstream.py",
            ".github/scripts/api_dry_run.py",
            ".github/scripts/release_gate.py",
            ".github/scripts/codex_review.py",
            ".github/scripts/gallery_close.py",
            ".github/scripts/ci_scope.py",
            ".github/workflows/sol-review.yml",
            ".github/codex/policy.txt",
        ):
            self.assertNotIn(path, HEAVY)

    def test_the_gated_jobs_are_the_heavy_ones_and_those_after_them(self):
        self.assertEqual(
            set(s.gated_jobs(WORKFLOW)),
            {"test", "integrity", "eqapo-playback", "camilla-playback", "reaper-playback",
             "bundle", "manage-e2e", "update-e2e", "gallery", "miri", "fuzz"})

    def test_needs_in_any_form_carries_the_gate(self):
        workflow = (
            "jobs:\n"
            "  heavy:\n    if: needs.scope.outputs.heavy == 'true'\n"
            "  inline:\n    needs: heavy\n"
            "  listed:\n    needs: [lint, inline]\n"
            "  block:\n    needs:\n      - lint\n      - listed\n    runs-on: x\n"
            "  free:\n    needs:\n      - lint\n")
        self.assertEqual(set(s.gated_jobs(workflow)), {"heavy", "inline", "listed", "block"})


class DecideTest(unittest.TestCase):
    def test_ci_and_docs_commits_on_light_files_skip_the_heavy_jobs(self):
        heavy, why = decide(
            [commit("ci: Sol 리뷰"), commit("Merge branch 'main' into claude/x", parents=2), commit("docs: 적어 둡니다")],
            [changed(".github/scripts/codex_review.py"), changed("docs/adr/0032-nuget-package.md"),
             changed("CLAUDE.md"), changed("crates/secs-api/README.md")])
        self.assertFalse(heavy, why)

    def test_one_other_commit_runs_them(self):
        heavy, why = decide([commit("ci: x"), commit("fix: y")], [changed("docs/a.md")])
        self.assertTrue(heavy)
        self.assertIn("fix: y", why)

    def test_a_ci_commit_on_code_or_a_heavy_input_runs_them(self):
        for files in ([changed("crates/secs-engine/src/fit.rs")],
                      [changed("features.toml")],
                      [changed(".github/workflows/ci.yml")],
                      [changed(".github/scripts/integrity.py")],
                      # A file moved out of a crate still changed the crate.
                      [changed("docs/fit.rs", previous="crates/secs-engine/src/fit.rs")]):
            heavy, why = decide([commit("ci: x")], files)
            self.assertTrue(heavy, files)

    def test_an_incomplete_list_or_no_own_commit_runs_them(self):
        commits, files = [commit("docs: x")], [changed("docs/a.md")]
        self.assertTrue(s.decide(commits, files, 251, 1, HEAVY)[0])
        self.assertTrue(s.decide(commits, files, 1, 3001, HEAVY)[0])
        self.assertTrue(s.decide(commits, files, None, 1, HEAVY)[0])
        self.assertTrue(decide([commit("Merge pull request #1", parents=2)], files)[0])

    def test_cli_prints_the_decision(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / "commits.jsonl").write_text(json.dumps(commit("docs: x")) + "\n", encoding="utf-8")
            (root / "files.jsonl").write_text(json.dumps(changed("docs/a.md")) + "\n", encoding="utf-8")
            out = subprocess.run(
                [sys.executable, "-I", str(HERE / "ci_scope.py"),
                 "--commits", str(root / "commits.jsonl"), "--files", str(root / "files.jsonl"),
                 "--expected-commits", "1", "--expected-files", "1",
                 "--workflow", str(ROOT / ".github" / "workflows" / "ci.yml"),
                 "--summary", str(root / "summary.md")],
                capture_output=True, text=True, check=True, encoding="utf-8")
            self.assertEqual(out.stdout, "false\n")
            self.assertIn("skipped", (root / "summary.md").read_text(encoding="utf-8"))


class WorkflowTest(unittest.TestCase):
    def test_every_gated_matrix_job_has_a_light_stand_in_under_its_name(self):
        for name, text in s.gated_jobs(WORKFLOW).items():
            if "strategy:" not in text:
                continue
            anchor = re.search(r"^    name: &([a-z-]+) ", text, re.MULTILINE)
            strategy = re.search(r"^    strategy: &([a-z-]+)$", text, re.MULTILINE)
            self.assertTrue(anchor and strategy, f"{name} has no name and strategy anchors")
            stand_in = re.search(
                rf"^  [a-z-]+-light:\n    name: \*{anchor.group(1)}\n    needs: scope\n"
                rf"    if: needs\.scope\.outputs\.heavy == 'false'\n    strategy: \*{strategy.group(1)}\n",
                WORKFLOW, re.MULTILINE)
            self.assertIsNotNone(stand_in, f"{name} has no light stand-in")

    def test_scope_falls_back_to_heavy(self):
        self.assertIn("heavy=true", WORKFLOW)
        self.assertIn("case \"$heavy\" in true|false) ;; *) heavy=true ;; esac", WORKFLOW)


if __name__ == "__main__":
    unittest.main()
