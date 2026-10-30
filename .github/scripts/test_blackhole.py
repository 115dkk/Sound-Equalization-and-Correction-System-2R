"""Pure tests for BlackHole pins, downloads, discovery and schema-aware TCC SQL."""
import hashlib
import io
import json
import sqlite3
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

import blackhole as b


class BlackHoleTest(unittest.TestCase):
    def setUp(self):
        self.folder = tempfile.TemporaryDirectory()
        self.addCleanup(self.folder.cleanup)
        self.root = Path(self.folder.name)

    def test_checked_in_pin_is_the_official_2ch_package(self):
        pin = b.load_manifest(b.DEFAULT_MANIFEST)
        self.assertEqual(pin["version"], "0.7.1")
        self.assertEqual(pin["sha256"], "57b540f27a3e29c37e310e01bee0fdfab76733087e47f997ef9dccf851400dcf")
        self.assertEqual(pin["url"], "https://existential.audio/downloads/BlackHole2ch-0.7.1.pkg")

    def test_pin_rejects_missing_keys_bad_hash_format_and_wrong_package(self):
        original = b.DEFAULT_MANIFEST.read_text()
        for content in ("[blackhole]\n", original.replace("pkg\"", "zip\""),
                        original.replace("57b540", "ABCDEF"),
                        original.replace("BlackHole2ch-", "BlackHole16ch-")):
            path = self.root / "pin.toml"
            path.write_text(content)
            with self.assertRaises(b.BlackHoleError):
                b.load_manifest(path)

    def test_download_is_verified_and_cached(self):
        data = b"test installer"
        digest = hashlib.sha256(data).hexdigest()
        path = self.root / "download" / "driver.pkg"
        opened = []

        def opener(request, timeout):
            opened.append((request.full_url, timeout))
            return io.BytesIO(data)

        b.fetch_file("https://example.test/pkg", digest, path, opener=opener)
        b.fetch_file("https://example.test/pkg", digest, path, opener=opener)
        self.assertEqual(opened, [("https://example.test/pkg", 60)])
        self.assertEqual(path.read_bytes(), data)
        path.write_bytes(b"corrupt cache")
        b.fetch_file("https://example.test/pkg", digest, path, opener=opener)
        self.assertEqual(len(opened), 2)

    def test_bad_download_is_not_installed_or_kept(self):
        path = self.root / "driver.pkg"
        with self.assertRaisesRegex(b.BlackHoleError, "checksum mismatch"):
            b.fetch_file("https://example.test/pkg", "0" * 64, path,
                         opener=lambda *args, **kwargs: io.BytesIO(b"wrong"))
        self.assertFalse(path.exists())
        self.assertFalse(path.with_suffix(".pkg.part").exists())

    def test_network_failures_are_bounded(self):
        waits = []
        def opener(*args, **kwargs):
            raise OSError("offline")
        with self.assertRaisesRegex(b.BlackHoleError, "after 4 attempts"):
            b.fetch_file("https://example.test/pkg", "0" * 64, self.root / "driver.pkg",
                         opener=opener, sleeper=waits.append)
        self.assertEqual(waits, [2, 4, 8])

    def test_device_name_must_match_an_endpoint(self):
        self.assertTrue(b.has_device({"SPAudioDataType": [{"_items": [{"_name": b.DEVICE}]}]}))
        self.assertFalse(b.has_device({"SPAudioDataType": [{"_name": "BlackHole 16ch"}]}))
        self.assertFalse(b.has_device({"description": b.DEVICE}))

    def test_install_fails_if_device_never_appears(self):
        with patch.object(b, "fetch_file"), patch.object(b, "run", return_value="{}") as run, \
                patch.object(b.time, "monotonic", side_effect=[0, 1, 61]), \
                patch.object(b.time, "sleep"):
            with self.assertRaisesRegex(b.BlackHoleError, "did not appear"):
                b.install(self.root, b.DEFAULT_MANIFEST)
        self.assertIn(["sudo", "killall", "-9", "coreaudiod"], [call.args[0] for call in run.call_args_list])


