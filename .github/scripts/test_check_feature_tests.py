"""Unit tests for check_feature_tests.py (python -m unittest, no dependencies)."""
import unittest

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

    def test_eqapo_playback_job_allows_its_ignored_test(self):
        wanted = {"secs-filters": {"exported_configs_play_as_designed"}}
        listed = {"secs-filters": {"exported_configs_play_as_designed"}}
        ignored = {"secs-filters": {"exported_configs_play_as_designed"}}
        self.assertEqual(c.missing(wanted, listed, ignored), [])


if __name__ == "__main__":
    unittest.main()
