"""Unit tests for reaper_engine.py; never execute REAPER or use the network."""
from __future__ import annotations

import hashlib
import io
import os
import tarfile
import tempfile
import unittest
from pathlib import Path
from unittest import mock

import reaper_engine as e


class Response(io.BytesIO):
    def __enter__(self):
        return self

    def __exit__(self, *_args):
        self.close()


class ManifestTest(unittest.TestCase):
    def test_checked_in_manifest(self):
        self.assertEqual(e.load_manifest(e.DEFAULT_MANIFEST)["version"], "7.84")

    def test_requires_keys_strings_digest_official_url_and_format(self):
        good = e.load_manifest(e.DEFAULT_MANIFEST)
        variants = [
            {key: value for key, value in good.items() if key != "sha256"},
            {**good, "version": ""}, {**good, "version": "7\n84"},
            {**good, "format": "zip"}, {**good, "sha256": "AB" * 32},
            {**good, "url": "http://www.reaper.fm/files/7.x/reaper784_linux_x86_64.tar.xz"},
            {**good, "url": "https://www.reaper.fm.evil.test/files/7.x/reaper784_linux_x86_64.tar.xz"},
            {**good, "url": "https://www.reaper.fm@evil.test/files/7.x/reaper784_linux_x86_64.tar.xz"},
            {**good, "url": good["url"] + "?redirect=elsewhere"},
            {**good, "url": good["url"] + "#fragment"},
        ]
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "engine.toml"
            for values in variants:
                with self.subTest(values=values):
                    import json
                    path.write_text("\n".join(f"{key}={json.dumps(value)}" for key, value in values.items()), encoding="utf-8")
                    with self.assertRaises(e.EngineError):
                        e.load_manifest(path)
            path.write_text('version=7\nurl="url"\nsha256="x"\nformat="tar.xz"')
            with self.assertRaises(e.EngineError):
                e.load_manifest(path)


class DownloadTest(unittest.TestCase):
    def test_digest_mismatch_removes_partial_download(self):
        with tempfile.TemporaryDirectory() as temporary:
            target = Path(temporary) / "engine.tar.xz"
            with self.assertRaisesRegex(e.EngineError, "checksum mismatch"):
                e.fetch_file("https://www.reaper.fm/x", "0" * 64, target,
                             lambda *_args, **_kwargs: Response(b"wrong"))
            self.assertFalse(target.exists())
            self.assertFalse(target.with_name(".engine.tar.xz.part").exists())

    def test_atomic_download_and_matching_cache(self):
        with tempfile.TemporaryDirectory() as temporary:
            target = Path(temporary) / "engine.tar.xz"
            payload = b"verified archive"
            digest = hashlib.sha256(payload).hexdigest()
            opener = mock.Mock(side_effect=lambda *_args, **_kwargs: Response(payload))
            self.assertTrue(e.fetch_file("https://www.reaper.fm/x", digest, target, opener))
            self.assertFalse(e.fetch_file("https://www.reaper.fm/x", digest, target, opener))
            self.assertEqual(opener.call_count, 1)
            self.assertEqual(target.read_bytes(), payload)

    def test_transient_errors_retry_but_bad_digest_does_not(self):
        with tempfile.TemporaryDirectory() as temporary:
            target = Path(temporary) / "engine.tar.xz"
            opener = mock.Mock(side_effect=[OSError("temporary"), Response(b"ok")])
            sleeper = mock.Mock()
            e.fetch_file("https://www.reaper.fm/x", hashlib.sha256(b"ok").hexdigest(), target, opener, sleeper)
            sleeper.assert_called_once_with(2)
            self.assertEqual(opener.call_count, 2)


class ExtractionTest(unittest.TestCase):
    def archive(self, root: Path, names: list[tuple[str, bytes | None, bytes]]) -> Path:
        path = root / "engine.tar.xz"
        with tarfile.open(path, "w:xz") as archive:
            for name, content, kind in names:
                member = tarfile.TarInfo(name)
                member.type = kind
                member.mode = 0o6755
                if kind in (tarfile.SYMTYPE, tarfile.LNKTYPE):
                    member.linkname = "../../outside"
                if content is not None:
                    member.size = len(content)
                archive.addfile(member, io.BytesIO(content) if content is not None else None)
        return path

    def test_regular_tree_is_confined_and_no_special_mode_survives(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            package = self.archive(root, [("reaper/REAPER", None, tarfile.DIRTYPE),
                                         ("reaper/REAPER/reaper", b"not executable code", tarfile.REGTYPE)])
            dest = root / "out"
            e.extract_package(package, dest)
            executable = e.find_reaper(dest)
            self.assertEqual(executable, (dest / "reaper/REAPER/reaper").resolve())
            self.assertEqual(executable.read_bytes(), b"not executable code")
            if os.name != "nt":
                self.assertEqual(executable.stat().st_mode & 0o7777, 0o755)

    def test_paths_links_devices_and_duplicates_are_rejected_before_writes(self):
        bad = [(name, b"bad", tarfile.REGTYPE) for name in [
            "../outside", "/absolute", "C:/outside", "C:outside", "\\\\server\\file",
            "safe/../../outside", "safe\\..\\outside", "safe/file:stream",
        ]]
        bad += [("link", None, kind) for kind in [tarfile.SYMTYPE, tarfile.LNKTYPE, tarfile.FIFOTYPE, tarfile.CHRTYPE]]
        bad.append(("safe", b"duplicate", tarfile.REGTYPE))
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            for index, member in enumerate(bad):
                with self.subTest(member=member):
                    package = self.archive(root, [("safe", b"safe", tarfile.REGTYPE), member])
                    dest = root / str(index)
                    with self.assertRaises(e.EngineError):
                        e.extract_package(package, dest)
                    self.assertEqual(list(dest.iterdir()), [])
                    self.assertFalse((root / "outside").exists())

    def test_existing_files_and_symlink_destination_are_not_followed(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            package = self.archive(root, [("reaper", b"new", tarfile.REGTYPE)])
            dest = root / "out"
            dest.mkdir()
            (dest / "reaper").write_bytes(b"preserve")
            with self.assertRaises(e.EngineError):
                e.extract_package(package, dest)
            self.assertEqual((dest / "reaper").read_bytes(), b"preserve")
            if os.name != "nt":
                link = root / "link"
                link.symlink_to(dest, target_is_directory=True)
                with self.assertRaises(e.EngineError):
                    e.extract_package(package, link)

    def test_finder_requires_one_executable(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            with self.assertRaisesRegex(e.EngineError, "found 0"):
                e.find_reaper(root)
            for name in ["a", "b"]:
                (root / name).mkdir()
                (root / name / "reaper").write_bytes(b"x")
            with self.assertRaisesRegex(e.EngineError, "found 2"):
                e.find_reaper(root)


class EnvironmentTest(unittest.TestCase):
    def test_writes_absolute_executable_path_only(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            path = root / "env"
            executable = root / "engine" / "reaper"
            e.write_github_env(path, executable)
            self.assertEqual(path.read_text(), f"SECS_REAPER={executable.resolve()}\n")


if __name__ == "__main__":
    unittest.main()
