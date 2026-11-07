"""Unit tests for check_feature_tests.py (python -m unittest, no dependencies)."""
import unittest

import pathlib
import re

import check_feature_tests as c


class CheckTest(unittest.TestCase):
    REGISTRY = {"feature": [
        {"id": "a", "status": "implemented", "tests": ["secs-types::envelope_wire_shape", "secs-policy::gate"]},
        {"id": "b", "status": "planned", "tests": ["secs-engine::not_yet"]},
    ]}

    def test_only_implemented_features_count(self):
        self.assertEqual(c.referenced_tests(self.REGISTRY),
                         {"secs-types": {"envelope_wire_shape"}, "secs-policy": {"gate"}})

    def test_terse_list_parsing(self):
        text = "ipc::tests::envelope_wire_shape: test\nipc::export_bindings_request: test\nbench_x: bench\n"
        self.assertEqual(c.listed_tests(text), {"envelope_wire_shape", "export_bindings_request"})

    def test_missing_reports_crate_and_name(self):
        wanted = {"secs-types": {"envelope_wire_shape", "orphan"}}
        listed = {"secs-types": {"envelope_wire_shape"}}
        self.assertEqual(c.missing(wanted, listed, {}), ["secs-types::orphan"])
        self.assertEqual(c.missing({"x": {"t"}}, {}, {}), ["x::t"])

    def test_test_listed_only_as_ignored_is_missing(self):
        wanted = {"secs-engine": {"runs", "manual_only"}}
        listed = {"secs-engine": {"runs", "manual_only"}}
        ignored = {"secs-engine": {"manual_only"}}
        self.assertEqual(c.missing(wanted, listed, ignored), ["secs-engine::manual_only"])

    def test_ignored_ci_jobs_exist_in_ci_yml(self):
        workflow = pathlib.Path(__file__).resolve().parents[1] / "workflows" / "ci.yml"
        jobs = set(re.findall(r"^  ([a-z0-9-]+):$", workflow.read_text(encoding="utf-8"), re.M))
        for job in c.IGNORED_CI_TESTS.values():
            self.assertIn(job, jobs)

    def test_union_accepts_a_test_one_leg_lists_and_ignores_only_when_every_leg_does(self):
        linux = {"listed": {"a": ["unix_only", "both", "flaky"]}, "ignored": {"a": ["flaky"]}}
        windows = {"listed": {"a": ["windows_only", "both", "flaky"]}, "ignored": {"a": []}}
        listed, ignored = c.union_listings([linux, windows])
        self.assertEqual(listed, {"a": {"unix_only", "windows_only", "both", "flaky"}})
        self.assertEqual(ignored, {"a": set()})
        wanted = {"a": {"unix_only", "windows_only", "both", "flaky"}}
        self.assertEqual(c.missing(wanted, listed, ignored), [])
        only_ignored = c.union_listings([
            {"listed": {"a": ["slow"]}, "ignored": {"a": ["slow"]}},
            {"listed": {"a": ["slow"]}, "ignored": {"a": ["slow"]}},
        ])
        self.assertEqual(only_ignored[1], {"a": {"slow"}})

    def test_public_api_crate_is_listed_with_its_workspace_features(self):
        api = "sound-equalization-and-correction-system-rust-api"
        self.assertIn("--all-features", c.list_command(api, False))
        self.assertEqual(c.list_command(api, True)[-1], "--ignored")
        plain = c.list_command("secs-graph", False)
        self.assertNotIn("--all-features", plain)
        self.assertEqual(plain[-3:], ["--list", "--format", "terse"])

    def test_eqapo_playback_job_allows_its_ignored_test(self):
        wanted = {"secs-filters": {"exported_configs_play_as_designed"}}
        listed = {"secs-filters": {"exported_configs_play_as_designed"}}
        ignored = {"secs-filters": {"exported_configs_play_as_designed"}}
        self.assertEqual(c.missing(wanted, listed, ignored), [])


if __name__ == "__main__":
    unittest.main()
