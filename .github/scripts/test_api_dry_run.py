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
    PUBLISHED = {"sound-equalization-and-correction-system-rust-api", "secs-types", "secs-audio"}
    MANIFEST = (
        '[package]\nname = "x"\nversion = "0.1.0"\n\n'
        '[dependencies]\nsecs = { package = "sound-equalization-and-correction-system-rust-api", '
        'path = "../a", version = "=0.1.0" }\n'
        'other = "0.1.0"\n\n'
        '[dev-dependencies]\nsecs-types = { path = "../b", version = "=0.1.0" }\n\n'
        "[target.'cfg(windows)'.dependencies]\nsecs-audio = { path = \"../c\", version = \"=0.1.0\" }\n"
    )

    def test_only_exact_requirements_in_dependency_tables_move(self):
        out = d.set_exact_dependencies(self.MANIFEST, self.PUBLISHED, "0.1.0", "0.1.0-dryrun")
        self.assertIn('version = "0.1.0"\n\n[dependencies]', out)  # the package's own version stays
        self.assertIn('path = "../a", version = "=0.1.0-dryrun" }', out)  # renamed with package =
        self.assertIn('other = "0.1.0"', out)  # not an exact requirement
        self.assertIn('secs-types = { path = "../b", version = "=0.1.0-dryrun" }', out)
        self.assertIn('secs-audio = { path = "../c", version = "=0.1.0-dryrun" }', out)

    def test_a_third_party_pin_with_the_same_version_stays(self):
        # secs-cuda-sys pins libloading = "=0.9.0"; API 0.9.0 must not move it (CI, 2026-10-31).
        manifest = (
            '[package]\nname = "secs-cuda-sys"\nversion = "0.40.0"\n\n'
            '[dependencies]\nlibloading = "=0.9.0"\n'
            'secs-types = { path = "../secs-types", version = "=0.9.0" }\n'
            'secs-sys = { path = "../secs-sys", version = "=0.9.0" }\n'
        )
        out = d.set_exact_dependencies(manifest, self.PUBLISHED, "0.9.0", "0.9.0-dryrun")
        self.assertIn('libloading = "=0.9.0"\n', out)
        self.assertIn('secs-types = { path = "../secs-types", version = "=0.9.0-dryrun" }', out)
        self.assertIn('secs-sys = { path = "../secs-sys", version = "=0.9.0" }', out)  # not published


if __name__ == "__main__":
    unittest.main()
