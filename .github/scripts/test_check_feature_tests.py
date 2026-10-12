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
        self.assertEqual(c.missing(wanted, listed), ["secs-types::orphan"])
        self.assertEqual(c.missing({"x": {"t"}}, {}), ["x::t"])


if __name__ == "__main__":
    unittest.main()
