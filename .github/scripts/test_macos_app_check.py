"""Unit tests for macos_app_check.py (python -m unittest, no dependencies)."""
import tempfile
import unittest
from pathlib import Path

import macos_app_check as m

LINKER_SIGNED = """Executable=/tmp/SECS 2R.app/Contents/MacOS/secs2r
Identifier=secs2r-c9ffdef3e8a4905a
Format=app bundle with Mach-O universal (x86_64 arm64)
CodeDirectory v=20400 size=187757 flags=0x20002(adhoc,linker-signed) hashes=5864+0 location=embedded
Signature=adhoc
"""
AD_HOC = """Executable=/tmp/SECS 2R.app/Contents/MacOS/secs2r
Identifier=io.github.115dkk.secs2r
CodeDirectory v=20500 size=187900 flags=0x10002(adhoc,runtime) hashes=5864+7 location=embedded
Signature=adhoc
"""
UNSIGNED = "/tmp/SECS 2R.app: code object is not signed at all\n"


class SignatureTest(unittest.TestCase):
    def test_reads_code_directory_flags(self):
        self.assertEqual(m.code_directory_flags(AD_HOC), {"adhoc", "runtime"})
        self.assertEqual(m.code_directory_flags(LINKER_SIGNED), {"adhoc", "linker-signed"})
        self.assertEqual(m.code_directory_flags("CodeDirectory v=20500 size=1 flags=0x0(none) hashes=1+7\n"), set())
        self.assertIsNone(m.code_directory_flags(UNSIGNED))

    def test_only_a_real_signature_passes(self):
        self.assertEqual(m.signature_problems("arm64", AD_HOC), [])
        self.assertEqual(m.signature_problems("arm64", LINKER_SIGNED),
                         ["the arm64 slice carries only the linker's signature (bundle not signed)"])
        self.assertEqual(m.signature_problems("x86_64", UNSIGNED), ["the x86_64 slice is not signed"])


class BundleTest(unittest.TestCase):
    def test_microphone_usage_must_be_described(self):
        self.assertEqual(m.info_plist_problems({"NSMicrophoneUsageDescription": "Measures speakers."}), [])
        for info in ({}, {"NSMicrophoneUsageDescription": " "}, {"NSMicrophoneUsageDescription": 1}):
            self.assertEqual(m.info_plist_problems(info), ["Info.plist has no NSMicrophoneUsageDescription"])

    def test_both_localizations_describe_the_microphone(self):
        def table(text: str) -> bytes:
            return m.plistlib.dumps({"NSMicrophoneUsageDescription": text})
        self.assertEqual(m.localized_problems({"en": table("Measures."), "ko": table("측정합니다.")}), [])
        self.assertEqual(m.localized_problems({"en": table("Measures.")}),
                         ["ko.lproj/InfoPlist.strings is missing"])
        self.assertEqual(m.localized_problems({"en": b"<plist", "ko": table(" ")}),
                         ["en.lproj/InfoPlist.strings is not a property list",
                          "ko.lproj/InfoPlist.strings has no NSMicrophoneUsageDescription"])

    def test_hardened_runtime_needs_the_audio_input_entitlement(self):
        self.assertEqual(m.entitlement_problems({}, hardened=False), [])
        self.assertEqual(m.entitlement_problems({m.AUDIO_INPUT: True}, hardened=True), [])
        self.assertEqual(len(m.entitlement_problems({}, hardened=True)), 1)
        self.assertEqual(len(m.entitlement_problems({m.AUDIO_INPUT: False}, hardened=True)), 1)


class ReportTest(unittest.TestCase):
    def setUp(self):
        self.work = Path(tempfile.mkdtemp())

    def export(self, folder: str, name: str, data: bytes = b"x") -> None:
        path = self.work / "export" / folder / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(data)

    def test_a_missing_or_failed_report_is_a_problem(self):
        self.assertEqual(m.report_problems("run", None, self.work), ["run: the app wrote no self-test report"])
        self.assertEqual(m.report_problems("run", {"ok": False, "error": "calibrate job failed"}, self.work),
                         ["run: the self-test failed: calibrate job failed"])

    def test_a_passing_report_needs_both_exports(self):
        self.export("camilla-dsp", "configs/secs_hq.yml")
        self.assertEqual(m.report_problems("run", {"ok": True}, self.work),
                         ["run: the equalizer-apo export has no config.txt file"])
        self.export("equalizer-apo", "config.txt")
        self.assertEqual(m.report_problems("run", {"ok": True}, self.work), [])
        self.export("equalizer-apo", "SECS/filter.txt", b"")
        problems = m.report_problems("run", {"ok": True}, self.work)
        self.assertEqual(len(problems), 1)
        self.assertIn("filter.txt is empty", problems[0])


if __name__ == "__main__":
    unittest.main()
