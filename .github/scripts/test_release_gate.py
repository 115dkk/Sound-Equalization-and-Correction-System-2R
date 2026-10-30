"""Unit tests for release_gate.py (python -m unittest, no dependencies)."""
import contextlib
import io
import subprocess
import tempfile
import unittest
from pathlib import Path
from unittest import mock

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
            "apps/secs2r/ui/index.html",
            "apps/secs2r/ui/vite.config.ts",
            "apps/secs2r/ui/src/screens/measure/MeasureScreen.tsx",
            "apps/secs2r/ui/package-lock.json",
            "apps/secs2r/ui/src/ipc/bindings/Request.ts",
            "apps/secs2r-cli/src/main.rs",
            "apps/secs2r/icons/icon.ico",
            "apps/secs2r/icons/window/bubble-32.png",
            "bindings/python/Cargo.toml",
            "bindings/python/pyproject.toml",
            "bindings/python/src/lib.rs",
            "bindings/python/secs2r/__init__.py",
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
            "apps/secs2r/ui/.gitignore",
            "apps/secs2r/ui/gallery.html",
            "apps/secs2r/ui/src/gallery/main.tsx",
            "apps/secs2r/ui/src/gallery/tour/scenes.ts",
            "apps/secs2r/ui/scripts/screen-tour.mjs",
            "apps/secs2r/icons/render_icons.py",
            "bindings/python/tests/test_secs2r.py",
            "bindings/python/README.md",
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

    def test_fuzz_lock_follows_the_workspace_packages_whatever_it_held(self):
        fuzz = """version = 4

[[package]]
name = "secs-types"
version = "0.0.0"

[[package]]
name = "secs2r-fuzz"
version = "0.0.0"

[[package]]
name = "unrelated"
version = "0.0.0"
source = "registry+https://github.com/rust-lang/crates.io-index"
"""
        members = g.workspace_packages(self.LOCK, "0.0.1")
        self.assertEqual(members, {"secs-types"})
        lock = g.set_member_versions(fuzz, members, "0.1.0")
        self.assertIn('name = "secs-types"\nversion = "0.1.0"', lock)
        self.assertIn('name = "secs2r-fuzz"\nversion = "0.0.0"', lock)
        self.assertIn('name = "unrelated"\nversion = "0.0.0"', lock)

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

    def test_bump_commit_skips_ci_and_keeps_its_subject(self):
        message = g.bump_commit_message("0.2.0")
        subject, _, body = message.partition("\n\n")
        self.assertEqual(subject, "chore(release): v0.2.0")
        # The deploy-key push starts push workflows; this keeps them off.
        self.assertIn("[skip ci]", body)
        # The subject still reads as a patch-level message for bump_kind.
        self.assertEqual(g.bump_kind([message]), "patch")

    def test_bump_commit_names_the_crates_version(self):
        message = g.bump_commit_message("0.31.0", "0.2.0")
        subject, _, body = message.partition("\n\n")
        self.assertEqual(subject, "chore(release): v0.31.0")
        self.assertIn("api v0.2.0", body)
        self.assertIn("[skip ci]", body)
        self.assertEqual(g.bump_kind([message]), "patch")


