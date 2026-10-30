"""Unit tests for collect_bundles.py (python -m unittest, no dependencies)."""
import struct
import sys
import tarfile
import tempfile
import unittest
import zipfile
from pathlib import Path
from unittest import mock

import collect_bundles as c


def fake_pe(subsystem: int) -> bytes:
    data = bytearray(512)
    data[0:2] = b"MZ"
    struct.pack_into("<I", data, 0x3C, 0x80)
    data[0x80:0x84] = b"PE\0\0"
    struct.pack_into("<H", data, 0x80 + 4 + 20 + 68, subsystem)
    return bytes(data)


class NamesTest(unittest.TestCase):
    def test_asset_names_are_stable(self):
        self.assertEqual(c.asset_name("0.1.0", "nsis"), "SECS2R-0.1.0-windows-x64-setup.exe")
        self.assertEqual(c.asset_name("0.1.0", "nsis-sig"), "SECS2R-0.1.0-windows-x64-setup.exe.sig")
        self.assertEqual(c.asset_name("0.1.0", "portable"), "SECS2R-0.1.0-windows-x64-portable.zip")
        self.assertEqual(c.asset_name("0.1.0", "cli-windows"), "SECS2R-0.1.0-windows-x64-cli.zip")
        self.assertEqual(c.asset_name("0.1.0", "dmg"), "SECS2R-0.1.0-macos-universal.dmg")
        self.assertEqual(c.asset_name("0.1.0", "updater-macos"), "SECS2R-0.1.0-macos-universal.app.tar.gz")
        self.assertEqual(c.asset_name("0.1.0", "updater-macos-sig"), "SECS2R-0.1.0-macos-universal.app.tar.gz.sig")
        self.assertEqual(c.asset_name("0.1.0", "cli-macos"), "SECS2R-0.1.0-macos-universal-cli.tar.gz")
        self.assertEqual(c.asset_name("0.1.0", "appimage"), "SECS2R-0.1.0-linux-x86_64.AppImage")
        self.assertEqual(c.asset_name("0.1.0", "appimage-sig"), "SECS2R-0.1.0-linux-x86_64.AppImage.sig")
        self.assertEqual(c.asset_name("0.1.0", "deb"), "SECS2R-0.1.0-linux-amd64.deb")
        self.assertEqual(c.asset_name("0.1.0", "deb-sig"), "SECS2R-0.1.0-linux-amd64.deb.sig")
        self.assertEqual(c.asset_name("0.1.0", "cli-linux"), "SECS2R-0.1.0-linux-x86_64-cli.tar.gz")

    def test_cli_version_line_matches_clap(self):
        self.assertEqual(c.expected_cli_version("0.31.0"), "secs2r-cli 0.31.0")


class PeTest(unittest.TestCase):
    def test_reads_subsystem(self):
        self.assertEqual(c.pe_subsystem(fake_pe(2)), 2)
        self.assertEqual(c.pe_subsystem(fake_pe(3)), 3)

    def test_rejects_non_pe(self):
        with self.assertRaises(ValueError):
            c.pe_subsystem(b"\x7fELF" + bytes(100))
        broken = bytearray(fake_pe(2))
        broken[0x80:0x84] = b"XX\0\0"
        with self.assertRaises(ValueError):
            c.pe_subsystem(bytes(broken))

    def test_subsystem_check_rejects_missing_and_wrong_executables(self):
        with tempfile.TemporaryDirectory() as name:
            root = Path(name)
            path = root / "app.exe"
            with self.assertRaises(SystemExit):
                c.check_pe_subsystem(path, c.PE_SUBSYSTEM_WINDOWS_GUI)
            path.write_bytes(fake_pe(c.PE_SUBSYSTEM_WINDOWS_CUI))
            with self.assertRaises(SystemExit):
                c.check_pe_subsystem(path, c.PE_SUBSYSTEM_WINDOWS_GUI)
            c.check_pe_subsystem(path, c.PE_SUBSYSTEM_WINDOWS_CUI)


