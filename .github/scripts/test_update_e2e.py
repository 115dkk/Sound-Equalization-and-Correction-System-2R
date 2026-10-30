"""Pure tests for update_e2e; never launch an app, installer, service, or driver."""
from __future__ import annotations

import json
import tempfile
import unittest
from pathlib import Path, PureWindowsPath
from unittest import mock

import update_e2e as u


class SetVersionTest(unittest.TestCase):
    def make_manifests(self, root: Path, content: str = "[package]\nversion.workspace = true\n"):
        for relative in u.MANIFESTS:
            path = root / relative
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(content, encoding="utf-8")

    def test_replaces_all_three_sample_manifests(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            self.make_manifests(root)
            changed = u.set_versions(root, "0.0.1")
            self.assertEqual(changed, [root / relative for relative in u.MANIFESTS])
            for path in changed:
                self.assertEqual(path.read_text(encoding="utf-8"),
                                 '[package]\nversion = "0.0.1"\n')

    def test_missing_line_fails_before_writing_any_manifest(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            self.make_manifests(root)
            missing = root / u.MANIFESTS[-1]
            missing.write_text('[package]\nversion = "9.9.9"\n', encoding="utf-8")
            before = {path: path.read_bytes() for path in (root / item for item in u.MANIFESTS)}
            with self.assertRaisesRegex(u.CheckError, "expected one"):
                u.set_versions(root, "0.0.1")
            self.assertEqual({path: path.read_bytes() for path in before}, before)

    def test_rejects_versions_that_could_change_toml_shape(self):
        with self.assertRaisesRegex(u.CheckError, "semantic version"):
            u.rewritten_manifest(b"version.workspace = true\n", '1.0.0"\nevil=true', Path("x"))


class EnvironmentTest(unittest.TestCase):
    def test_builder_copies_input_and_overrides_only_the_endpoint(self):
        base = {"PATH": "runner-bin", "SECS2R_UPDATE_ENDPOINT": "https://release.invalid/latest.json"}
        environment = u.build_environment(base)
        self.assertEqual(environment, {
            "PATH": "runner-bin",
            "SECS2R_UPDATE_ENDPOINT": u.ENDPOINT,
        })
        self.assertEqual(base["SECS2R_UPDATE_ENDPOINT"], "https://release.invalid/latest.json")
        self.assertIsNot(environment, base)


class UninstallVersionTest(unittest.TestCase):
    def test_reads_the_unique_display_version(self):
        entries = [
            {"DisplayName": "Other", "DisplayVersion": "7"},
            {"DisplayName": "SECS 2R", "DisplayVersion": "0.0.1"},
            {"DisplayName": "SECS 2R", "DisplayVersion": "0.0.1"},
        ]
        self.assertEqual(u.uninstall_entry_version(lambda: iter(entries)), "0.0.1")

    def test_missing_ambiguous_and_blank_versions_fail(self):
        cases = (
            [],
            [{"DisplayName": "SECS 2R", "DisplayVersion": ""}],
            [
                {"DisplayName": "SECS 2R", "DisplayVersion": "0.0.1"},
                {"DisplayName": "SECS 2R", "DisplayVersion": "0.0.2"},
            ],
        )
        for entries in cases:
            with self.subTest(entries=entries), self.assertRaises(u.CheckError):
                u.uninstall_entry_version(lambda entries=entries: iter(entries))

    def test_uninstaller_uses_only_the_executable_from_the_registry(self):
        path = u.uninstaller_path({
            "DisplayName": "SECS 2R",
            "UninstallString": r'"C:\Users\runner\SECS 2R\uninstall.exe" /old-argument',
        })
        self.assertEqual(PureWindowsPath(path), PureWindowsPath(r"C:\Users\runner\SECS 2R\uninstall.exe"))


class UpdateJsonTest(unittest.TestCase):
    def test_requires_from_and_to_versions_and_returns_evidence(self):
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "update.json"
            expected = {"from": "0.0.1", "to": "0.4.0", "notes": "fixed test note"}
            path.write_text(json.dumps(expected), encoding="utf-8")
            self.assertEqual(u.check_update_json(path, "0.0.1", "0.4.0"), expected)
            with self.assertRaisesRegex(u.CheckError, "do not match"):
                u.check_update_json(path, "0.0.2", "0.4.0")

    def test_missing_or_invalid_file_fails(self):
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "update.json"
            with self.assertRaisesRegex(u.CheckError, "could not read"):
                u.check_update_json(path, "0.0.1", "0.4.0")
            path.write_text("[]", encoding="utf-8")
            with self.assertRaisesRegex(u.CheckError, "not an object"):
                u.check_update_json(path, "0.0.1", "0.4.0")


class ShellLogonStartTest(unittest.TestCase):
    def test_value_matches_the_one_the_app_registers(self):
        # secs_sys::host::shell_autostart_value_quotes_the_executable_and_hides_the_window
        self.assertEqual(
            u.shell_autostart_value(PureWindowsPath(r"C:\Program Files\SECS 2R\secs2r-host-shell.exe")),
            r'"C:\Program Files\SECS 2R\secs2r-host-shell.exe" --hidden',
        )
        self.assertEqual((u.RUN_KEY, u.SHELL_RUN_VALUE),
                         (r"Software\Microsoft\Windows\CurrentVersion\Run", "SECS2RHostShell"))


class AppCloseTest(unittest.TestCase):
    def scenario(self):
        scenario = object.__new__(u.UpdateScenario)
        scenario.driver = mock.Mock()
        scenario.driver.session = "session-1"
        return scenario

    def test_close_sends_wm_close_to_the_one_app_and_retires_the_session(self):
        scenario = self.scenario()
        replies = iter(['{"pid":4242,"path":"C:\\\\Users\\\\runner\\\\SECS2R\\\\secs2r.exe"}', "True"])
        with mock.patch.object(u, "powershell", side_effect=lambda script: next(replies)) as shell:
            scenario.close_app_window()
        self.assertIn("Name='secs2r.exe'", shell.call_args_list[0].args[0])
        self.assertEqual(shell.call_args_list[1].args[0], "(Get-Process -Id 4242).CloseMainWindow()")
        scenario.driver.command.assert_not_called()
        self.assertIsNone(scenario.driver.session)

    def test_close_fails_without_exactly_one_app_or_a_main_window(self):
        for replies, message in ((["[]"], "expected one running secs2r.exe"),
                                 (['[{"pid":1,"path":"a"},{"pid":2,"path":"b"}]'], "expected one running"),
                                 (['{"pid":7,"path":"a"}', "False"], "has no main window")):
            scenario = self.scenario()
            answers = iter(replies)
            with mock.patch.object(u, "powershell", side_effect=lambda script: next(answers)):
                with self.assertRaisesRegex(u.CheckError, message):
                    scenario.close_app_window()
            self.assertIsNone(scenario.driver.session)


class ReportTest(unittest.TestCase):
    def test_report_marks_later_steps_not_run_and_escapes_evidence(self):
        report = u.render_report([
            {"number": 1, "result": "PASS", "detail": "served | signed"},
            {"number": 2, "result": "FAIL", "detail": "wrong\nendpoint"},
        ], ["signature enabled"])
        self.assertIn("| 1. Serve the signed new build | PASS | served \\| signed |", report)
        self.assertIn("| 2. Confine child processes to the local endpoint | FAIL | wrong<br>endpoint |", report)
        self.assertIn("| 9. Clean up | NOT RUN |", report)
        self.assertIn("- signature enabled", report)


class StepOrderTest(unittest.TestCase):
    def test_fake_runner_records_the_fixed_order(self):
        class FakeRunner:
            def __init__(self):
                self.calls = []

            def run_step(self, number, operation):
                self.calls.append((number, operation.__name__))
                operation()

            def __getattr__(self, name):
                if name in u.STEP_METHODS:
                    def operation():
                        return name
                    operation.__name__ = name
                    return operation
                raise AttributeError(name)

        runner = FakeRunner()
        u.execute_steps(runner)
        self.assertEqual(runner.calls, list(enumerate(u.STEP_METHODS, 1)))

    def test_cleanup_still_runs_after_a_failed_step(self):
        class FakeRunner:
            def __init__(self):
                self.calls = []

            def run_step(self, number, operation):
                self.calls.append(operation.__name__)
                operation()

            def serve(self):
                raise u.CheckError("serve failed")

            def cleanup(self):
                pass

        runner = FakeRunner()
        with self.assertRaisesRegex(u.CheckError, "serve failed"):
            u.execute_steps(runner)
        self.assertEqual(runner.calls, ["serve", "cleanup"])


class ParserTest(unittest.TestCase):
    def test_run_parser_has_the_ci_command_arguments(self):
        args = u.parser().parse_args([
            "run", "--old", "old", "--new", "new", "--version", "0.2.0",
            "--asset-version", "0.0.0-ci.42", "--tauri-driver", "tauri-driver.exe",
            "--msedgedriver", "msedgedriver.exe", "--report", "report.md",
            "--artifacts", "artifacts",
        ])
        self.assertEqual(args.old, Path("old"))
        self.assertEqual(args.asset_version, "0.0.0-ci.42")
        self.assertEqual(args.tauri_driver, Path("tauri-driver.exe"))
        self.assertEqual(args.msedgedriver, Path("msedgedriver.exe"))
        self.assertIs(args.function, u.run)
        self.assertIn("set-version", u.parser().format_help())


if __name__ == "__main__":
    unittest.main()