class ApiTest(unittest.TestCase):
    PUBLISHED = (
        '[package]\nname = "secs-types"\nversion = "0.1.0"\nedition.workspace = true\n'
        '\n[dependencies]\nserde.workspace = true\n'
    )
    UNPUBLISHED = (
        '[package]\nname = "secs-sys"\nversion.workspace = true\npublish.workspace = true\n'
    )
    WORKSPACE = (
        '[workspace.package]\nversion = "0.30.0"\n\n[workspace.dependencies]\n'
        'secs-types = { path = "crates/secs-types", version = "=0.1.0" }\n'
        'secs-oracle = { path = "crates/secs-oracle" }\n'
        'other = { version = "=0.1.0" }\n'
    )
    PYTHON = (
        '[package]\nname = "secs-py"\nversion = "0.1.0"\n\n[dependencies]\n'
        'secs = { package = "api", version = "=0.1.0" }\n'
        'pyo3 = { version = "0.30.1" }\n'
    )

    def test_publishing_follows_the_package_table(self):
        self.assertTrue(g.manifest_publishes(self.PUBLISHED))
        self.assertFalse(g.manifest_publishes(self.UNPUBLISHED))
        self.assertFalse(g.manifest_publishes('[package]\nname = "a"\npublish = false\n'))
        self.assertFalse(g.manifest_publishes('[package]\nname = "a"\npublish = { workspace = true }\n'))

    def test_reads_and_sets_the_literal_version(self):
        self.assertEqual(g.package_field(self.PUBLISHED, "name"), "secs-types")
        self.assertEqual(g.package_field(self.PUBLISHED, "version"), "0.1.0")
        moved = g.set_package_version(self.PUBLISHED, "0.2.0")
        self.assertEqual(g.package_field(moved, "version"), "0.2.0")
        self.assertIn("serde.workspace = true", moved)
        with self.assertRaises(ValueError):
            g.package_field(self.UNPUBLISHED, "version")

    def test_moves_only_the_named_exact_requirements(self):
        moved = g.set_exact_requirements(self.WORKSPACE, {"secs-types"}, "0.1.0", "0.2.0")
        self.assertIn('secs-types = { path = "crates/secs-types", version = "=0.2.0" }', moved)
        self.assertIn('other = { version = "=0.1.0" }', moved)
        self.assertIn('version = "0.30.0"', moved)

    def test_python_manifest_follows_the_api_version(self):
        moved = g.set_package_version(self.PYTHON, "0.2.0")
        moved = g.set_dependency_versions(moved, {"api", "secs-py"}, "0.1.0", "0.2.0")
        self.assertEqual(g.package_field(moved, "version"), "0.2.0")
        self.assertIn('secs = { package = "api", version = "=0.2.0" }', moved)
        self.assertIn('pyo3 = { version = "0.30.1" }', moved)

    def test_exact_requirements_move_by_package_name_not_by_version(self):
        # A third-party crate pinned at the API's version number stays put.
        manifest = (
            '[package]\nname = "secs-py"\nversion = "0.9.0"\n\n[dependencies]\n'
            'secs = { package = "api", version = "=0.9.0" }\n'
            'libloading = "=0.9.0"\n'
        )
        moved = g.set_dependency_versions(manifest, {"api"}, "0.9.0", "0.10.0")
        self.assertIn('secs = { package = "api", version = "=0.10.0" }', moved)
        self.assertIn('libloading = "=0.9.0"', moved)
        workspace = (
            '[workspace.dependencies]\n'
            'secs-types = { path = "crates/secs-types", version = "=0.9.0" }\n'
            'libloading = "=0.9.0"\n'
        )
        moved = g.set_exact_requirements(workspace, {"secs-types"}, "0.9.0", "0.10.0")
        self.assertIn('secs-types = { path = "crates/secs-types", version = "=0.10.0" }', moved)
        self.assertIn('libloading = "=0.9.0"', moved)
        self.assertEqual(g.dependency_package('secs = { package = "api", path = "a" }'), "api")
        self.assertEqual(g.dependency_package("[dependencies]"), None)

    def test_packaged_files_exclude_tests_examples_and_benches(self):
        changed = [
            "crates/secs-types/src/ipc.rs",
            "crates/secs-types/tests/wire.rs",
            "crates/secs-types/README.md",
            "crates/secs-service/examples/support/case.rs",
            "crates/secs-sys/src/lib.rs",
            "apps/secs2r/src/main.rs",
            "bindings/python/src/lib.rs",
            "bindings/python/tests/test_secs2r.py",
        ]
        self.assertEqual(
            g.api_files(
                changed,
                ["crates/secs-types", "crates/secs-service", "bindings/python"],
            ),
            [
                "crates/secs-types/src/ipc.rs",
                "crates/secs-types/README.md",
                "bindings/python/src/lib.rs",
            ],
        )

    def test_api_decision(self):
        self.assertEqual(g.api_decide("0.1.0", set(), [], []), ("publish", "0.1.0"))
        self.assertEqual(g.api_decide("0.1.0", {"api-v0.1.0"}, [], []), ("none", "0.1.0"))
        self.assertEqual(g.api_decide("0.1.0", {"api-v0.1.0"}, ["crates/x/src/a.rs"], ["fix: a"]),
                         ("bump", "0.1.1"))
        self.assertEqual(g.api_decide("0.1.0", {"api-v0.1.0"}, ["crates/x/src/a.rs"], ["feat: a"]),
                         ("bump", "0.2.0"))
        self.assertEqual(g.api_decide("0.1.0", {"api-v0.1.0", "api-v0.1.1"},
                                      ["crates/x/src/a.rs"], ["fix: a"]), ("bump", "0.1.2"))

    def test_lock_entries_of_the_crates_move_together(self):
        lock = (
            '[[package]]\nname = "secs-types"\nversion = "0.1.0"\ndependencies = []\n\n'
            '[[package]]\nname = "secs2r"\nversion = "0.30.0"\n\n'
            '[[package]]\nname = "serde"\nversion = "0.1.0"\nsource = "registry+https://x"\n'
        )
        moved = g.set_member_versions(lock, {"secs-types"}, "0.2.0")
        self.assertIn('name = "secs-types"\nversion = "0.2.0"', moved)
        self.assertIn('name = "secs2r"\nversion = "0.30.0"', moved)
        self.assertIn('name = "serde"\nversion = "0.1.0"', moved)


