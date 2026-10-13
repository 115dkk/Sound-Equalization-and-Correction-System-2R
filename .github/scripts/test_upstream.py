"""Unit tests for upstream.py (python -m unittest, no dependencies)."""
import unittest

import upstream as u


class ReachTest(unittest.TestCase):
    def test_specific_files_win_over_directories(self):
        self.assertEqual(u.reach("src/secs/rewapi/session.py"), "secs-project (.irs 저장소), secs-rew")
        self.assertEqual(u.reach("src/secs/rewapi/rew.py"), "secs-rew")
        self.assertEqual(u.reach("src/secs/rew_mdat/serialization/parser.py"), "secs-jos")
        self.assertEqual(u.reach("src/secs/rew_mdat/builder.py"), "secs-mdat")
        self.assertEqual(u.reach("src/secs/wizard/en_g8.py"), "화면 카탈로그 (apps/secs2r/ui)")
        self.assertEqual(u.reach("src/secs/wizard/step2_config.py"), "화면 (apps/secs2r/ui)")
        self.assertEqual(u.reach("src/secs/mimo_iir.py"), "secs-engine (MIMO IIR 맞춤), secs-gpu")
        self.assertEqual(u.reach("src/secs/mimo.py"), "secs-engine (MIMO)")

    def test_unknown_paths_ask_for_review(self):
        self.assertEqual(u.reach("src/secs/new_module.py"), "검토 필요")
        self.assertEqual(u.reach("something/else.txt"), "검토 필요")


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