class PortableArchiveTest(unittest.TestCase):
    def test_zip_holds_the_app_cli_service_and_gui_shell(self):
        with tempfile.TemporaryDirectory() as name:
            root = Path(name)
            files = {}
            for filename in [
                "secs2r.exe",
                "secs2r-cli.exe",
                "secs2r-host.exe",
                "secs2r-host-shell.exe",
                "secs2r-host.json",
                "secs2r-host.json.sig",
                "LICENSE",
            ]:
                files[filename] = root / filename
                files[filename].write_bytes(filename.encode())
            path = root / "portable.zip"
            c.write_portable_zip(
                path,
                files["secs2r.exe"],
                files["secs2r-cli.exe"],
                files["secs2r-host.exe"],
                files["secs2r-host-shell.exe"],
                files["LICENSE"],
                files["secs2r-host.json"],
                files["secs2r-host.json.sig"],
            )
            with zipfile.ZipFile(path) as zf:
                self.assertEqual(sorted(zf.namelist()), [
                    "SECS 2R/LICENSE.txt",
                    "SECS 2R/README.txt",
                    "SECS 2R/SECS 2R.exe",
                    "SECS 2R/portable",
                    "SECS 2R/secs2r-cli.exe",
                    "SECS 2R/secs2r-host-shell.exe",
                    "SECS 2R/secs2r-host.exe",
                    "SECS 2R/secs2r-host.json",
                    "SECS 2R/secs2r-host.json.sig",
                ])
                self.assertEqual(zf.read("SECS 2R/portable"), b"")
                self.assertEqual(zf.read("SECS 2R/secs2r-host.json"), b"secs2r-host.json")
                self.assertEqual(zf.read("SECS 2R/secs2r-host.json.sig"), b"secs2r-host.json.sig")
                self.assertIn("new release is available",
                              zf.read("SECS 2R/README.txt").decode("utf-8"))


class CliArchiveTest(unittest.TestCase):
    def setUp(self):
        self.dir = tempfile.TemporaryDirectory()
        self.root = Path(self.dir.name)
        self.binary = self.root / "secs2r-cli"
        self.binary.write_bytes(b"binary")
        self.license = self.root / "LICENSE"
        self.license.write_bytes(b"GPL")

    def tearDown(self):
        self.dir.cleanup()

    def test_zip_holds_exe_licence_and_readme(self):
        path = self.root / "cli.zip"
        c.write_cli_zip(path, self.binary, self.license)
        with zipfile.ZipFile(path) as zf:
            self.assertEqual(sorted(zf.namelist()), [
                "secs2r-cli/LICENSE.txt", "secs2r-cli/README.txt", "secs2r-cli/secs2r-cli.exe"])
            self.assertEqual(zf.read("secs2r-cli/secs2r-cli.exe"), b"binary")
            self.assertIn("secs2r-cli --help", zf.read("secs2r-cli/README.txt").decode("utf-8"))

    def test_tar_keeps_the_executable_bit(self):
        path = self.root / "cli.tar.gz"
        c.write_cli_tar(path, self.binary, self.license)
        with tarfile.open(path) as tf:
            members = {member.name: member for member in tf.getmembers()}
            self.assertEqual(sorted(members), [
                "secs2r-cli/LICENSE.txt", "secs2r-cli/README.txt", "secs2r-cli/secs2r-cli"])
            self.assertEqual(members["secs2r-cli/secs2r-cli"].mode, 0o755)
            self.assertEqual(members["secs2r-cli/LICENSE.txt"].mode, 0o644)
            self.assertEqual(tf.extractfile("secs2r-cli/secs2r-cli").read(), b"binary")


