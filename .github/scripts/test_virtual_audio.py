"""Pure/mocked tests; never install a driver or query this machine's audio services."""
from __future__ import annotations

import argparse
import base64
import io
import json
import subprocess
import tempfile
import unittest
import zipfile
from pathlib import Path
from unittest import mock

import virtual_audio as v


class PackageTest(unittest.TestCase):
    def test_official_pin(self):
        entry = v.load_manifest(v.DEFAULT_MANIFEST, "scream", {"zip"})
        self.assertEqual(entry["version"], "4.0")
        self.assertEqual(entry["url"], "https://github.com/duncanthrax/scream/releases/download/4.0/Scream4.0.zip")
        self.assertEqual(entry["sha256"], "fa33e25f9a46c61e4e0cd83362c51c3d2a45c6fe4091aad7507e240e40f1a520")

    def test_only_fixed_members_are_extracted(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            package = root / "scream.zip"
            with zipfile.ZipFile(package, "w") as archive:
                for name in v.MEMBERS:
                    archive.writestr(name, name)
                archive.writestr("../escape", "ignored")
                archive.writestr("Install/Install-x64.bat", "never execute")
            devcon, inf = v.extract_package(package, root / "output")
            self.assertEqual(devcon.read_text(), v.MEMBERS[0])
            self.assertEqual(inf.read_text(), v.MEMBERS[1])
            self.assertEqual(len(list((root / "output").rglob("*.*"))), 4)
            self.assertFalse((root / "escape").exists())
            with self.assertRaisesRegex(v.EngineError, "not empty"):
                v.extract_package(package, root / "output")

    def test_missing_and_linked_members_are_refused_before_writing(self):
        for linked in (False, True):
            with self.subTest(linked=linked), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                package = root / "scream.zip"
                with zipfile.ZipFile(package, "w") as archive:
                    for name in v.MEMBERS[1:]:
                        archive.writestr(name, b"data")
                    if linked:
                        member = zipfile.ZipInfo(v.MEMBERS[0])
                        member.external_attr = 0o120777 << 16
                        archive.writestr(member, "target")
                with self.assertRaisesRegex(v.EngineError, "missing, duplicate or linked"):
                    v.extract_package(package, root / "output")
                self.assertFalse((root / "output").exists())

    def test_checksum_failure_leaves_no_executable_or_archive(self):
        with tempfile.TemporaryDirectory() as directory:
            target = Path(directory) / "scream.zip"
            with self.assertRaisesRegex(v.EngineError, "checksum mismatch"):
                v.fetch_file("https://example.invalid/package", "0" * 64, target,
                             opener=lambda _: io.BytesIO(b"not the pinned release"))
            self.assertFalse(target.exists())
            self.assertEqual(list(Path(directory).iterdir()), [])


class EndpointTest(unittest.TestCase):
    def test_enumerates_both_active_directions_and_keeps_empty_capture(self):
        facts = {"render": [{"id": "opaque", "name": "Speakers (Scream (WDM))"}], "capture": []}
        with mock.patch.object(v, "powershell", return_value=json.dumps(facts)) as run:
            self.assertEqual(v.endpoints(), facts)
        script = run.call_args.args[0]
        self.assertIn("[AudioEndpoints]::Read(0)", script)
        self.assertIn("[AudioEndpoints]::Read(1)", script)
        self.assertIn("EnumAudioEndpoints(flow, 1", script)
        self.assertEqual(v.scream_endpoint(facts), facts["render"][0])

    def test_no_ambiguous_or_non_scream_fallback(self):
        for render in ([], [{"name": "Hardware speakers"}], [{"name": "Scream"}, {"name": "Scream 2"}]):
            with self.assertRaisesRegex(v.EngineError, "Expected one active Scream"):
                v.scream_endpoint({"render": render, "capture": []})
        with mock.patch.object(v, "powershell", return_value='{"render":{},"capture":[]}'):
            with self.assertRaisesRegex(v.EngineError, "invalid list"):
                v.endpoints()

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


class InstallTest(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.args = argparse.Namespace(manifest=str(v.DEFAULT_MANIFEST), dest=str(self.root))

    def test_refuses_local_execution_before_download_or_service_changes(self):
        with mock.patch.dict(v.os.environ, {}, clear=True), mock.patch.object(v, "fetch_file") as fetch, mock.patch.object(v, "powershell") as shell:
            with self.assertRaisesRegex(v.EngineError, "disposable GitHub-hosted"):
                v.install(self.args)
            fetch.assert_not_called()
            shell.assert_not_called()

    def install_with_mocks(self, code=0, signature="Valid", endpoint=None):
        devcon, inf = self.root / "devcon-x64.exe", self.root / "Scream.inf"
        facts = endpoint if endpoint is not None else {"render": [{"id": "id", "name": "Speakers (Scream (WDM))"}], "capture": []}
        with mock.patch.object(v, "runner_only"), mock.patch.object(v, "fetch_file") as fetch, \
             mock.patch.object(v, "extract_package", return_value=(devcon, inf)), \
             mock.patch.object(v, "powershell", return_value=signature) as shell, \
             mock.patch.object(v.subprocess, "run", return_value=subprocess.CompletedProcess([], code, "mock install", "mock error")) as run, \
             mock.patch.object(v, "endpoints", return_value=facts), \
             mock.patch.object(v.time, "monotonic", side_effect=[0, 61]):
            v.install(self.args)
        return fetch, shell, run

    def test_uses_verified_pin_release_command_and_audio_services(self):
        fetch, shell, run = self.install_with_mocks()
        entry = v.load_manifest(v.DEFAULT_MANIFEST, "scream", {"zip"})
        self.assertEqual(fetch.call_args.args[:2], (entry["url"], entry["sha256"]))
        self.assertEqual(run.call_args.args[0], [str(self.root / "devcon-x64.exe"), "install", str(self.root / "Scream.inf"), "*Scream"])
        self.assertEqual(run.call_args.kwargs["stdin"], subprocess.DEVNULL)
        self.assertEqual(run.call_args.kwargs["timeout"], 120)
        self.assertIn("'AudioEndpointBuilder','Audiosrv'", shell.call_args_list[0].args[0])
        self.assertIn("Start-Service", shell.call_args_list[0].args[0])
        self.assertEqual(len(shell.call_args_list), 3)
        self.assertIn("Get-AuthenticodeSignature", shell.call_args_list[1].args[0])
        self.assertIn("scream.cat", shell.call_args_list[2].args[0])

    def test_nonzero_and_reboot_results_fail(self):
        for code in (1, 2, 3):
            with self.subTest(code=code), self.assertRaisesRegex(v.EngineError, "failed or requires reboot"):
                self.install_with_mocks(code=code)

    def test_untrusted_signature_and_missing_endpoint_fail(self):
        with self.assertRaisesRegex(v.EngineError, "signature is not valid"):
            self.install_with_mocks(signature="NotTrusted")
        with self.assertRaisesRegex(v.EngineError, "Expected one active Scream"):
            self.install_with_mocks(endpoint={"render": [], "capture": []})


if __name__ == "__main__":
    unittest.main()
