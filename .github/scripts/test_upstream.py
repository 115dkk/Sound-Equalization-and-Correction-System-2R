"""Unit tests for upstream.py (python -m unittest, no dependencies)."""
import contextlib
import io
import subprocess
import unittest
from unittest import mock

import upstream as u


class ReachTest(unittest.TestCase):
    def test_specific_files_win_over_directories(self):
        self.assertEqual(u.reach("src/secs/rewapi/session.py"), "secs-project (.irs 저장소), secs-rew")
        self.assertEqual(u.reach("src/secs/rewapi/rew.py"), "secs-rew")
        self.assertEqual(u.reach("src/secs/rew_mdat/serialization/parser.py"), "secs-jos")
        self.assertEqual(u.reach("src/secs/rew_mdat/builder.py"), "secs-mdat")
        self.assertEqual(u.reach("src/secs/rew_mdat/source_info.py"), "secs-mdat (원 측정 환경), secs-project (.src.json)")
        self.assertEqual(u.reach("src/secs/wizard/en_g8.py"), "화면 카탈로그 (apps/secs2r/ui)")
        self.assertEqual(u.reach("src/secs/wizard/step2_config.py"), "화면 (apps/secs2r/ui)")
        self.assertEqual(u.reach("src/secs/wizard/assets/logo_b8.svg"), "로고: 아이콘 다시 뽑기 (apps/secs2r/icons/render_icons.py), 레일 (ui/src/app/logo.ts)")
        self.assertEqual(u.reach("src/secs/wizard/assets/app.ico"), "로고: 아이콘 다시 뽑기 (apps/secs2r/icons/render_icons.py), 레일 (ui/src/app/logo.ts)")
        self.assertEqual(u.reach("src/secs/wizard/assets/loading_drip.png"), "화면 자산 (apps/secs2r/ui)")
        self.assertEqual(u.reach("src/secs/mimo_iir.py"), "secs-engine (MIMO IIR 맞춤), secs-gpu")
        self.assertEqual(u.reach("src/secs/mimo.py"), "secs-engine (MIMO)")
        self.assertEqual(u.reach("src/secs/export_filters.py"), "secs-filters (필터 값 텍스트, Equalizer APO)")

    def test_unknown_paths_ask_for_review(self):
        self.assertEqual(u.reach("src/secs/new_module.py"), "검토 필요")
        self.assertEqual(u.reach("something/else.txt"), "검토 필요")


class CheckTreeTest(unittest.TestCase):
    @staticmethod
    def result(returncode=0, stdout="", stderr=""):
        return subprocess.CompletedProcess([], returncode, stdout, stderr)

    def run_check(self, diff, untracked=""):
        with mock.patch.object(u, "load_upstream", return_value={"commit": "a" * 40}), \
             mock.patch.object(u, "_run", side_effect=[self.result(), diff]), \
             mock.patch.object(u, "_git", return_value=untracked), \
             contextlib.redirect_stdout(io.StringIO()) as output:
            code = u.check_tree()
        return code, output.getvalue()

    def test_matching_tree_passes(self):
        code, output = self.run_check(self.result())
        self.assertEqual(code, 0)
        self.assertIn("upstream tree matches", output)

    def test_difference_fails(self):
        code, output = self.run_check(self.result(stdout=" 1 file changed\n"))
        self.assertEqual(code, 1)
        self.assertIn("differs", output)

    def test_diff_command_failure_prints_stderr_and_fails(self):
        code, output = self.run_check(self.result(128, stderr="fatal: bad object\n"))
        self.assertEqual(code, 1)
        self.assertIn("fatal: bad object", output)


class VersionTest(unittest.TestCase):
    def test_reads_stage_and_build(self):
        text = "APP_NAME = 'SECS 2'\nSTAGE = 'Beta'\nBUILD = '1.3.0'\n"
        self.assertEqual(u.upstream_version(text), "Beta 1.3.0")
        self.assertEqual(u.upstream_version("STAGE = ''\nBUILD = '2.0.0'\n"), "2.0.0")
        self.assertEqual(u.upstream_version(""), "알 수 없음")


class IssueTest(unittest.TestCase):
    def test_name_status_keeps_new_path_of_renames(self):
        text = "M\tsrc/secs/model.py\nR087\told.py\tsrc/secs/new.py\nA\t배포/x.cmd\n"
        self.assertEqual(u.parse_name_status(text),
                         [("M", "src/secs/model.py"), ("R", "src/secs/new.py"), ("A", "배포/x.cmd")])

    def test_body_lists_commits_changes_and_steps(self):
        body = u.issue_body("https://github.com/o/r", "main", "a" * 40, "Beta 1.2.0", "b" * 40,
                            "Beta 1.3.0", ["bbbbbbb 새 기능"], [("M", "src/secs/mimo.py")], False)
        self.assertIn("SECS 2 Beta 1.2.0", body)
        self.assertIn("- bbbbbbb 새 기능", body)
        self.assertIn("| `src/secs/mimo.py` | 수정 | secs-engine (MIMO) |", body)
        self.assertIn("UPSTREAM.toml", body)
        self.assertNotIn("기록을 다시 썼으므로", body)
        rewritten = u.issue_body("r", "main", "a" * 40, "x", "b" * 40, "y", [], [], True)
        self.assertIn("기록을 다시 썼으므로", rewritten)
        self.assertIn("| (없음) | | |", rewritten)

    def test_title_names_version_and_short_head(self):
        self.assertEqual(u.issue_title("0123456789abcdef", "Beta 1.3.0"),
                         "업스트림 SECS 2 Beta 1.3.0 (0123456) 이식")


if __name__ == "__main__":
    unittest.main()
