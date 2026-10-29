"""Unit tests for camilla_engine.py (python -m unittest, no network)."""
from __future__ import annotations

import hashlib
import io
import tarfile
import tempfile
import unittest
import zipfile
from pathlib import Path

import camilla_engine as c


class Response(io.BytesIO):
    def __enter__(self):
        return self

    def __exit__(self, *_args):
        self.close()


class ManifestTest(unittest.TestCase):
    def write(self, root: Path, body: str) -> Path:
        path = root / "engines.toml"
        path.write_text(body, encoding="utf-8")
        return path

    def test_manifest_requires_keys_digest_and_camilla_format(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            good = self.write(
                root,
                '[linux]\nversion="4.1.3"\nurl="https://example.test/x"\n'
                f'sha256="{"a" * 64}"\nformat="tar.gz"\n',
            )
            self.assertEqual(c.load_manifest(good, "linux", c.FORMATS)["version"], "4.1.3")
            for body in [
                '[linux]\nversion="1"\nurl="x"\nformat="tar.gz"\n',
                '[linux]\nversion="1"\nurl="x"\nsha256="AA"\nformat="zip"\n',
                f'[linux]\nversion="1"\nurl="x"\nsha256="{"a" * 64}"\nformat="nupkg"\n',
            ]:
                path = self.write(root, body)
                with self.assertRaises(c.EngineError):
                    c.load_manifest(path, "linux", c.FORMATS)


class DownloadTest(unittest.TestCase):
    def test_checksum_mismatch_removes_file(self):
        with tempfile.TemporaryDirectory() as temporary:
            target = Path(temporary) / "download" / "engine.bin"
            with self.assertRaisesRegex(c.EngineError, "checksum mismatch"):
                c.fetch_file(
                    "https://example.test/x",
                    "0" * 64,
                    target,
                    lambda _request: Response(b"bad"),
                )
            self.assertFalse(target.exists())
            self.assertFalse(target.with_name(".engine.bin.part").exists())

    def test_matching_digest_is_written(self):
        with tempfile.TemporaryDirectory() as temporary:
            target = Path(temporary) / "engine.bin"
            payload = b"engine"
            expected = hashlib.sha256(payload).hexdigest()
            self.assertTrue(
                c.fetch_file(
                    "https://example.test/x",
                    expected,
                    target,
                    lambda _request: Response(payload),
                )
            )
            self.assertEqual(target.read_bytes(), payload)


class ExtractionTest(unittest.TestCase):
    def tar_package(self, root: Path, entries: dict[str, bytes]) -> Path:
        path = root / "engine.tar.gz"
        with tarfile.open(path, "w:gz") as archive:
            for name, body in entries.items():
                info = tarfile.TarInfo(name)
                info.size = len(body)
                archive.addfile(info, io.BytesIO(body))
        return path

    def test_tar_extracts_only_the_binary(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            package = self.tar_package(
                root, {"release/camilladsp": b"binary", "release/readme": b"skip"}
            )
            binary = c.extract_tar(package, root / "engine")
            self.assertEqual(binary.read_bytes(), b"binary")
            self.assertFalse((root / "engine" / "readme").exists())

    def test_tar_member_that_escapes_destination_is_refused(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            for name in ["../escape", "/absolute", "C:\\escape"]:
                package = self.tar_package(
                    root, {name: b"bad", "camilladsp": b"binary"}
                )
                with self.assertRaisesRegex(c.EngineError, "unsafe archive path"):
                    c.extract_tar(package, root / "engine")

    def test_zip_extracts_windows_binary_and_rejects_escape(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            package = root / "engine.zip"
            with zipfile.ZipFile(package, "w") as archive:
                archive.writestr("release/camilladsp.exe", b"binary")
            binary = c.extract_zip(package, root / "engine")
            self.assertEqual(binary.read_bytes(), b"binary")
            with zipfile.ZipFile(package, "w") as archive:
                archive.writestr("../escape", b"bad")
                archive.writestr("camilladsp.exe", b"binary")
            with self.assertRaisesRegex(c.EngineError, "unsafe archive path"):
                c.extract_zip(package, root / "other")


class EnvironmentTest(unittest.TestCase):
    def test_github_env_has_absolute_binary_and_version(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            binary = root / "engine" / "camilladsp"
            env = root / "env"
            c.write_github_env(env, binary, "4.1.3")
            self.assertEqual(
                env.read_text(encoding="utf-8").splitlines(),
                [
                    f"SECS_CAMILLADSP={binary.resolve()}",
                    "SECS_CAMILLADSP_VERSION=4.1.3",
                ],
            )


if __name__ == "__main__":
    unittest.main()
