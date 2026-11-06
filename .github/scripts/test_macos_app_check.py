"""Unit tests for macos_app_check.py (python -m unittest, no dependencies)."""
import json
import struct
import tempfile
import unittest
from subprocess import CompletedProcess
from unittest.mock import patch
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
CERTIFICATE = """Executable=/tmp/SECS 2R.app/Contents/MacOS/secs2r
Identifier=io.github.115dkk.secs2r
Format=app bundle with Mach-O universal (x86_64 arm64)
CodeDirectory v=20500 size=187900 flags=0x10000(runtime) hashes=5864+7 location=embedded
Signature size=1773
Authority=SECS 2R (self-signed)
Signed Time=6 Nov 2026 at 12:00:00
TeamIdentifier=not set
"""
# A throwaway certificate (one day, 2026-11-06); its hash is what the
# designated requirement of a bundle signed with it carries, in the root slot
# (a self-signed certificate is its own root).
FIXTURE_PEM = """-----BEGIN CERTIFICATE-----
MIIDXTCCAkWgAwIBAgIUWe0vuEUONzJbk8mvUtH5m8DeZ3cwDQYJKoZIhvcNAQEL
BQAwPjEdMBsGA1UEAwwUU0VDUyAyUiB0ZXN0IGZpeHR1cmUxHTAbBgNVBAoMFFNF
Q1MgMlIgY29udHJpYnV0b3JzMB4XDTI2MTEwNjAyMzMwN1oXDTI2MTEwNzAyMzMw
N1owPjEdMBsGA1UEAwwUU0VDUyAyUiB0ZXN0IGZpeHR1cmUxHTAbBgNVBAoMFFNF
Q1MgMlIgY29udHJpYnV0b3JzMIIBIjANBgkqhkiG9w0BAQEFAAOCAQ8AMIIBCgKC
AQEAt+ORZF4OWhdIJwVlM5mZ/2f1WChsMGjNdhGz/sGGm1ao8XTQxkZVHoAWJf4U
a+CAB/xfe2WEtdyFT5l5+O2uX0bOMqkq+4adBHd0GQc2Cd0OLRr9f24AIwVMyANZ
HXAHsGt8vmcSSZPDj5xj9uMKj7EjhYp4Dn2FXcXm72B2yYPqXY0PpnWHqfFTKq/O
PYPtgim16MlX0/ZjuESnXs5yt5RN92yEuHVfIKndRCYZbQwuMJm6eGuGccvjMIDf
IEmRtutdujE1obyYoQWa6NV4zM9j3Dg+AzFRgFyWBT3Em0UV5jq1mfb2vTRVd35c
mtQ7N4lWl7+SKw6WzlE77deWnQIDAQABo1MwUTAdBgNVHQ4EFgQUSo4wrkAL2x4S
q4Pkwo7bk/KgG3gwHwYDVR0jBBgwFoAUSo4wrkAL2x4Sq4Pkwo7bk/KgG3gwDwYD
VR0TAQH/BAUwAwEB/zANBgkqhkiG9w0BAQsFAAOCAQEArzPKiZgvpS3Um9WEkPq3
bL04B8+8Ff1qtQEWNsnDbWrsi0JDjjfQICfpktoQF1z4jh0Ga3H+Ntp/lFPNCisU
4j/AlqLfesQD//fk/AWZmK0NufbMjzre3cnWYogr6KWAfE0mD/gV++vUJdYQlf+J
pfTTpzr64xElKi6QOzpyEruqo0mHzjm1oElqcC4X8Slz/bmR2+SDUfGNuADVNLx3
AZzBprGbWzgDjXoJOfnJhFcjFhHxw5t3jxLh5cKogFnRG0CQfroFFLzdqX4oPKY/
rax7Sq+/rs1IGmzOTogTgKQcIeljMOk4JdYO0Vyq9nGd6NZdNOvvyCQCKVCIZrND
ew==
-----END CERTIFICATE-----
"""
FIXTURE_SHA1 = "249bedcecc0f837de3a26280d27b2714563b483b"
IDENTIFIER = "io.github.115dkk.secs2r"
DESIGNATED = ('designated => identifier "io.github.115dkk.secs2r" and certificate root = '
              'H"249BEDCECC0F837DE3A26280D27B2714563B483B"\n')


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

    def test_a_certificate_signature_is_neither_ad_hoc_nor_outside_the_hardened_runtime(self):
        self.assertEqual(m.signature_problems("arm64", CERTIFICATE, certificate=True), [])
        self.assertEqual(m.signature_problems("arm64", CERTIFICATE), [])
        self.assertEqual(m.signature_problems("arm64", AD_HOC, certificate=True),
                         ["the arm64 slice is ad-hoc signed, not signed with the certificate"])
        no_runtime = CERTIFICATE.replace("flags=0x10000(runtime)", "flags=0x0(none)")
        self.assertEqual(m.signature_problems("x86_64", no_runtime, certificate=True),
                         ["the x86_64 slice does not run under the hardened runtime"])
        self.assertEqual(m.signature_problems("x86_64", UNSIGNED, certificate=True),
                         ["the x86_64 slice is not signed"])
        self.assertEqual(m.authorities(CERTIFICATE), ["SECS 2R (self-signed)"])
        self.assertEqual(m.authorities(AD_HOC), [])

    def test_the_certificate_hash_is_the_requirement_hash(self):
        self.assertEqual(m.certificate_sha1(FIXTURE_PEM), FIXTURE_SHA1)
        self.assertEqual(m.certificate_sha1("key material\n" + FIXTURE_PEM + "trailer"), FIXTURE_SHA1)
        with self.assertRaises(ValueError):
            m.certificate_sha1("not a certificate")

    def test_the_designated_requirement_names_the_identifier_and_the_certificate(self):
        self.assertEqual(m.requirement_problems("Executable=/tmp/x\n" + DESIGNATED, IDENTIFIER, FIXTURE_SHA1), [])
        leaf = DESIGNATED.replace("certificate root", "certificate leaf")
        self.assertEqual(m.requirement_problems(leaf, IDENTIFIER, FIXTURE_SHA1), [])
        cdhash = 'designated => cdhash H"0f0f0f" or cdhash H"1e1e1e"\n'
        problems = m.requirement_problems(cdhash, IDENTIFIER, FIXTURE_SHA1)
        self.assertEqual(len(problems), 3)
        self.assertTrue(any("cdhash" in problem for problem in problems))
        other = DESIGNATED.replace(FIXTURE_SHA1.upper(), "0" * 40)
        self.assertEqual(len(m.requirement_problems(other, IDENTIFIER, FIXTURE_SHA1)), 1)
        self.assertEqual(len(m.requirement_problems(DESIGNATED, "io.example.other", FIXTURE_SHA1)), 1)
        self.assertEqual(m.requirement_problems("Executable=/tmp/x\n", IDENTIFIER, FIXTURE_SHA1),
                         ["the bundle has no designated requirement"])

    def test_the_requirement_is_read_only_when_a_certificate_is_required(self):
        def fake_run(cmd, **kwargs):
            if cmd[:2] == ["codesign", "--verify"]:
                return CompletedProcess(cmd, 0, "", "")
            if cmd[0] == "lipo":
                return CompletedProcess(cmd, 0, "arm64\n", "")
            if cmd[:2] == ["codesign", "-dv"]:
                return CompletedProcess(cmd, 0, "", CERTIFICATE)
            if cmd[:3] == ["codesign", "-d", "-r"]:
                return CompletedProcess(cmd, 0, DESIGNATED, "Executable=/tmp/x\n")
            return CompletedProcess(cmd, 0, "", "")

        with patch.object(m, "run", side_effect=fake_run) as run, \
                patch.object(m, "executable", return_value=Path("/tmp/x")), \
                patch.object(m, "bundle_identifier", return_value=IDENTIFIER):
            self.assertEqual(m.check_signature(Path("/tmp/SECS 2R.app"), FIXTURE_SHA1), ([], True))
            self.assertIn(["codesign", "-d", "-r"], [call.args[0][:3] for call in run.call_args_list])
            run.reset_mock()
            self.assertEqual(m.check_signature(Path("/tmp/SECS 2R.app")), ([], True))
            self.assertNotIn(["codesign", "-d", "-r"], [call.args[0][:3] for call in run.call_args_list])
            run.reset_mock()
            problems, _ = m.check_signature(Path("/tmp/SECS 2R.app"), "0" * 40)
            self.assertEqual(len(problems), 1)
            self.assertIn("not anchored", problems[0])


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


