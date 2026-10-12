"""Unit tests for collect_bundles.py (python -m unittest, no dependencies)."""
import struct
import unittest

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
        self.assertEqual(c.asset_name("0.1.0", "portable"), "SECS2R-0.1.0-windows-x64-portable.zip")
        self.assertEqual(c.asset_name("0.1.0", "dmg"), "SECS2R-0.1.0-macos-universal.dmg")
        self.assertEqual(c.asset_name("0.1.0", "appimage"), "SECS2R-0.1.0-linux-x86_64.AppImage")
        self.assertEqual(c.asset_name("0.1.0", "deb"), "SECS2R-0.1.0-linux-amd64.deb")


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


if __name__ == "__main__":
    unittest.main()
