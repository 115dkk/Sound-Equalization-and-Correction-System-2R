"""Unit tests for eqapo_engine.py (python -m unittest, no dependencies)."""
from __future__ import annotations

import hashlib
import io
import sys
import tempfile
import unittest
import zipfile
from pathlib import Path
from unittest import mock

import eqapo_engine as e


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

    def test_manifest_requires_keys_digest_and_known_format(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            good = self.write(
                root,
                '[xt]\nversion="1"\nurl="https://example.test/x"\n'
                f'sha256="{"a" * 64}"\nformat="nupkg"\n',
            )
            self.assertEqual(e.load_manifest(good, "xt")["version"], "1")
            for body in [
                '[xt]\nversion="1"\nurl="x"\nformat="nupkg"\n',
                '[xt]\nversion="1"\nurl="x"\nsha256="AA"\nformat="nupkg"\n',
                f'[xt]\nversion="1"\nurl="x"\nsha256="{"a" * 64}"\nformat="tar"\n',
            ]:
                path = self.write(root, body)
                with self.assertRaises(e.EngineError):
                    e.load_manifest(path, "xt")


class DownloadTest(unittest.TestCase):
    def test_checksum_mismatch_removes_file(self):
        with tempfile.TemporaryDirectory() as temporary:
            target = Path(temporary) / "download" / "engine.bin"
            expected = "0" * 64
            with self.assertRaisesRegex(e.EngineError, "checksum mismatch"):
                e.fetch_file(
                    "https://example.test/x", expected, target, lambda _request: Response(b"bad")
                )
            self.assertFalse(target.exists())
            self.assertFalse(target.with_name(".engine.bin.part").exists())

    def test_matching_cache_is_reused_without_opening(self):
        with tempfile.TemporaryDirectory() as temporary:
            target = Path(temporary) / "engine.bin"
            target.write_bytes(b"cached")
            expected = hashlib.sha256(b"cached").hexdigest()
            self.assertFalse(
                e.fetch_file(
                    "https://example.test/x",
                    expected,
                    target,
                    lambda _request: self.fail("cache must avoid network"),
                )
            )

    def test_transient_failure_retries_with_two_four_eight_seconds(self):
        with tempfile.TemporaryDirectory() as temporary:
            target = Path(temporary) / "engine.bin"
            payload = b"downloaded"
            expected = hashlib.sha256(payload).hexdigest()
            attempts = 0
            sleeps = []

            def opening(_request):
                nonlocal attempts
                attempts += 1
                if attempts < 4:
                    raise OSError("temporary")
                return Response(payload)

            self.assertTrue(e.fetch_file("https://example.test/x", expected, target, opening, sleeps.append))
            self.assertEqual(attempts, 4)
            self.assertEqual(sleeps, [2, 4, 8])
            self.assertEqual(target.read_bytes(), payload)


class ExtractionTest(unittest.TestCase):
    def package(self, root: Path, entries: dict[str, bytes]) -> Path:
        path = root / "engine.nupkg"
        with zipfile.ZipFile(path, "w") as archive:
            for name, body in entries.items():
                archive.writestr(name, body)
        return path

    def test_nupkg_extracts_only_lib_app_without_prefix(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            package = self.package(
                root,
                {"lib/app/tools/Benchmark.exe": b"exe", "content/readme": b"skip"},
            )
            destination = root / "engine"
            e.extract_nupkg(package, destination)
            self.assertEqual((destination / "tools/Benchmark.exe").read_bytes(), b"exe")
            self.assertFalse((destination / "content/readme").exists())

    def test_nupkg_rejects_zip_slip_even_outside_lib_app(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            for name in ["../escape", "/absolute", "C:\\escape"]:
                package = self.package(root, {name: b"bad"})
                with self.assertRaisesRegex(e.EngineError, "unsafe nupkg path"):
                    e.extract_nupkg(package, root / "engine")

    def test_benchmark_finder_requires_exactly_one_case_insensitive_match(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            with self.assertRaisesRegex(e.EngineError, "found 0"):
                e.find_benchmark(root)
            (root / "BENCHMARK.EXE").write_bytes(b"one")
            self.assertEqual(e.find_benchmark(root).name, "BENCHMARK.EXE")
            other = root / "nested"
            other.mkdir()
            (other / "Benchmark.exe").write_bytes(b"two")
            with self.assertRaisesRegex(e.EngineError, "found 2"):
                e.find_benchmark(root)


class EnvironmentTest(unittest.TestCase):
    def test_github_env_has_absolute_paths_and_optional_config(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            benchmark = root / "engine" / "Benchmark.exe"
            config = root / "config"
            env = root / "env"
            e.write_github_env(env, benchmark, "original", config)
            self.assertEqual(
                env.read_text(encoding="utf-8").splitlines(),
                [
                    f"SECS_EQAPO_BENCHMARK={benchmark.resolve()}",
                    "SECS_EQAPO_ENGINE=original",
                    f"SECS_EQAPO_CONFIG_DIR={config.resolve()}",
                ],
            )

    @unittest.skipIf(sys.platform == "win32", "non-Windows behavior")
    def test_register_is_rejected_off_windows(self):
        with self.assertRaisesRegex(e.EngineError, "requires Windows"):
            e.register_config(Path("unused"))


if __name__ == "__main__":
    unittest.main()