def passing_audio():
    configs = []
    for stage, channel, outputs in (("level_L", 1, [1, 2]), ("level_R", 2, [1, 2]), ("sweep", 1, [1, 1])):
        configs.append({"stage": stage, "config": {
            "method": "native", "input_device": "blackhole-in", "output_device": "blackhole-out",
            "input_channel": channel, "output_channels": outputs, "roles": ["L", "R"],
            "sample_rate": 48000, "sweep_length": 65536, "repetitions": 1,
            "sweep_level_dbfs": -20, "share_mode": "shared", "timing": "none",
        }})
    return {
        "ok": True, "device": "BlackHole 2ch", "input_device": "blackhole-in", "output_device": "blackhole-out",
        "configurations": configs, "idle_meter": {"events": 5, "first_ms": 100},
        "played_dbfs": -20, "levels": {"L": -20, "R": -20.01},
        "records": {"records": [{"position": 1, "done": True, "roles": ["L", "R"],
                                  "sources": {"L": "measured", "R": "measured"}}]},
        "impulses": {role: {"sample_rate": 48000, "frames": 96480, "peak_amplitude": 0.9,
                            "peak_ms": 10.02, "arrival_ms": 30, "direct_energy_fraction": 0.99}
                     for role in ("L", "R")},
    }