class GitFailureTest(unittest.TestCase):
    @staticmethod
    def result(returncode=0, stdout="", stderr=""):
        return subprocess.CompletedProcess([], returncode, stdout, stderr)

    def test_failed_push_with_unchanged_main_fails(self):
        runs = [self.result(), self.result(), self.result(), self.result(),
                self.result(1, stderr="permission denied"), self.result(),
                self.result(stdout="a" * 40 + "\n")]
        with mock.patch.object(g, "_read", side_effect=[VersionTest.CARGO, VersionTest.LOCK]), \
             mock.patch.object(g, "FUZZ_LOCK", Path("no-fuzz-workspace/Cargo.lock")), \
             mock.patch.object(g, "_write"), mock.patch.object(g, "_run", side_effect=runs), \
             self.assertRaisesRegex(SystemExit, "bump push failed"):
            g._commit_bump("0.0.1", "0.0.2", "a" * 40)

    def test_failed_push_with_advanced_main_skips(self):
        runs = [self.result(), self.result(), self.result(), self.result(),
                self.result(1, stderr="non-fast-forward"), self.result(),
                self.result(stdout="b" * 40 + "\n")]
        with mock.patch.object(g, "_read", side_effect=[VersionTest.CARGO, VersionTest.LOCK]), \
             mock.patch.object(g, "FUZZ_LOCK", Path("no-fuzz-workspace/Cargo.lock")), \
             mock.patch.object(g, "_write"), mock.patch.object(g, "_run", side_effect=runs):
            self.assertIsNone(g._commit_bump("0.0.1", "0.0.2", "a" * 40))

    def test_bump_also_rewrites_and_stages_the_fuzz_lock(self):
        fuzz_lock = 'version = 4\n\n[[package]]\nname = "secs-types"\nversion = "0.0.0"\n'
        runs = [self.result()] * 5 + [self.result(stdout="c" * 40 + "\n")]
        with tempfile.TemporaryDirectory() as folder:
            fuzz = Path(folder) / "Cargo.lock"
            fuzz.write_text(fuzz_lock)
            with mock.patch.object(g, "_read",
                                   side_effect=[VersionTest.CARGO, VersionTest.LOCK, fuzz_lock]), \
                 mock.patch.object(g, "FUZZ_LOCK", fuzz), \
                 mock.patch.object(g, "_write") as write, \
                 mock.patch.object(g, "_run", side_effect=runs) as run:
                self.assertEqual(g._commit_bump("0.0.1", "0.0.2", "a" * 40), "c" * 40)
        written = {path: text for (path, text), _ in write.call_args_list}
        self.assertIn('name = "secs-types"\nversion = "0.0.2"', written[fuzz])
        self.assertIn(str(fuzz), run.call_args_list[2].args)

    def test_fetch_failure_fails_and_prints_stderr(self):
        with mock.patch.object(g, "_run", return_value=self.result(1, stderr="network down")), \
             contextlib.redirect_stderr(io.StringIO()) as stderr, \
             self.assertRaisesRegex(SystemExit, "git fetch failed"):
            g.fetch_remote_main(tags=True)
        self.assertIn("network down", stderr.getvalue())


if __name__ == "__main__":
    unittest.main()