class CollectTest(unittest.TestCase):
    def setUp(self):
        self.dir = tempfile.TemporaryDirectory()
        self.repo = Path(self.dir.name)
        self.release = self.repo / "target" / "release"
        self.bundle = self.release / "bundle"
        self.out = self.repo / "dist"
        self.release.mkdir(parents=True)
        (self.repo / "LICENSE").write_bytes(b"GPL")

    def tearDown(self):
        self.dir.cleanup()

    def put(self, relative: str, data: bytes = b"asset") -> Path:
        path = self.release / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(data)
        return path

    @mock.patch.object(c, "check_cli")
    @mock.patch.object(c, "check_pe_subsystem")
    def test_windows_collects_signature_and_host_update_files(self, _check_pe, _check_cli):
        for filename in [
            "secs2r.exe", "secs2r-cli.exe", "secs2r-host.exe", "secs2r-host-shell.exe"
        ]:
            self.put(filename, filename.encode())
        self.put("secs2r-host.json", b"manifest")
        self.put("secs2r-host.json.sig", b"manifest-signature")
        self.put("bundle/nsis/SECS-setup.exe", b"installer")
        self.put("bundle/nsis/SECS-setup.exe.sig", b"installer-signature")

        made = c.collect("windows", "", "0.1.0", self.out, self.repo)

        self.assertEqual([path.name for path in made], [
            "SECS2R-0.1.0-windows-x64-setup.exe",
            "SECS2R-0.1.0-windows-x64-setup.exe.sig",
            "SECS2R-0.1.0-windows-x64-portable.zip",
            "SECS2R-0.1.0-windows-x64-cli.zip",
        ])
        self.assertEqual(made[1].read_bytes(), b"installer-signature")
        with zipfile.ZipFile(made[2]) as zf:
            self.assertEqual(zf.read("SECS 2R/portable"), b"")
            self.assertEqual(zf.read("SECS 2R/secs2r-host.json"), b"manifest")
            self.assertEqual(zf.read("SECS 2R/secs2r-host.json.sig"), b"manifest-signature")

    @mock.patch.object(c, "check_cli")
    def test_macos_collects_updater_archive_and_signature(self, _check_cli):
        self.put("secs2r-cli", b"cli")
        self.put("bundle/dmg/SECS.dmg", b"dmg")
        self.put("bundle/macos/SECS.app.tar.gz", b"updater")
        self.put("bundle/macos/SECS.app.tar.gz.sig", b"updater-signature")

        made = c.collect("macos", "", "0.1.0", self.out, self.repo)

        self.assertEqual([path.name for path in made], [
            "SECS2R-0.1.0-macos-universal.dmg",
            "SECS2R-0.1.0-macos-universal.app.tar.gz",
            "SECS2R-0.1.0-macos-universal.app.tar.gz.sig",
            "SECS2R-0.1.0-macos-universal-cli.tar.gz",
        ])
        self.assertEqual(made[1].read_bytes(), b"updater")
        self.assertEqual(made[2].read_bytes(), b"updater-signature")

    @mock.patch.object(c, "check_cli")
    def test_linux_collects_both_updater_signatures(self, _check_cli):
        self.put("secs2r-cli", b"cli")
        self.put("bundle/appimage/SECS.AppImage", b"appimage")
        self.put("bundle/appimage/SECS.AppImage.sig", b"appimage-signature")
        self.put("bundle/deb/SECS.deb", b"deb")
        self.put("bundle/deb/SECS.deb.sig", b"deb-signature")

        made = c.collect("linux", "", "0.1.0", self.out, self.repo)

        self.assertEqual([path.name for path in made], [
            "SECS2R-0.1.0-linux-x86_64.AppImage",
            "SECS2R-0.1.0-linux-x86_64.AppImage.sig",
            "SECS2R-0.1.0-linux-amd64.deb",
            "SECS2R-0.1.0-linux-amd64.deb.sig",
            "SECS2R-0.1.0-linux-x86_64-cli.tar.gz",
        ])
        self.assertEqual(made[1].read_bytes(), b"appimage-signature")
        self.assertEqual(made[3].read_bytes(), b"deb-signature")

    @mock.patch.object(c, "check_cli")
    def test_missing_signature_is_an_error(self, _check_cli):
        self.put("secs2r-cli", b"cli")
        self.put("bundle/appimage/SECS.AppImage", b"appimage")
        self.put("bundle/deb/SECS.deb", b"deb")
        self.put("bundle/deb/SECS.deb.sig", b"deb-signature")

        with self.assertRaises(SystemExit):
            c.collect("linux", "", "0.1.0", self.out, self.repo)

    @mock.patch.object(c, "check_cli")
    @mock.patch.object(c, "check_pe_subsystem")
    def test_missing_host_manifest_is_an_error(self, _check_pe, _check_cli):
        for filename in [
            "secs2r.exe", "secs2r-cli.exe", "secs2r-host.exe", "secs2r-host-shell.exe"
        ]:
            self.put(filename, filename.encode())
        self.put("bundle/nsis/SECS-setup.exe", b"installer")
        self.put("bundle/nsis/SECS-setup.exe.sig", b"installer-signature")

        with self.assertRaises(SystemExit):
            c.collect("windows", "", "0.1.0", self.out, self.repo)


class CheckCliTest(unittest.TestCase):
    def fake_cli(self, root: Path, line: str) -> Path:
        script = root / "fake_cli.py"
        script.write_text(f"print({line!r})\n", encoding="utf-8")
        if sys.platform == "win32":
            launcher = root / "fake_cli.cmd"
            launcher.write_text(f'@"{sys.executable}" "{script}" %*\n', encoding="utf-8")
        else:
            launcher = root / "fake_cli"
            launcher.write_text(f'#!/bin/sh\nexec "{sys.executable}" "{script}" "$@"\n', encoding="utf-8")
            launcher.chmod(0o755)
        return launcher

    def test_accepts_the_release_version_and_rejects_another(self):
        with tempfile.TemporaryDirectory() as name:
            root = Path(name)
            c.check_cli(self.fake_cli(root, "secs2r-cli 0.31.0"), "0.31.0")
            with self.assertRaises(SystemExit):
                c.check_cli(self.fake_cli(root, "secs2r-cli 0.30.0"), "0.31.0")


if __name__ == "__main__":
    unittest.main()