def float_wav(samples):
    payload = struct.pack(f"<{len(samples)}f", *samples)
    return (b"RIFF" + struct.pack("<I", 36 + len(payload)) + b"WAVEfmt " +
            struct.pack("<IHHIIHH", 16, 3, 1, 48000, 192000, 4, 32) +
            b"data" + struct.pack("<I", len(payload)) + payload)


class AudioReportTest(unittest.TestCase):
    def test_three_configurations_and_both_real_channels_pass(self):
        self.assertEqual(m.audio_problems(passing_audio(), "BlackHole 2ch"), [])

    def test_at_least_five_idle_meter_events_are_required(self):
        for value in (None, 4, True, 5.0):
            audio = passing_audio()
            if value is None:
                audio.pop("idle_meter")
            else:
                audio["idle_meter"]["events"] = value
            self.assertTrue(any("idle meter events" in issue
                                for issue in m.audio_problems(audio, "BlackHole 2ch")))
        audio = passing_audio()
        audio["idle_meter"]["events"] = 6
        self.assertEqual(m.audio_problems(audio, "BlackHole 2ch"), [])

    def test_audio_is_required_only_when_requested(self):
        with tempfile.TemporaryDirectory() as folder:
            work = Path(folder)
            for name, suffix in m.EXPORTS.items():
                path = work / "export" / name / f"export{suffix}"
                path.parent.mkdir(parents=True)
                path.write_bytes(b"export")
            report = {"ok": True}
            self.assertEqual(m.report_problems("run", report, work), [])
            self.assertIn("native audio step did not pass",
                          m.report_problems("run", report, work, "BlackHole 2ch")[0])
            report["facts"] = {"audio": passing_audio()}
            self.assertEqual(m.report_problems("run", report, work, "BlackHole 2ch"), [])

    def test_retried_attempts_become_notes_and_do_not_fail(self):
        audio = passing_audio()
        audio["retries"] = [{"stage": "level_R", "attempt": 1, "error": "level_play: gap"}]
        self.assertEqual(m.audio_problems(audio, "BlackHole 2ch"), [])
        self.assertEqual(m.audio_retries(audio), ["retried level_R after attempt 1: level_play: gap"])
        self.assertEqual(m.audio_retries(None), [])
        self.assertEqual(m.audio_retries({"retries": "x"}), [])

    def test_missing_channel_silence_nan_or_bad_level_fails(self):
        for role in ("L", "R"):
            for value in (None, -120, -26.01, -13.99, float("nan"), float("inf"), True):
                audio = passing_audio()
                audio["levels"][role] = value
                self.assertTrue(any("levels_get" in issue for issue in m.audio_problems(audio, "BlackHole 2ch")))
        audio = passing_audio()
        audio["levels"] = {"L": -26, "R": -14}
        self.assertEqual(m.audio_problems(audio, "BlackHole 2ch"), [])

    def test_wrong_channel_mapping_mock_method_and_unread_config_fail(self):
        for index, field, value in ((0, "method", "mock"), (1, "input_channel", 1),
                                    (2, "output_channels", [1, 2]), (0, "input_device", "default"),
                                    (0, "roles", ["R", "L"])):
            audio = passing_audio()
            audio["configurations"][index]["config"][field] = value
            self.assertTrue(any("measure_config_get" in issue for issue in m.audio_problems(audio, "BlackHole 2ch")))
        audio = passing_audio()
        audio["configurations"].pop()
        self.assertTrue(m.audio_problems(audio, "BlackHole 2ch"))
        self.assertTrue(m.audio_problems(passing_audio(), "Other Device"))
        self.assertTrue(m.audio_problems({"ok": False}, "BlackHole 2ch"))

    def test_both_roles_must_be_recorded_by_the_native_measurement(self):
        for change in ({"done": False}, {"roles": ["L"]}, {"sources": {"L": "sample", "R": "measured"}}):
            audio = passing_audio()
            audio["records"]["records"][0].update(change)
            self.assertTrue(any("measurement_records" in issue for issue in m.audio_problems(audio, "BlackHole 2ch")))

    def test_silent_delayed_diffuse_truncated_or_missing_impulses_fail(self):
        for role in ("L", "R"):
            for field, value in (("peak_amplitude", 0), ("peak_amplitude", 0.01), ("peak_ms", 500),
                                 ("arrival_ms", 600), ("arrival_ms", 40), ("frames", 48000),
                                 ("direct_energy_fraction", 0.1), ("peak_amplitude", float("nan"))):
                audio = passing_audio()
                audio["impulses"][role][field] = value
                self.assertTrue(m.audio_problems(audio, "BlackHole 2ch"), (role, field, value))
            audio = passing_audio()
            del audio["impulses"][role]
            self.assertTrue(m.audio_problems(audio, "BlackHole 2ch"))

    def test_the_real_saved_wav_is_used_for_impulse_facts(self):
        samples = [0.0] * 96480
        samples[480] = 1.0
        wav = float_wav(samples)
        facts = m.impulse_facts(wav, 0.020)
        self.assertEqual(facts["peak_ms"], 10)
        self.assertEqual(facts["arrival_ms"], 30)
        self.assertEqual(facts["peak_amplitude"], 1)
        self.assertEqual(facts["direct_energy_fraction"], 1)
        with tempfile.TemporaryDirectory() as folder:
            work = Path(folder)
            (work / "audio.irs").mkdir()
            for role in ("L", "R"):
                (work / "audio.irs" / f"{role}_P1.wav").write_bytes(wav)
            project = {"measurements": [{"pos": 1, "done": True,
                                        "ir": {role: f"audio.irs/{role}_P1.wav" for role in ("L", "R")},
                                        "t0": {"L": 0.020, "R": 0.020}}]}
            (work / "audio.secs").write_text(json.dumps(project))
            self.assertEqual(m.read_audio_impulses(work), {"L": facts, "R": facts})
            project["measurements"][0]["ir"]["R"] = "sample.irs/R_P1.wav"
            (work / "audio.secs").write_text(json.dumps(project))
            with self.assertRaisesRegex(ValueError, "does not reference its native IR"):
                m.read_audio_impulses(work)

    def test_bad_wav_and_nonfinite_time_are_rejected(self):
        valid = float_wav([0.0, 1.0])
        for data in (b"", valid[:-1], float_wav([float("nan")]), valid.replace(b"WAVE", b"FAIL")):
            with self.assertRaises(ValueError):
                m.impulse_facts(data, 0)
        with self.assertRaises(ValueError):
            m.impulse_facts(valid, float("nan"))

    def test_only_native_launch_receives_audio_device(self):
        with tempfile.TemporaryDirectory() as folder:
            work = Path(folder)
            with patch.object(m, "run") as run:
                m.self_test(Path("app.app"), "arm64", work, 1, "BlackHole 2ch")
                native = run.call_args.args[0]
                m.self_test(Path("app.app"), "x86_64", work, 1)
                rosetta = run.call_args.args[0]
            self.assertIn("SECS2R_SELF_TEST_AUDIO=BlackHole 2ch", native)
            self.assertIn("SECS2R_SELF_TEST_AUDIO=", rosetta)
            self.assertIn("--arch", rosetta)


if __name__ == "__main__":
    unittest.main()
