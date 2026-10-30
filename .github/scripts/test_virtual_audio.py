"""Pure/mocked tests; never install a driver or query this machine's audio services."""
from __future__ import annotations

import argparse
import base64
import io
import json
import subprocess
import tempfile
import unittest
import warnings
import zipfile
from pathlib import Path
from unittest import mock

import virtual_audio as v


VALID = {"status": "Valid", "message": "Signature verified.", "signer": "CN=VB-Audio",
         "thumbprint": "A" * 40, "timestamper": "CN=Timestamp", "publisher_trusted": False}
WINDOWS = {**VALID, "signer": "CN=Microsoft Windows Hardware Compatibility Publisher, O=Microsoft Corporation, C=US"}
RENDER = {"id": "opaque", "name": "CABLE Input (VB-Audio Virtual Cable)"}
MULTICHANNEL = {"id": "16ch", "name": "CABLE In 16ch (VB-Audio Virtual Cable)"}
CAPTURE = {"id": "capture", "name": "CABLE Output (VB-Audio Virtual Cable)"}


class PackageTest(unittest.TestCase):
    def test_official_pin(self):
        entry = v.load_manifest(v.DEFAULT_MANIFEST, "vbcable", {"zip"})
        self.assertEqual(entry["version"], "Pack45")
        self.assertEqual(entry["url"], "https://download.vb-audio.com/Download_CABLE/VBCABLE_Driver_Pack45.zip")
        self.assertEqual(entry["sha256"], "b950e39f01af1d04ea623c8f6d8eb9b6ea5c477c637295fabf20631c85116bfb")
        self.assertEqual(len(v.MEMBERS), 31)
        self.assertIn("vbMmeCable64_win10.inf", v.MEMBERS)
        self.assertIn("vbaudio_cable64_win10.sys", v.MEMBERS)
        self.assertIn("VBCABLE_ControlPanel.exe", v.MEMBERS)

    def test_only_fixed_members_are_extracted(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            package = root / "vbcable.zip"
            with zipfile.ZipFile(package, "w") as archive:
                for name in v.MEMBERS:
                    archive.writestr(name, name)
                archive.writestr("../escape", "ignored")
                archive.writestr("unapproved.exe", "never execute")
            installer, catalog = v.extract_package(package, root / "output")
            self.assertEqual(installer.read_text(), v.INSTALLER)
            self.assertEqual(catalog.read_text(), v.CATALOG)
            self.assertEqual({p.name for p in (root / "output").iterdir()}, set(v.MEMBERS))
            self.assertFalse((root / "escape").exists())
            with self.assertRaisesRegex(v.EngineError, "not empty"):
                v.extract_package(package, root / "output")

    def test_missing_duplicate_and_linked_members_are_refused_before_writing(self):
        for kind in ("missing", "duplicate", "linked"):
            with self.subTest(kind=kind), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                package = root / "vbcable.zip"
                with zipfile.ZipFile(package, "w") as archive:
                    for name in v.MEMBERS[1:]:
                        archive.writestr(name, b"data")
                    if kind == "linked":
                        member = zipfile.ZipInfo(v.MEMBERS[0])
                        member.external_attr = 0o120777 << 16
                        archive.writestr(member, "target")
                    if kind == "duplicate":
                        with warnings.catch_warnings():
                            warnings.simplefilter("ignore", UserWarning)
                            archive.writestr(v.MEMBERS[0], "one")
                            archive.writestr(v.MEMBERS[0], "two")
                with self.assertRaisesRegex(v.EngineError, "missing, duplicate or linked"):
                    v.extract_package(package, root / "output")
                self.assertFalse((root / "output").exists())

    def test_checksum_failure_leaves_no_executable_or_archive(self):
        with tempfile.TemporaryDirectory() as directory:
            target = Path(directory) / "vbcable.zip"
            with self.assertRaisesRegex(v.EngineError, "checksum mismatch"):
                v.fetch_file("https://example.invalid/package", "0" * 64, target,
                             opener=lambda _: io.BytesIO(b"not the pinned release"))
            self.assertFalse(target.exists())
            self.assertEqual(list(Path(directory).iterdir()), [])


class EndpointTest(unittest.TestCase):
    def test_enumerates_both_active_directions_and_keeps_empty_capture(self):
        for capture in ([], [CAPTURE]):
            facts = {"render": [MULTICHANNEL, RENDER], "capture": capture}
            with mock.patch.object(v, "powershell", return_value=json.dumps(facts)) as run:
                self.assertEqual(v.endpoints(), facts)
            script = run.call_args.args[0]
            self.assertIn("[AudioEndpoints]::Read(0)", script)
            self.assertIn("[AudioEndpoints]::Read(1)", script)
            self.assertIn("EnumAudioEndpoints(flow, 1", script)
            self.assertEqual(v.virtual_output_endpoint(facts), RENDER)

    def test_no_ambiguous_capture_multichannel_or_unrelated_fallback(self):
        for render in ([], [{"name": "Hardware speakers"}], [RENDER, RENDER], [MULTICHANNEL],
                       [CAPTURE], [{"name": "CABLE-A Input (VB-Audio Cable A)"}],
                       [{"name": RENDER["name"] + " impostor"}]):
            with self.subTest(render=render), self.assertRaisesRegex(v.EngineError, "Expected one active VB-CABLE"):
                v.virtual_output_endpoint({"render": render, "capture": [RENDER]})
        with mock.patch.object(v, "powershell", return_value='{"render":{},"capture":[]}'):
            with self.assertRaisesRegex(v.EngineError, "invalid list"):
                v.endpoints()

    def test_endpoint_name_is_case_insensitive(self):
        device = {**RENDER, "name": RENDER["name"].lower()}
        self.assertEqual(v.virtual_output_endpoint({"render": [device]}), device)

    def test_powershell_is_noninteractive_and_errors_are_not_hidden(self):
        with mock.patch.object(v.subprocess, "run", return_value=subprocess.CompletedProcess([], 0, "[]\n", "")) as run:
            self.assertEqual(v.powershell("test"), "[]")
        command = run.call_args.args[0]
        self.assertIn("-NonInteractive", command)
        self.assertIn("$ErrorActionPreference='Stop'", base64.b64decode(command[-1]).decode("utf-16le"))
        failed = subprocess.CompletedProcess([], 1, "", "Get-AuthenticodeSignature: module could not be loaded\n")
        with mock.patch.object(v.subprocess, "run", return_value=failed):
            with self.assertRaisesRegex(v.EngineError, "module could not be loaded"):
                v.powershell("test")

    def test_windows_powershell_does_not_inherit_the_pwsh_module_path(self):
        env = v.windows_powershell_env({"PSModulePath": "C:/pwsh/Modules", "Path": "C:/bin"})
        self.assertEqual(env, {"Path": "C:/bin"})
        self.assertEqual(v.windows_powershell_env({"PSMODULEPATH": "x"}), {})
        with mock.patch.object(v.subprocess, "run", return_value=subprocess.CompletedProcess([], 0, "", "")) as run, \
                mock.patch.dict(v.os.environ, {"PSModulePath": "C:/pwsh/Modules"}):
            v.powershell("test")
        self.assertNotIn("PSMODULEPATH", {key.upper() for key in run.call_args.kwargs["env"]})


class TrustTest(unittest.TestCase):
    def test_trust_is_only_needed_for_unknown_third_party_publishers(self):
        self.assertTrue(v.needs_publisher_trust(VALID))
        self.assertFalse(v.needs_publisher_trust({**VALID, "publisher_trusted": True}))
        self.assertFalse(v.needs_publisher_trust(WINDOWS))
        for subject in ("CN=Microsoft Windows Hardware Compatibility Publisher, O=Other",
                        WINDOWS["signer"].replace("Corporation,", "Corporation impostor,"),
                        "CN=Microsoft Corporation, O=Microsoft Corporation"):
            self.assertTrue(v.needs_publisher_trust({**VALID, "signer": subject}))

    def test_trust_helper_itself_refuses_local_execution(self):
        with mock.patch.dict(v.os.environ, {}, clear=True), mock.patch.object(v, "powershell") as shell:
            with self.assertRaisesRegex(v.EngineError, "disposable GitHub-hosted"):
                v.trust_catalog_publisher(Path("catalog.cat"), VALID)
            shell.assert_not_called()

    def test_import_rechecks_signature_and_exact_thumbprint_before_adding_only_signer(self):
        with mock.patch.object(v, "runner_only"), mock.patch.object(v, "powershell", return_value='{"action":"added"}') as shell:
            v.trust_catalog_publisher(Path("path'quoted") / v.CATALOG, VALID)
        script = shell.call_args.args[0]
        self.assertIn("path''quoted", script)
        self.assertLess(script.index("$s.Status -ne 'Valid'"), script.index("$store.Open"))
        self.assertLess(script.index("$s.SignerCertificate.Thumbprint -ne '" + "A" * 40), script.index("$store.Open"))
        self.assertIn("$s.StatusMessage,$s.SignerCertificate.Subject", script)
        self.assertIn("::new('TrustedPublisher','LocalMachine')", script)
        self.assertIn("$store.Add($s.SignerCertificate)", script)
        self.assertIn("finally { $store.Close() }", script)
        self.assertNotIn("AddRange", script)
        self.assertNotIn(".cer", script)


class InstallTest(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.args = argparse.Namespace(manifest=str(v.DEFAULT_MANIFEST), dest=str(self.root))
        self.installer, self.catalog = self.root / v.INSTALLER, self.root / v.CATALOG
        self.real_runner_only = v.runner_only
        self.guard = self.patch(v, "runner_only")
        self.fetch = self.patch(v, "fetch_file")
        self.extract = self.patch(v, "extract_package", return_value=(self.installer, self.catalog))
        self.shell = self.patch(v, "powershell", side_effect=[json.dumps(VALID), json.dumps(VALID), '{"action":"added"}', ""])
        self.run = self.patch(v.subprocess, "run", return_value=subprocess.CompletedProcess([], 0, "mock install", "mock error"))
        self.endpoints = self.patch(v, "endpoints", return_value={"render": [MULTICHANNEL, RENDER], "capture": [CAPTURE]})
        self.patch(v.time, "monotonic", side_effect=[0, 61])
        self.sleep = self.patch(v.time, "sleep")

    def patch(self, target, name, **kwargs):
        patcher = mock.patch.object(target, name, **kwargs)
        result = patcher.start()
        self.addCleanup(patcher.stop)
        return result

    def test_guard_failure_precedes_every_side_effect(self):
        self.guard.side_effect = v.EngineError("disposable GitHub-hosted")
        with self.assertRaisesRegex(v.EngineError, "disposable GitHub-hosted"):
            v.install(self.args)
        for operation in (self.fetch, self.extract, self.shell, self.run):
            operation.assert_not_called()

    def test_runner_guard_rejects_workstations_self_hosted_and_non_windows(self):
        # The patched entry point above tests ordering; call the real guard here.
        for platform, env in (("win32", {}), ("linux", {"GITHUB_ACTIONS": "true", "RUNNER_ENVIRONMENT": "github-hosted"}),
                              ("win32", {"GITHUB_ACTIONS": "true", "RUNNER_ENVIRONMENT": "self-hosted"})):
            with self.subTest(platform=platform, env=env), mock.patch.object(v.sys, "platform", platform), \
                    mock.patch.dict(v.os.environ, env, clear=True):
                with self.assertRaisesRegex(v.EngineError, "disposable GitHub-hosted"):
                    self.real_runner_only()

    def test_wrong_url_or_version_is_rejected_before_download(self):
        entry = v.load_manifest(v.DEFAULT_MANIFEST, "vbcable", {"zip"})
        for changes in ({"url": "https://example.invalid/VBCABLE_Driver_Pack45.zip"}, {"version": "Pack43"}):
            with mock.patch.object(v, "load_manifest", return_value={**entry, **changes}):
                with self.assertRaisesRegex(v.EngineError, "approved official Pack45"):
                    v.install(self.args)
        self.fetch.assert_not_called()
        self.shell.assert_not_called()

    def test_uses_verified_pin_silent_command_and_audio_services(self):
        with mock.patch("sys.stdout", new_callable=io.StringIO) as output:
            v.install(self.args)
        entry = v.load_manifest(v.DEFAULT_MANIFEST, "vbcable", {"zip"})
        self.assertEqual(self.fetch.call_args.args[:2], (entry["url"], entry["sha256"]))
        self.assertEqual(self.run.call_args.args[0], [str(self.installer), "-i", "-h"])
        self.assertEqual(self.run.call_args.kwargs["cwd"], self.installer.parent)
        self.assertEqual(self.run.call_args.kwargs["stdin"], subprocess.DEVNULL)
        self.assertEqual(self.run.call_args.kwargs["timeout"], 120)
        scripts = [call.args[0] for call in self.shell.call_args_list]
        self.assertEqual(len(scripts), 4)
        self.assertIn(v.INSTALLER, scripts[0])
        self.assertIn(v.CATALOG, scripts[1])
        self.assertIn("$store.Add($s.SignerCertificate)", scripts[2])
        self.assertIn("'AudioEndpointBuilder','Audiosrv'", scripts[3])
        self.assertIn("Start-Service", scripts[3])
        self.assertIn('TrustedPublisher: {"action":"added"}', output.getvalue())
        self.assertEqual(json.loads(output.getvalue().splitlines()[-1]),
                         {"version": entry["version"], "sha256": entry["sha256"], "endpoint": RENDER})

    def test_trust_step_is_skipped_for_windows_or_already_trusted_catalog(self):
        for facts in (WINDOWS, {**VALID, "publisher_trusted": True}):
            self.shell.reset_mock()
            self.shell.side_effect = [json.dumps(VALID), json.dumps(facts), ""]
            with mock.patch.object(v.time, "monotonic", return_value=0), mock.patch("sys.stdout", new_callable=io.StringIO) as output:
                v.install(self.args)
            self.assertEqual(self.shell.call_count, 3)
            self.assertTrue(all("$store.Add" not in call.args[0] for call in self.shell.call_args_list))
            self.assertIn("TrustedPublisher unchanged", output.getvalue())

    def test_invalid_installer_or_catalog_stops_before_trust_services_and_execution(self):
        for index in (0, 1):
            for status in ("NotSigned", "NotTrusted", "HashMismatch", "UnknownError"):
                with self.subTest(index=index, status=status):
                    bad = {**VALID, "status": status, "message": "Windows signature reason"}
                    self.shell.reset_mock()
                    self.shell.side_effect = [json.dumps(VALID)] * index + [json.dumps(bad)]
                    with self.assertRaisesRegex(v.EngineError, status + ": Windows signature reason .*signer: CN=VB-Audio"):
                        v.install(self.args)
                    self.assertEqual(self.shell.call_count, index + 1)
                    self.run.assert_not_called()
                    self.endpoints.assert_not_called()

    def test_missing_signer_fails_before_trust_and_execution(self):
        self.shell.side_effect = [json.dumps({**VALID, "thumbprint": ""})]
        with self.assertRaisesRegex(v.EngineError, "no signer certificate"):
            v.install(self.args)
        self.run.assert_not_called()

    def test_failed_trust_prevents_services_and_installer(self):
        self.shell.side_effect = [json.dumps(VALID), json.dumps(VALID), v.EngineError("catalog signer changed")]
        with self.assertRaisesRegex(v.EngineError, "catalog signer changed"):
            v.install(self.args)
        self.assertEqual(self.shell.call_count, 3)
        self.run.assert_not_called()

    def test_nonzero_and_reboot_results_fail_and_preserve_stderr(self):
        for code in (1, 2, 3, 3010):
            with self.subTest(code=code):
                self.shell.side_effect = [json.dumps(VALID), json.dumps(VALID), "added", ""]
                self.run.return_value = subprocess.CompletedProcess([], code, "", "installer stderr")
                with self.assertRaisesRegex(v.EngineError, r"failed or requires reboot .*installer stderr"):
                    v.install(self.args)
                self.endpoints.assert_not_called()

    def test_missing_endpoint_fails_after_deadline(self):
        self.endpoints.return_value = {"render": [], "capture": [CAPTURE]}
        with self.assertRaisesRegex(v.EngineError, "Expected one active VB-CABLE"):
            v.install(self.args)
        self.sleep.assert_not_called()

    def test_waits_for_render_endpoint_after_successful_installer(self):
        self.endpoints.side_effect = [{"render": [], "capture": []}, {"render": [RENDER], "capture": [CAPTURE]}]
        with mock.patch.object(v.time, "monotonic", side_effect=[0, 1]):
            v.install(self.args)
        self.sleep.assert_called_once_with(1)
        self.assertEqual(self.endpoints.call_count, 2)


if __name__ == "__main__":
    unittest.main()
