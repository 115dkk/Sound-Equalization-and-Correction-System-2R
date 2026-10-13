"""Unit tests for release_gate.py (python -m unittest, no dependencies)."""
import unittest

import release_gate as g


class ShipsTest(unittest.TestCase):
    def test_build_inputs_ship(self):
        for path in [
            "crates/secs-engine/src/lib.rs",
            "crates/secs-types/src/ipc.rs",
            "apps/secs2r/src/main.rs",
            "apps/secs2r/tauri.conf.json",
            "apps/secs2r/icons/icon.ico",
            "apps/secs2r/ui/src/App.tsx",
            "apps/secs2r/ui/package-lock.json",
            "apps/secs2r/ui/src/ipc/bindings/Request.ts",
            "apps/secs2r-cli/src/main.rs",
            "Cargo.toml",
            "Cargo.lock",
            "rust-toolchain.toml",
            "LICENSE",
            "UPSTREAM.toml",
        ]:
            self.assertTrue(g.ships(path), path)

    def test_tests_docs_policy_oracle_and_ci_do_not_ship(self):
        for path in [
            "crates/secs-engine/tests/rc.rs",
            "crates/secs-dsp/benches/perf.rs",
            "crates/secs-jos/fuzz/fuzz_targets/parse.rs",
            "crates/secs-engine/README.md",
            "crates/secs-policy/tests/gates.rs",
            "crates/secs-policy/src/lib.rs",
            "apps/secs2r/ui/src/ipc/client.test.ts",
            "apps/secs2r/ui/node_modules/react/index.js",
            "features.toml",
            "unsafe-budget.toml",
            "deny.toml",
            "docs/adr/0001-fork-layout-and-upstream.md",
            ".github/workflows/ci.yml",
            "CONTEXT.md",
            "README.md",
            "THIRD_PARTY_NOTICES.md",
            "src/secs/correction_engine.py",
            "tests/test_model.py",
            "tools/qa_capture.py",
            "",
        ]:
            self.assertFalse(g.ships(path), path)


class VersionTest(unittest.TestCase):
    CARGO = """[workspace]
members = ["crates/*", "apps/*"]

[workspace.package]
version = "0.0.1"
edition = "2024"

[workspace.dependencies]
serde = { version = "1.0.229" }
"""

    LOCK = """version = 4

[[package]]
name = "secs-types"
version = "0.0.1"
dependencies = [
 "serde",
]

[[package]]
name = "unrelated"
version = "0.0.1"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "00"
"""

    def test_workspace_version_reads_and_writes_only_the_package_table(self):
        self.assertEqual(g.workspace_version(self.CARGO), "0.0.1")
        bumped = g.set_workspace_version(self.CARGO, "0.1.0")
        self.assertEqual(g.workspace_version(bumped), "0.1.0")
        self.assertIn('serde = { version = "1.0.229" }', bumped)
        with self.assertRaises(ValueError):
            g.workspace_version('[package]\nversion = "1.0.0"\n')

    def test_lock_rewrite_leaves_registry_packages_alone(self):
        lock = g.set_lock_versions(self.LOCK, "0.0.1", "0.1.0")
        self.assertIn('name = "secs-types"\nversion = "0.1.0"', lock)
        self.assertIn('name = "unrelated"\nversion = "0.0.1"', lock)

    def test_latest_tag_ignores_suffixed_tags(self):
        tags = ["v0.1.0-pre.11", "v0.1.9", "v0.1.10", "v0.2.0-rc.1", "x", "v2.0.0-beta.1.2.0"]
        self.assertEqual(g.latest_tag(tags), "v0.1.10")
        self.assertIsNone(g.latest_tag(["v0.1.0-pre.11"]))

    def test_bump_kind_follows_commit_subjects(self):
        self.assertEqual(g.bump_kind(["fix: a", "docs: b"]), "patch")
        self.assertEqual(g.bump_kind(["fix: a", "feat(engine): b"]), "minor")
        self.assertEqual(g.bump_kind(["refactor!: drop the old API"]), "minor")
        self.assertEqual(g.bump_kind(["fix: x\n\nBREAKING CHANGE: y"]), "minor")
        self.assertEqual(g.bump_kind(["chore: feature flags"]), "patch")
        self.assertEqual(g.bump_kind([]), "patch")

    def test_prerelease_means_major_zero(self):
        self.assertTrue(g.is_prerelease("0.9.3"))
        self.assertFalse(g.is_prerelease("1.0.0"))
        with self.assertRaises(ValueError):
            g.parse_version("0.1.0-pre.11")


class DecideTest(unittest.TestCase):
    def test_untagged_version_releases_as_is(self):
        self.assertEqual(g.decide("0.0.1", set(), [], [], False), ("release", "0.0.1"))
        self.assertEqual(g.decide("1.0.0", {"v0.9.0"}, [], [], False), ("release", "1.0.0"))

    def test_shipped_changes_bump(self):
        tags = {"v0.1.0"}
        self.assertEqual(g.decide("0.1.0", tags, ["Cargo.lock"], ["fix: a"], False),
                         ("bump", "0.1.1"))
        self.assertEqual(g.decide("0.1.3", {"v0.1.3"}, ["crates/x/src/lib.rs"],
                                  ["feat: b"], False),
                         ("bump", "0.2.0"))

    def test_nothing_shipped_does_nothing_unless_forced(self):
        self.assertEqual(g.decide("0.1.0", {"v0.1.0"}, [], [], False), ("none", "0.1.0"))
        self.assertEqual(g.decide("0.1.0", {"v0.1.0"}, [], [], True), ("bump", "0.1.1"))

    def test_stale_triggers_do_not_release(self):
        self.assertFalse(g.is_stale("a" * 40, "a" * 40))
        self.assertTrue(g.is_stale("a" * 40, "b" * 40))
        # Without a readable main head the gate keeps its decision.
        self.assertFalse(g.is_stale("a" * 40, ""))

    def test_bump_skips_taken_versions(self):
        self.assertEqual(g.decide("0.1.0", {"v0.1.0", "v0.1.1"}, ["Cargo.toml"],
                                  ["fix: a"], False),
                         ("bump", "0.1.2"))


if __name__ == "__main__":
    unittest.main()
