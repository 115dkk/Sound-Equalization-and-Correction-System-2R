import io
import json
import os
import unittest
from contextlib import redirect_stdout
from unittest.mock import patch

import updater_config as m


class OverlayTest(unittest.TestCase):
    def test_release_windows_overlay_has_resources_and_no_key(self):
        config = m.overlay(True, None)
        self.assertEqual(set(config), {"bundle"})
        resources = config["bundle"]["resources"]
        self.assertEqual(resources["../../target/release/secs2r-host.json.sig"], "secs2r-host.json.sig")
        self.assertEqual(sorted(resources.values()), sorted([
            "secs2r-cli.exe", "secs2r-host.exe", "secs2r-host-shell.exe",
            "secs2r-host.json", "secs2r-host.json.sig",
        ]))

    def test_ci_key_replaces_the_pubkey_only_when_set(self):
        self.assertEqual(m.overlay(False, None), {})
        self.assertEqual(m.overlay(False, "KEY"), {"plugins": {"updater": {"pubkey": "KEY"}}})

    def test_cli_reads_the_named_variable_and_ignores_blank(self):
        for value, expected in (("KEY\n", {"plugins": {"updater": {"pubkey": "KEY"}}}), ("  ", {})):
            with patch.dict(os.environ, {"TEST_PUBKEY": value}):
                out = io.StringIO()
                with redirect_stdout(out):
                    self.assertEqual(m.main(["--pubkey-env", "TEST_PUBKEY"]), 0)
                self.assertEqual(json.loads(out.getvalue()), expected)
        out = io.StringIO()
        with redirect_stdout(out):
            m.main(["--windows-resources"])
        self.assertIn("bundle", json.loads(out.getvalue()))


if __name__ == "__main__":
    unittest.main()