class ConsentTest(unittest.TestCase):
    def database(self, extra=""):
        connection = sqlite3.connect(":memory:")
        self.addCleanup(connection.close)
        connection.row_factory = sqlite3.Row
        connection.execute("""CREATE TABLE access (
            service TEXT NOT NULL, client TEXT NOT NULL, client_type INTEGER NOT NULL,
            auth_value INTEGER NOT NULL, auth_reason INTEGER NOT NULL, auth_version INTEGER NOT NULL,
            csreq BLOB, policy_id INTEGER, indirect_object_identifier_type INTEGER,
            indirect_object_identifier TEXT NOT NULL DEFAULT 'UNUSED', indirect_object_code_identity BLOB,
            flags INTEGER, last_modified INTEGER NOT NULL""" + extra + ", PRIMARY KEY(service, client, client_type, indirect_object_identifier))")
        return connection

    def columns(self, connection):
        return [dict(row) for row in connection.execute("PRAGMA table_info(access)")]

    def test_older_and_newer_schemas_grant_only_bundle_microphone(self):
        for extra in ("", ", pid INTEGER, pid_version INTEGER, boot_uuid TEXT NOT NULL DEFAULT 'UNUSED', last_reminded INTEGER NOT NULL DEFAULT 0"):
            connection = self.database(extra)
            statement = b.consent_sql(self.columns(connection), "io.github.115dkk.secs2r", 123)
            connection.execute(statement)
            connection.execute(statement)
            rows = [dict(row) for row in connection.execute("SELECT * FROM access")]
            self.assertEqual(len(rows), 1)
            row = rows[0]
            self.assertEqual(row["service"], "kTCCServiceMicrophone")
            self.assertEqual(row["client"], "io.github.115dkk.secs2r")
            self.assertEqual(row["client_type"], 0)
            self.assertEqual((row["auth_value"], row["auth_reason"], row["auth_version"]), (2, 4, 1))
            self.assertIsNone(row["csreq"])
            self.assertEqual(row["last_modified"], 123)
            if extra:
                self.assertEqual(row["last_reminded"], 123)

    def test_unknown_required_columns_and_missing_auth_are_errors(self):
        connection = self.database(", future_required TEXT NOT NULL")
        with self.assertRaisesRegex(b.BlackHoleError, "Unsupported required TCC column"):
            b.consent_sql(self.columns(connection), "test.app", 123)
        with self.assertRaisesRegex(b.BlackHoleError, "missing columns"):
            b.consent_sql([], "test.app", 123)

    def test_unknown_defaulted_columns_are_left_to_sqlite(self):
        connection = self.database(", future_default TEXT NOT NULL DEFAULT 'value'")
        connection.execute(b.consent_sql(self.columns(connection), "test.app", 123))
        self.assertEqual(connection.execute("SELECT future_default FROM access").fetchone()[0], "value")

    def test_read_only_tcc_fails_with_reason(self):
        with patch.object(Path, "is_file", return_value=True), \
                patch.object(b, "run", side_effect=b.BlackHoleError("readonly database")):
            with self.assertRaisesRegex(b.BlackHoleError, "readonly database.*will not be skipped"):
                b.grant_microphone("test.app")

    def test_both_databases_are_written_read_back_and_daemon_restarted(self):
        columns = self.columns(self.database())
        result = json.dumps([{"auth_value": 2}])
        with patch.object(Path, "is_file", return_value=True), \
                patch.object(b, "run", side_effect=[json.dumps(columns), "", result] * 2 + [""]) as run:
            b.grant_microphone("test.app")
        commands = [call.args[0] for call in run.call_args_list]
        self.assertEqual(commands[-1], ["sudo", "killall", "tccd"])
        self.assertEqual(sum("INSERT OR REPLACE" in cmd[-1] for cmd in commands), 2)


if __name__ == "__main__":
    unittest.main()
