"""Unit tests for updater_manifest.py (python -m unittest, no dependencies)."""
import hashlib
import json
import tempfile
import unittest
from pathlib import Path

import updater_manifest as u


class LatestManifestTest(unittest.TestCase):
    VERSION = "0.36.0"

    def setUp(self):
        self.dir = tempfile.TemporaryDirectory()
        self.root = Path(self.dir.name)
        self.assets = self.root / "assets"
        self.assets.mkdir()
        for _group, suffix, _keys in u.UPDATER_ASSETS:
            asset = self.assets / f"SECS2R-{self.VERSION}{suffix}"
            asset.write_bytes(asset.name.encode())
            Path(f"{asset}.sig").write_text(f"signature:{asset.name}", encoding="utf-8")

    def tearDown(self):
        self.dir.cleanup()

    def manifest(self, previous: str = ""):
        return u.latest_manifest(
            self.VERSION,
            previous,
            "release notes\n",
            self.assets,
            "https://example.invalid/releases/v0.36.0/",
            "2026-10-30T12:34:56Z",
        )

    def test_platform_map_matches_plugin_target_selection(self):
        manifest = self.manifest()
        self.assertEqual(set(manifest["platforms"]), {
            "windows-x86_64",
            "windows-x86_64-nsis",
            "darwin-aarch64",
            "darwin-x86_64",
            "darwin-aarch64-app",
            "darwin-x86_64-app",
            "linux-x86_64",
            "linux-x86_64-appimage",
            "linux-x86_64-deb",
        })
        windows = manifest["platforms"]["windows-x86_64-nsis"]
        self.assertEqual(
            windows["url"],
            "https://example.invalid/releases/v0.36.0/SECS2R-0.36.0-windows-x64-setup.exe",
        )
        self.assertEqual(
            windows["signature"],
            "signature:SECS2R-0.36.0-windows-x64-setup.exe",
        )
        self.assertEqual(
            manifest["platforms"]["darwin-aarch64-app"],
            manifest["platforms"]["darwin-x86_64-app"],
        )
        self.assertEqual(
            manifest["platforms"]["linux-x86_64"],
            manifest["platforms"]["linux-x86_64-appimage"],
        )

    def test_missing_asset_is_an_error(self):
        (self.assets / f"SECS2R-{self.VERSION}-linux-amd64.deb").unlink()
        with self.assertRaisesRegex(SystemExit, "required updater asset is missing"):
            self.manifest()

    def test_missing_signature_is_an_error(self):
        (self.assets / f"SECS2R-{self.VERSION}-linux-amd64.deb.sig").unlink()
        with self.assertRaisesRegex(SystemExit, "required updater signature is missing"):
            self.manifest()

    def test_asset_version_mismatch_is_an_error(self):
        with self.assertRaisesRegex(SystemExit, "asset version does not match 0.37.0"):
            u.latest_manifest(
                "0.37.0",
                "",
                "notes",
                self.assets,
                "https://example.invalid/download",
                "2026-10-30T12:34:56+00:00",
            )

    def test_previous_is_omitted_when_empty_and_present_when_set(self):
        self.assertNotIn("previous", self.manifest())
        self.assertEqual(self.manifest("v0.35.1")["previous"], "v0.35.1")

    def test_only_requires_and_writes_selected_platform_groups(self):
        for group, suffix, _keys in u.UPDATER_ASSETS:
            if group != "windows":
                asset = self.assets / f"SECS2R-{self.VERSION}{suffix}"
                asset.unlink()
                Path(f"{asset}.sig").unlink()

        manifest = u.latest_manifest(
            self.VERSION,
            "",
            "notes",
            self.assets,
            "https://example.invalid/download",
            "2026-10-30T12:34:56Z",
            {"windows"},
        )

        self.assertEqual(set(manifest["platforms"]), {
            "windows-x86_64", "windows-x86_64-nsis"
        })

    def test_asset_version_can_differ_from_manifest_version(self):
        manifest = u.latest_manifest(
            "0.36.1",
            "",
            "notes",
            self.assets,
            "https://example.invalid/download",
            "2026-10-30T12:34:56Z",
            {"windows", "linux"},
            self.VERSION,
        )

        self.assertEqual(manifest["version"], "0.36.1")
        self.assertNotIn("darwin-aarch64", manifest["platforms"])
        self.assertIn(f"SECS2R-{self.VERSION}-windows-x64-setup.exe",
                      manifest["platforms"]["windows-x86_64"]["url"])
        self.assertIn(f"SECS2R-{self.VERSION}-linux-amd64.deb",
                      manifest["platforms"]["linux-x86_64-deb"]["url"])

    def test_latest_command_keeps_notes_text_and_writes_a_newline(self):
        notes = self.root / "notes.md"
        notes.write_text("first line\n\nlast line\n", encoding="utf-8")
        out = self.root / "latest.json"
        result = u.main([
            "latest",
            "--version", "0.36.1",
            "--asset-version", self.VERSION,
            "--only", "windows",
            "--only", "linux",
            "--previous", "",
            "--notes-file", str(notes),
            "--assets", str(self.assets),
            "--base-url", "https://example.invalid/download",
            "--pub-date", "2026-10-30T12:34:56.123Z",
            "--out", str(out),
        ])
        self.assertEqual(result, 0)
        self.assertTrue(out.read_bytes().endswith(b"\n"))
        written = json.loads(out.read_text(encoding="utf-8"))
        self.assertEqual(written["version"], "0.36.1")
        self.assertEqual(written["notes"], "first line\n\nlast line\n")
        self.assertNotIn("darwin-aarch64", written["platforms"])
        self.assertIn("windows-x86_64", written["platforms"])
        self.assertIn("linux-x86_64-deb", written["platforms"])


class HostManifestTest(unittest.TestCase):
    def test_host_manifest_bytes_are_sorted_and_end_in_a_newline(self):
        with tempfile.TemporaryDirectory() as name:
            root = Path(name)
            exe = root / "built-host.exe"
            exe.write_bytes(b"host executable bytes\n")
            out = root / "secs2r-host.json"

            result = u.main([
                "host",
                "--exe", str(exe),
                "--version", "0.36.0",
                "--out", str(out),
            ])

            sha256 = hashlib.sha256(exe.read_bytes()).hexdigest()
            expected = (
                "{\n"
                '  "file": "secs2r-host.exe",\n'
                f'  "sha256": "{sha256}",\n'
                '  "version": "0.36.0"\n'
                "}\n"
            ).encode()
            self.assertEqual(result, 0)
            self.assertEqual(out.read_bytes(), expected)


if __name__ == "__main__":
    unittest.main()
