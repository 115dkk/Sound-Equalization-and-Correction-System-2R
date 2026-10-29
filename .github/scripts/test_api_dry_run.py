"""Unit tests for api_dry_run.py (python -m unittest, no dependencies)."""
import unittest

import api_dry_run as d


class VersionTest(unittest.TestCase):
    def test_suffix(self):
        self.assertEqual(d.dry_run_version("0.1.0"), "0.1.0-dryrun")

    def test_prerelease_rejected(self):
        with self.assertRaises(SystemExit):
            d.dry_run_version("0.1.0-rc.1")


class ExactDependencyTest(unittest.TestCase):
    MANIFEST = (
        '[package]\nname = "x"\nversion = "0.1.0"\n\n'
        '[dependencies]\nsecs = { path = "../a", version = "=0.1.0" }\n'
        'other = "0.1.0"\n\n'
        '[dev-dependencies]\nsecs-types = { path = "../b", version = "=0.1.0" }\n\n'
        "[target.'cfg(windows)'.dependencies]\nsecs-sys = { path = \"../c\", version = \"=0.1.0\" }\n"
    )

    def test_only_exact_requirements_in_dependency_tables_move(self):
        out = d.set_exact_dependencies(self.MANIFEST, "0.1.0", "0.1.0-dryrun")
        self.assertIn('version = "0.1.0"\n\n[dependencies]', out)  # the package's own version stays
        self.assertIn('secs = { path = "../a", version = "=0.1.0-dryrun" }', out)
        self.assertIn('other = "0.1.0"', out)  # not an exact requirement
        self.assertIn('secs-types = { path = "../b", version = "=0.1.0-dryrun" }', out)
        self.assertIn('secs-sys = { path = "../c", version = "=0.1.0-dryrun" }', out)


if __name__ == "__main__":
    unittest.main()
