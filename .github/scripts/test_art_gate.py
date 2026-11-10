"""Unit tests for art_gate.py using synthetic case data only."""
from __future__ import annotations

import base64
import contextlib
import hashlib
import importlib.util
import io
import json
import math
import os
import shutil
import subprocess
import sys
import tarfile
import tempfile
import textwrap
import unittest
from pathlib import Path
from unittest import mock

import art_gate

CRYPTO_AVAILABLE = importlib.util.find_spec("cryptography") is not None
REPO_ROOT = Path(__file__).resolve().parents[2]
REAL_CLI = os.environ.get("SECS2R_CLI")


def write_file(root: Path, relative: str, data: bytes = b"synthetic") -> Path:
    path = root.joinpath(*relative.split("/"))
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(data)
    return path


def write_case(root: Path, *, art: bool = False, mimo: bool = False) -> dict:
    project = {"schema": 1}
    if mimo:
        project["calibration"] = {"mimo_on": True}
    write_file(
        root,
        "project/case.secs",
        json.dumps(project, separators=(",", ":")).encode(),
    )
    write_file(root, "eval/L.wav", b"raw-wave")
    document = {
        "schema": 1,
        "project": "project/case.secs",
        "inputs": ["L"],
        "eval": {"raw": {"L": {"wav": "eval/L.wav", "t0": 0.0}}},
        "phase_pairs": [["L", "R"]],
    }
    if art:
        write_file(root, "art/L.wav", b"art-wave")
        document["art"] = {"L": {"wav": "art/L.wav", "t0": 0.0}}
    write_file(
        root,
        "case.json",
        json.dumps(document, separators=(",", ":")).encode(),
    )
    return document


def valid_case_toml(
    *,
    name: str = "lab-4.2",
    file_name: str = "lab-4.2.art",
    digest: str = "a" * 64,
    kind: str = "art",
    groups: str = '["arrival", "gd_excess", "pre", "decay"]',
    variants: str = 'base = []\nlf_align = ["--lf-align", "on"]',
) -> str:
    return textwrap.dedent(
        f"""
        [[case]]
        name = {json.dumps(name)}
        file = {json.dumps(file_name)}
        sha256 = {json.dumps(digest)}
        kind = {json.dumps(kind)}
        groups = {groups}

        [case.variants]
        {variants}
        """
    )


def write_manifest(root: Path, body: str) -> Path:
    path = root / "cases.toml"
    path.write_text("schema = 1\n" + body, encoding="utf-8")
    return path


def make_case(
    name: str,
    bundle: bytes,
    *,
    kind: str = "input",
    variants: dict[str, tuple[str, ...]] | None = None,
    groups: tuple[str, ...] = ("arrival", "gd_excess", "pre", "decay"),
) -> art_gate.Case:
    return art_gate.Case(
        name=name,
        file=f"{name}.art",
        sha256=hashlib.sha256(bundle).hexdigest(),
        kind=kind,
        groups=groups,
        variants=variants or {"base": ()},
    )


def plaintext_with_member(name: str, member_type: bytes = tarfile.REGTYPE) -> bytes:
    output = io.BytesIO()
    with tarfile.open(fileobj=output, mode="w:xz") as archive:
        info = tarfile.TarInfo(name)
        info.mtime = 0
        info.mode = 0o644
        info.type = member_type
        if member_type == tarfile.REGTYPE:
            data = b"x"
            info.size = len(data)
            archive.addfile(info, io.BytesIO(data))
        else:
            info.linkname = "target"
            archive.addfile(info)
    return output.getvalue()


class FakeMetrics:
    CROSS_ROOM_GROUPS = ("arrival", "gd_excess", "pre")
    GROUP_METRICS = {
        "arrival": "arrival_80",
        "gd_excess": "gd_excess_60_100",
        "pre": "pre_63",
        "decay": "post20_80_120",
    }

    def __init__(self):
        self.calls: list[tuple] = []
        self.case_dirs: list[Path] = []
        self.export_dirs: list[Path] = []

    def score_case(self, case_dir: Path, export_dir: Path, case: dict) -> dict:
        self.calls.append(("score", "art" in case))
        self.case_dirs.append(case_dir)
        self.export_dirs.append(export_dir)
        predicted = {
            "L": {
                "arrival_80": 123.456789,
                "gd_excess_60_100": float("nan"),
                "pre_63": -7.25,
                "post20_80_120": 91.0,
            }
        }
        reference = (
            {
                "L": {
                    "arrival_80": 123.0,
                    "gd_excess_60_100": 2.0,
                    "pre_63": -7.0,
                    "post20_80_120": 90.0,
                }
            }
            if "art" in case
            else None
        )
        return {
            "predicted": predicted,
            "predicted_phase": {"L/R": 17.0},
            "art": reference,
            "art_phase": {"L/R": 18.0} if reference is not None else None,
        }

    def judge(self, predicted, reference, groups):
        self.calls.append(("judge", tuple(groups)))
        return {self.GROUP_METRICS[group]: "similar" for group in groups}

    def judge_phase(self, predicted, reference):
        self.calls.append(("judge_phase",))
        return {"phase:L/R": "different"}

    def pool(self, references):
        self.calls.append(("pool", len(references)))
        return {"arrival_80": 123.0, "pre_63": -7.0}

    def judge_pooled(self, predicted, pooled, groups):
        self.calls.append(("judge_pooled", tuple(groups), dict(pooled)))
        return {self.GROUP_METRICS[group]: "unknown" for group in groups}

    def grade(self, verdicts):
        self.calls.append(("grade", tuple(verdicts.values())))
        decided = [value for value in verdicts.values() if value != "unknown"]
        if not decided:
            return "none"
        share = sum(value == "similar" for value in decided) / len(decided)
        if share >= 0.75:
            return "close"
        if share >= 0.40:
            return "fair"
        return "far"


def write_fake_cli(root: Path) -> Path:
    script = root / "fake_cli.py"
    script.write_text(
        textwrap.dedent(
            """
            import json
            import os
            import pathlib
            import sys

            args = sys.argv[1:]
            if "project" in args and "set" in args:
                stage = "set"
                index = args.index("set")
                project = pathlib.Path(args[index + 1])
                settings = args[index + 2:]
                if "--mimo" in settings:
                    value = settings[settings.index("--mimo") + 1]
                    document = json.loads(project.read_text(encoding="utf-8"))
                    document.setdefault("calibration", {})["mimo_on"] = value == "on"
                    project.write_text(json.dumps(document), encoding="utf-8")
            elif "calibrate" in args:
                stage = "calibrate"
            elif "export" in args:
                stage = "export"
                out = pathlib.Path(args[args.index("--out") + 1])
                out.mkdir(parents=True, exist_ok=True)
            else:
                stage = "unknown"

            record = pathlib.Path(os.environ["FAKE_RECORD"])
            with record.open("a", encoding="utf-8") as handle:
                handle.write(json.dumps({"stage": stage, "args": args,
                                         "cwd": os.getcwd()}) + "\\n")

            print("CLI_PRIVATE_TOKEN")
            print("CLI_PRIVATE_ERROR", file=sys.stderr)
            if os.environ.get("FAKE_REPLY_STAGE") == stage:
                print(json.dumps({"reply": {"ok": False}}))
            if os.environ.get("FAKE_FAIL_STAGE") == stage:
                if os.environ.get("FAKE_FAIL_ONCE") == "1":
                    marker = pathlib.Path(str(record) + ".failed")
                    if not marker.exists():
                        marker.write_text("failed", encoding="utf-8")
                        raise SystemExit(7)
                else:
                    raise SystemExit(7)
            """
        ).lstrip(),
        encoding="utf-8",
    )
    return script


def read_records(path: Path) -> list[dict]:
    if not path.exists():
        return []
    return [json.loads(line) for line in path.read_text(encoding="utf-8").splitlines()]


class ManifestTests(unittest.TestCase):
    def load_text(self, text: str) -> list[art_gate.Case]:
        with tempfile.TemporaryDirectory() as temporary:
            return art_gate.load_manifest(write_manifest(Path(temporary), text))

    def assert_manifest_error(self, text: str, *words: str) -> None:
        with self.assertRaises(art_gate.ConfigError) as raised:
            self.load_text(text)
        for word in words:
            self.assertIn(word, str(raised.exception))

    def test_valid_manifest_loads_and_preserves_variant_order(self):
        cases = self.load_text(valid_case_toml())
        self.assertEqual(len(cases), 1)
        self.assertEqual(cases[0].name, "lab-4.2")
        self.assertEqual(list(cases[0].variants), ["base", "lf_align"])
        self.assertEqual(cases[0].variants["lf_align"], ("--lf-align", "on"))

    def test_empty_manifest_is_valid(self):
        self.assertEqual(self.load_text(""), [])

    def test_schema_must_be_one(self):
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary, "cases.toml")
            path.write_text("schema = 2\n", encoding="utf-8")
            with self.assertRaisesRegex(art_gate.ConfigError, "schema"):
                art_gate.load_manifest(path)

    def test_case_and_variant_validation_rules(self):
        valid = valid_case_toml
        cases = {
            "bad name": (valid(name="Bad Name"), ("case", "name")),
            "duplicate": (
                valid() + valid(name="lab-4.2", file_name="other.art"),
                ("lab-4.2", "name"),
            ),
            "bad file": (valid(file_name="nested/lab.art"), ("lab-4.2", "file")),
            "bad sha": (valid(digest="A" * 64), ("lab-4.2", "sha256")),
            "unknown kind": (valid(kind="other"), ("lab-4.2", "kind")),
            "empty groups": (valid(groups="[]"), ("lab-4.2", "groups")),
            "unknown group": (
                valid(groups='["arrival", "other"]'),
                ("lab-4.2", "groups"),
            ),
            "no variant": (valid(variants=""), ("lab-4.2", "variants")),
            "bad variant name": (
                valid(variants='"Bad Name" = []'),
                ("lab-4.2", "variants.name"),
            ),
            "flag outside list": (
                valid(variants='base = ["--other", "on"]'),
                ("lab-4.2", "flag"),
            ),
            "odd arguments": (
                valid(variants='base = ["--lf-align"]'),
                ("lab-4.2", "variants.base"),
            ),
            "value with space": (
                valid(variants='base = ["--lf-align", "not on"]'),
                ("lab-4.2", "value"),
            ),
            "value with slash": (
                valid(variants='base = ["--lf-align", "on/off"]'),
                ("lab-4.2", "value"),
            ),
        }
        for label, (text, words) in cases.items():
            with self.subTest(label=label):
                self.assert_manifest_error(text, *words)

    def test_bad_argument_container_names_case_and_field(self):
        self.assert_manifest_error(
            valid_case_toml(variants='base = "not-an-array"'),
            "lab-4.2",
            "variants.base",
        )


class VerifyTests(unittest.TestCase):
    def test_verify_reports_ok_missing_and_mismatch(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            good_data = b"good"
            bad_data = b"bad"
            (root / "good.art").write_bytes(good_data)
            (root / "bad.art").write_bytes(bad_data)
            cases = [
                make_case("good", good_data),
                art_gate.Case(
                    "bad",
                    "bad.art",
                    hashlib.sha256(b"different").hexdigest(),
                    "input",
                    ("arrival",),
                    {"base": ()},
                ),
                art_gate.Case(
                    "missing",
                    "missing.art",
                    hashlib.sha256(b"").hexdigest(),
                    "input",
                    ("arrival",),
                    {"base": ()},
                ),
            ]
            output = io.StringIO()
            self.assertEqual(art_gate.verify_cases(cases, root, output), 1)
            self.assertEqual(
                output.getvalue().splitlines(),
                ["good: ok", "bad: sha256 mismatch", "missing: missing"],
            )

    def test_committed_manifest_loads_and_verifies(self):
        manifest = REPO_ROOT / "reference" / "art" / "cases.toml"
        if not manifest.exists():
            self.skipTest("reference/art/cases.toml is not committed yet")
        cases = art_gate.load_manifest(manifest)
        output = io.StringIO()
        self.assertEqual(art_gate.verify_cases(cases, manifest.parent, output), 0)


@unittest.skipUnless(CRYPTO_AVAILABLE, "cryptography is not installed")
class CryptoTests(unittest.TestCase):
    def test_pack_and_open_round_trip_byte_for_byte(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            source = root / "source"
            source.mkdir()
            write_case(source, art=True)
            write_file(source, "project/extra.bin", bytes(range(64)))
            key = bytes(range(32))
            out = root / "case.art"
            digest, size = art_gate.pack_case(source, "roundtrip", out, key)
            self.assertEqual(digest, hashlib.sha256(out.read_bytes()).hexdigest())
            self.assertEqual(size, out.stat().st_size)
            dest = root / "dest"
            art_gate.open_bundle(out.read_bytes(), "roundtrip", key, dest)
            expected = {
                path.relative_to(source).as_posix(): path.read_bytes()
                for path in source.rglob("*")
                if path.is_file()
            }
            actual = {
                path.relative_to(dest).as_posix(): path.read_bytes()
                for path in dest.rglob("*")
                if path.is_file()
            }
            self.assertEqual(actual, expected)

    def test_wrong_key_name_and_changed_byte_are_same_error(self):
        key = b"k" * 32
        encrypted = art_gate.encrypt_bundle(b"plaintext", "case", key)
        changed = bytearray(encrypted)
        changed[-1] ^= 1
        for label, data, name, candidate_key in (
            ("key", encrypted, "case", b"x" * 32),
            ("name", encrypted, "other", key),
            ("changed", bytes(changed), "case", key),
        ):
            with self.subTest(label=label), tempfile.TemporaryDirectory() as temporary:
                with self.assertRaises(art_gate.BundleError) as raised:
                    art_gate.open_bundle(
                        data, name, candidate_key, Path(temporary, "dest")
                    )
                self.assertEqual(str(raised.exception), "cannot decrypt")

    def test_two_packs_have_different_ciphertext_and_same_plaintext(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            source = root / "source"
            source.mkdir()
            write_case(source)
            key = b"p" * 32
            first = root / "first.art"
            second = root / "second.art"
            art_gate.pack_case(source, "same", first, key)
            art_gate.pack_case(source, "same", second, key)
            self.assertNotEqual(first.read_bytes(), second.read_bytes())
            self.assertEqual(
                art_gate.decrypt_bundle(first.read_bytes(), "same", key),
                art_gate.decrypt_bundle(second.read_bytes(), "same", key),
            )

    def test_non_32_byte_key_is_redacted_configuration_error(self):
        raw = b"distinct-invalid-key-material"
        encoded = base64.b64encode(raw).decode()
        with self.assertRaises(art_gate.ConfigError) as raised:
            art_gate.parse_key(encoded)
        self.assertNotIn(encoded, str(raised.exception))
        self.assertNotIn(raw.decode(), str(raised.exception))

    def test_keygen_refuses_overwrite_and_generated_key_parses(self):
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary, "key.txt")
            art_gate.generate_key(path)
            self.assertEqual(len(art_gate.parse_key(path.read_text())), 32)
            original = path.read_bytes()
            with self.assertRaises(art_gate.ConfigError):
                art_gate.generate_key(path)
            self.assertEqual(path.read_bytes(), original)


class TarSafetyTests(unittest.TestCase):
    def test_absolute_parent_symlink_and_hardlink_are_rejected(self):
        cases = {
            "absolute": plaintext_with_member("/absolute.txt"),
            "parent": plaintext_with_member("../outside.txt"),
            "symlink": plaintext_with_member("link", tarfile.SYMTYPE),
            "hardlink": plaintext_with_member("link", tarfile.LNKTYPE),
        }
        for label, plaintext in cases.items():
            with self.subTest(label=label), tempfile.TemporaryDirectory() as temporary:
                with mock.patch.object(
                    art_gate, "decrypt_bundle", return_value=plaintext
                ):
                    with self.assertRaises(art_gate.BundleError):
                        art_gate.open_bundle(
                            b"unused", "case", b"k" * 32, Path(temporary, "dest")
                        )

    def test_safe_plaintext_extracts_without_cryptography(self):
        plaintext = plaintext_with_member("folder/file.txt")
        with tempfile.TemporaryDirectory() as temporary:
            dest = Path(temporary, "dest")
            with mock.patch.object(art_gate, "decrypt_bundle", return_value=plaintext):
                art_gate.open_bundle(b"unused", "case", b"k" * 32, dest)
            self.assertEqual((dest / "folder" / "file.txt").read_bytes(), b"x")


class PackValidationTests(unittest.TestCase):
    def test_missing_case_json_is_rejected(self):
        with tempfile.TemporaryDirectory() as temporary:
            with self.assertRaisesRegex(art_gate.ConfigError, "case.json"):
                art_gate.build_plaintext(Path(temporary))

    def test_missing_wav_parent_path_and_missing_eval_raw_are_rejected(self):
        mutations = ("missing wav", "parent path", "missing eval.raw")
        for mutation in mutations:
            with self.subTest(mutation=mutation), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                document = write_case(root)
                if mutation == "missing wav":
                    (root / "eval" / "L.wav").unlink()
                elif mutation == "parent path":
                    outside = root.parent / f"outside-{root.name}.wav"
                    outside.write_bytes(b"outside")
                    self.addCleanup(outside.unlink, missing_ok=True)
                    document["eval"]["raw"]["L"]["wav"] = f"../{outside.name}"
                    (root / "case.json").write_text(json.dumps(document), encoding="utf-8")
                else:
                    document["eval"].pop("raw")
                    (root / "case.json").write_text(json.dumps(document), encoding="utf-8")
                with self.assertRaises(art_gate.ConfigError):
                    art_gate.build_plaintext(root)

    def test_non_numeric_t0_and_missing_project_are_rejected(self):
        for mutation in ("t0", "project"):
            with self.subTest(mutation=mutation), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                document = write_case(root)
                if mutation == "t0":
                    document["eval"]["raw"]["L"]["t0"] = "zero"
                else:
                    (root / "project" / "case.secs").unlink()
                (root / "case.json").write_text(json.dumps(document), encoding="utf-8")
                with self.assertRaises(art_gate.ConfigError):
                    art_gate.build_plaintext(root)


class RunHarness(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.root = Path(self.temporary.name)
        self.fake_cli = write_fake_cli(self.root)
        self.record = self.root / "cli-record.jsonl"
        self.environment = mock.patch.dict(
            os.environ,
            {"FAKE_RECORD": str(self.record)},
            clear=False,
        )
        self.environment.start()

    def tearDown(self):
        self.environment.stop()
        self.temporary.cleanup()

    def source(self, name: str, *, art: bool = False, mimo: bool = False) -> Path:
        root = self.root / f"source-{name}"
        root.mkdir()
        write_case(root, art=art, mimo=mimo)
        return root

    def install(self, name: str, *, kind: str = "input", **case_options):
        bundle = f"bundle-{name}".encode()
        (self.root / f"{name}.art").write_bytes(bundle)
        case = make_case(name, bundle, kind=kind, **case_options)
        source = self.source(name, art=kind == "art")
        return case, source

    def run_with_sources(
        self,
        cases: list[art_gate.Case],
        sources: dict[str, Path],
        metrics: FakeMetrics,
    ) -> tuple[art_gate.GateResult, str, str]:
        def fake_open(data, name, key, dest):
            shutil.copytree(sources[name], dest)

        stdout = io.StringIO()
        stderr = io.StringIO()
        with mock.patch.object(art_gate, "open_bundle", side_effect=fake_open):
            with contextlib.redirect_stderr(stderr):
                result = art_gate.run_gate(
                    cases,
                    self.root,
                    [sys.executable, str(self.fake_cli)],
                    b"k" * 32,
                    metrics=metrics,
                    out=stdout,
                )
        return result, stdout.getvalue(), stderr.getvalue()


class RunTests(RunHarness):
    def test_cli_argument_order_variants_and_mimo_detection(self):
        variants = {"base": (), "mimo": ("--mimo", "on")}
        case, source = self.install("artcase", kind="art", variants=variants)
        metrics = FakeMetrics()
        result, output, _ = self.run_with_sources(
            [case], {case.name: source}, metrics
        )
        self.assertEqual(result.exit_code, 0)
        records = read_records(self.record)
        self.assertEqual(
            [record["stage"] for record in records],
            ["calibrate", "export", "set", "calibrate", "export"],
        )
        for record in records:
            self.assertEqual(
                record["args"][:4],
                ["--quiet", "--compute", "cpu", "--deterministic-uuids"],
            )
        self.assertNotIn("--mimo", records[0]["args"])
        project_argument = str(Path("project") / "case.secs")
        self.assertEqual(
            records[2]["args"][4:],
            ["project", "set", project_argument, "--mimo", "on"],
        )
        self.assertEqual(
            records[3]["args"][4:],
            ["calibrate", "--mimo", project_argument],
        )
        self.assertEqual(
            records[4]["args"][4:],
            [
                "export",
                "--kind",
                "convolution",
                "--out",
                "export",
                project_argument,
            ],
        )
        self.assertEqual(sum(call[0] == "judge" for call in metrics.calls), 2)
        self.assertEqual(sum(call[0] == "judge_phase" for call in metrics.calls), 2)
        self.assertIn("metric=phase:L/R verdict=아님", output)
        self.assertEqual(output.count(" grade="), 2)

    def test_input_uses_only_cross_room_groups_and_art_pool(self):
        art_case, art_source = self.install("artroom", kind="art")
        input_case, input_source = self.install("inputroom", kind="input")
        metrics = FakeMetrics()
        result, output, _ = self.run_with_sources(
            [art_case, input_case],
            {"artroom": art_source, "inputroom": input_source},
            metrics,
        )
        self.assertEqual(result.exit_code, 0)
        pooled_calls = [call for call in metrics.calls if call[0] == "judge_pooled"]
        self.assertEqual(len(pooled_calls), 1)
        self.assertEqual(pooled_calls[0][1], ("arrival", "gd_excess", "pre"))
        self.assertNotIn("post20_80_120 verdict=판정 불가", output.split("inputroom")[-1])
        self.assertIn("case=inputroom variant=base metric=arrival_80", output)

    def test_without_art_every_input_verdict_is_unknown(self):
        case, source = self.install("inputonly", kind="input")
        metrics = FakeMetrics()
        result, output, _ = self.run_with_sources(
            [case], {case.name: source}, metrics
        )
        self.assertEqual(result.pooled, {})
        self.assertNotIn("pool", [call[0] for call in metrics.calls])
        verdict_lines = [line for line in output.splitlines() if " metric=" in line]
        self.assertTrue(verdict_lines)
        self.assertTrue(all(line.endswith("verdict=판정 불가") for line in verdict_lines))
        self.assertIn("grade=없음", output)

    def test_failed_stage_returns_one_and_later_case_still_runs(self):
        first, first_source = self.install("first")
        second, second_source = self.install("second")
        metrics = FakeMetrics()
        with mock.patch.dict(
            os.environ,
            {"FAKE_FAIL_STAGE": "calibrate", "FAKE_FAIL_ONCE": "1"},
            clear=False,
        ):
            result, output, _ = self.run_with_sources(
                [first, second],
                {"first": first_source, "second": second_source},
                metrics,
            )
        self.assertEqual(result.exit_code, 1)
        self.assertIn("case=first variant=base failed stage=calibrate", output)
        self.assertIn("case=second variant=base grade=", output)
        self.assertEqual(sum(call[0] == "score" for call in metrics.calls), 1)

    def test_json_reply_ok_false_fails_its_stage(self):
        case, source = self.install("reply")
        with mock.patch.dict(
            os.environ, {"FAKE_REPLY_STAGE": "export"}, clear=False
        ):
            result, output, _ = self.run_with_sources(
                [case], {case.name: source}, FakeMetrics()
            )
        self.assertEqual(result.exit_code, 1)
        self.assertEqual(
            output.strip(), "case=reply variant=base failed stage=export"
        )

    def test_hash_mismatch_fails_before_decrypting(self):
        bundle = b"ciphertext"
        (self.root / "bad.art").write_bytes(bundle)
        case = art_gate.Case(
            "bad",
            "bad.art",
            "0" * 64,
            "input",
            ("arrival",),
            {"base": (), "other": ()},
        )
        output = io.StringIO()
        opener = mock.Mock()
        with mock.patch.object(art_gate, "open_bundle", opener):
            result = art_gate.run_gate(
                [case],
                self.root,
                [sys.executable, str(self.fake_cli)],
                b"k" * 32,
                metrics=FakeMetrics(),
                out=output,
            )
        opener.assert_not_called()
        self.assertEqual(result.exit_code, 1)
        self.assertEqual(output.getvalue().count("failed stage=hash"), 2)

    def test_art_case_without_art_fails_case_stage(self):
        bundle = b"bundle-no-art"
        (self.root / "noart.art").write_bytes(bundle)
        case = make_case("noart", bundle, kind="art")
        source = self.source("noart", art=False)
        result, output, _ = self.run_with_sources(
            [case], {case.name: source}, FakeMetrics()
        )
        self.assertEqual(result.exit_code, 1)
        self.assertEqual(
            output.strip(), "case=noart variant=base failed stage=case"
        )
        self.assertEqual(read_records(self.record), [])

    def test_temporary_work_folder_is_removed(self):
        case, source = self.install("cleanup")
        metrics = FakeMetrics()
        result, _, _ = self.run_with_sources(
            [case], {case.name: source}, metrics
        )
        self.assertEqual(result.exit_code, 0)
        self.assertTrue(metrics.case_dirs)
        self.assertTrue(all(not path.exists() for path in metrics.case_dirs))
        self.assertTrue(all(not path.exists() for path in metrics.export_dirs))

    def test_metric_and_cli_values_and_temporary_paths_are_redacted(self):
        case, source = self.install("redact", kind="art")
        metrics = FakeMetrics()
        result, output, stderr = self.run_with_sources(
            [case], {case.name: source}, metrics
        )
        summary = self.root / "summary.md"
        art_gate.append_summary(summary, result)
        combined = output + stderr + summary.read_text(encoding="utf-8")
        self.assertNotIn("123.456789", combined)
        self.assertNotIn("CLI_PRIVATE_TOKEN", combined)
        self.assertNotIn("CLI_PRIVATE_ERROR", combined)
        for path in metrics.case_dirs + metrics.export_dirs:
            self.assertNotIn(str(path), combined)
        self.assertIn("ART reference (similarity)", combined)
        self.assertIn("ART 실측", combined)

    def test_metrics_exception_message_is_redacted(self):
        case, source = self.install("metric-error", kind="art")
        metrics = FakeMetrics()
        metrics.score_case = mock.Mock(
            side_effect=ValueError("METRIC_PRIVATE_TOKEN 123.456789")
        )
        result, output, stderr = self.run_with_sources(
            [case], {case.name: source}, metrics
        )
        self.assertEqual(result.exit_code, 1)
        self.assertEqual(
            output.strip(), "case=metric-error variant=base failed stage=score"
        )
        self.assertNotIn("METRIC_PRIVATE_TOKEN", output + stderr)
        self.assertNotIn("123.456789", output + stderr)

    def test_values_file_contains_numbers_and_converts_nan_to_null(self):
        case, source = self.install("values", kind="art")
        result, output, _ = self.run_with_sources(
            [case], {case.name: source}, FakeMetrics()
        )
        path = self.root / "values.json"
        art_gate.write_values(path, result)
        text = path.read_text(encoding="utf-8")
        document = json.loads(text)
        self.assertIn("123.456789", text)
        self.assertIsNone(
            document["values"]["base"]["predicted"]["L"]["gd_excess_60_100"]
        )
        self.assertIn("pooled", document)
        self.assertNotIn("123.456789", output)

    def test_summary_has_one_row_per_variant_and_metric_table(self):
        variants = {"base": (), "lf": ("--lf-align", "on")}
        case, source = self.install("summary", kind="art", variants=variants)
        result, _, _ = self.run_with_sources(
            [case], {case.name: source}, FakeMetrics()
        )
        path = self.root / "summary.md"
        path.write_text("existing\n", encoding="utf-8")
        art_gate.append_summary(path, result)
        text = path.read_text(encoding="utf-8")
        self.assertIn("| 사례 | 기준 | 설정 | 등급 | 비슷함 | 아님 | 판정 불가 |", text)
        self.assertIn("| 지표 | base | lf |", text)
        self.assertIn("| arrival_80 | 비슷함 | 비슷함 |", text)
        self.assertEqual(text.count("| summary | ART 실측 |"), 2)


class MainTests(unittest.TestCase):
    def test_no_key_skips_and_require_key_returns_two(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            manifest = write_manifest(root, "")
            environment = dict(os.environ)
            environment.pop("ART_REFERENCE_KEY", None)
            for required, expected, expected_text in (
                (False, 0, "ART reference: key not set, skipped\n"),
                (True, 2, "ART reference: key not set\n"),
            ):
                arguments = [
                    "run",
                    "--manifest",
                    str(manifest),
                    "--cli",
                    "unused",
                ]
                if required:
                    arguments.append("--require-key")
                stdout = io.StringIO()
                stderr = io.StringIO()
                with self.subTest(required=required), mock.patch.dict(
                    os.environ, environment, clear=True
                ), contextlib.redirect_stdout(stdout), contextlib.redirect_stderr(stderr):
                    self.assertEqual(art_gate.main(arguments), expected)
                self.assertEqual(stdout.getvalue() + stderr.getvalue(), expected_text)

    def test_values_out_is_refused_on_github_actions_before_manifest_read(self):
        with tempfile.TemporaryDirectory() as temporary:
            values = Path(temporary, "values.json")
            stdout = io.StringIO()
            stderr = io.StringIO()
            with mock.patch.dict(
                os.environ, {"GITHUB_ACTIONS": "true"}, clear=False
            ), contextlib.redirect_stdout(stdout), contextlib.redirect_stderr(stderr):
                code = art_gate.main(
                    [
                        "run",
                        "--manifest",
                        "does-not-exist.toml",
                        "--cli",
                        "unused",
                        "--values-out",
                        str(values),
                    ]
                )
            self.assertEqual(code, 2)
            self.assertIn("not allowed in CI", stderr.getvalue())
            self.assertFalse(values.exists())

    def test_values_out_is_refused_when_ci_is_set_even_empty(self):
        with tempfile.TemporaryDirectory() as temporary:
            values = Path(temporary, "values.json")
            stderr = io.StringIO()
            with mock.patch.dict(
                os.environ, {"CI": ""}, clear=False
            ), contextlib.redirect_stderr(stderr):
                code = art_gate.main(
                    [
                        "run",
                        "--manifest",
                        "missing.toml",
                        "--cli",
                        "unused",
                        "--values-out",
                        str(values),
                    ]
                )
            self.assertEqual(code, 2)
            self.assertFalse(values.exists())

    def test_unknown_selected_case_is_configuration_error(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            manifest = write_manifest(root, valid_case_toml())
            environment = dict(os.environ)
            environment["ART_REFERENCE_KEY"] = base64.b64encode(b"k" * 32).decode()
            stderr = io.StringIO()
            with mock.patch.dict(
                os.environ, environment, clear=True
            ), contextlib.redirect_stderr(stderr):
                code = art_gate.main(
                    [
                        "run",
                        "--manifest",
                        str(manifest),
                        "--cli",
                        "unused",
                        "--case",
                        "missing",
                    ]
                )
            self.assertEqual(code, 2)
            self.assertIn("case missing", stderr.getvalue())


class ResolveCliTests(unittest.TestCase):
    """ci.yml passes the CLI relative to the checkout; stages run elsewhere."""

    def test_relative_file_becomes_absolute_and_bare_command_is_kept(self):
        with tempfile.TemporaryDirectory() as temporary, contextlib.chdir(temporary):
            root = Path(temporary).resolve()
            write_file(root, "target/release/tool")
            write_file(root, "local-tool")
            self.assertEqual(
                Path(art_gate.resolve_cli(Path("target/release/tool"))),
                root / "target" / "release" / "tool",
            )
            # A bare name that is a file here is that file, not a PATH lookup.
            self.assertEqual(Path(art_gate.resolve_cli(Path("local-tool"))), root / "local-tool")
            self.assertEqual(art_gate.resolve_cli(Path("some-command")), "some-command")
            absolute = root / "local-tool"
            self.assertEqual(Path(art_gate.resolve_cli(absolute)), absolute)

    def test_windows_executable_suffix_is_found(self):
        with tempfile.TemporaryDirectory() as temporary, contextlib.chdir(temporary):
            root = Path(temporary).resolve()
            write_file(root, "bin/tool.exe")
            self.assertEqual(Path(art_gate.resolve_cli(Path("bin/tool"))), root / "bin" / "tool")

    def test_missing_path_is_a_configuration_error(self):
        with tempfile.TemporaryDirectory() as temporary, contextlib.chdir(temporary):
            with self.assertRaises(art_gate.ConfigError):
                art_gate.resolve_cli(Path("target/release/missing"))
            with self.assertRaises(art_gate.ConfigError):
                art_gate.resolve_cli(Path(temporary) / "missing")


class EndToEndTests(unittest.TestCase):
    def _run_setup(self, command: list[str], cwd: Path) -> subprocess.CompletedProcess:
        return subprocess.run(
            command,
            cwd=cwd,
            capture_output=True,
            text=True,
            encoding="utf-8",
            errors="replace",
            timeout=120,
            check=False,
        )

    def _require_dependencies(self):
        if not REAL_CLI:
            self.skipTest("SECS2R_CLI is not set")
        cli = Path(REAL_CLI)
        if not cli.is_file():
            self.skipTest("SECS2R_CLI does not name a file")
        if not CRYPTO_AVAILABLE:
            self.skipTest("cryptography is not installed")
        try:
            import numpy  # noqa: F401
            import art_metrics  # noqa: F401
        except ImportError as error:
            self.skipTest(f"numeric dependency unavailable: {error.name}")
        return cli

    def _make_real_case(self, root: Path, cli: Path) -> tuple[Path, dict]:
        case_root = root / "case"
        project_dir = case_root / "project"
        project_dir.mkdir(parents=True)
        commands = [
            [
                str(cli),
                "--quiet",
                "--deterministic-uuids",
                "project",
                "new",
                "--preset",
                "2.0",
                "case.secs",
            ],
            [
                str(cli),
                "--quiet",
                "--deterministic-uuids",
                "sample",
                "case.secs",
                "--positions",
                "2",
            ],
        ]
        for command in commands:
            completed = self._run_setup(command, project_dir)
            if completed.returncode != 0:
                detail = (completed.stdout + completed.stderr).strip().replace("\n", " ")
                self.skipTest(f"real CLI setup command failed: {detail[:300]}")
        project_path = project_dir / "case.secs"
        try:
            project = json.loads(project_path.read_text(encoding="utf-8"))
            measurement = project["measurements"][0]
            irs = measurement["ir"]
            times = measurement["t0"]
            raw = {
                role: {
                    "wav": f"project/{irs[role]}",
                    "t0": times[role],
                }
                for role in ("L", "R")
            }
        except (OSError, KeyError, TypeError, json.JSONDecodeError) as error:
            self.skipTest(f"real CLI project shape differs: {type(error).__name__}")
        document = {
            "schema": 1,
            "project": "project/case.secs",
            "inputs": ["L", "R"],
            "eval": {"raw": raw},
        }
        (case_root / "case.json").write_text(json.dumps(document), encoding="utf-8")
        art_gate.validate_case_dir(case_root)
        return case_root, document

    def test_real_cli_input_and_art_cases(self):
        cli = self._require_dependencies()
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            case_root, document = self._make_real_case(root, cli)
            key = b"e" * 32

            input_bundle = root / "synthetic-input.art"
            art_gate.pack_case(case_root, "synthetic-input", input_bundle, key)
            input_case = make_case(
                "synthetic-input",
                input_bundle.read_bytes(),
                variants={"base": (), "lf_align": ("--lf-align", "on")},
            )
            output = io.StringIO()
            result = art_gate.run_gate(
                [input_case], root, cli, key, out=output
            )
            self.assertEqual(result.exit_code, 0)
            lines = output.getvalue().splitlines()
            self.assertEqual(sum(" grade=" in line for line in lines), 2)
            verdicts = [line for line in lines if " metric=" in line]
            self.assertTrue(verdicts)
            self.assertTrue(all("verdict=판정 불가" in line for line in verdicts))
            self.assertTrue(
                all(
                    not any(name in line for name in ("post20_80_120", "edt_63_125", "phase:"))
                    for line in verdicts
                )
            )

            document["art"] = dict(document["eval"]["raw"])
            (case_root / "case.json").write_text(
                json.dumps(document), encoding="utf-8"
            )
            art_bundle = root / "synthetic-art.art"
            art_gate.pack_case(case_root, "synthetic-art", art_bundle, key)
            art_case = make_case(
                "synthetic-art", art_bundle.read_bytes(), kind="art"
            )
            art_output = io.StringIO()
            art_result = art_gate.run_gate(
                [art_case], root, cli, key, out=art_output
            )
            self.assertEqual(art_result.exit_code, 0)
            art_lines = art_output.getvalue().splitlines()
            self.assertEqual(sum(" grade=" in line for line in art_lines), 1)
            for metric in (
                "arrival_80",
                "gd_excess_60_100",
                "pre_63",
                "post20_80_120",
            ):
                self.assertTrue(any(f"metric={metric} " in line for line in art_lines))

    def test_main_runs_with_the_cli_given_relative_to_the_current_folder(self):
        """As ci.yml starts it: on 2026-11-10 every stage failed in CI because
        the relative path was looked up from each stage's own work folder."""
        cli = self._require_dependencies()
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            case_root, _ = self._make_real_case(root, cli)
            key = b"r" * 32
            bundle = root / "synthetic-input.art"
            digest, _ = art_gate.pack_case(case_root, "synthetic-input", bundle, key)
            manifest = write_manifest(
                root,
                valid_case_toml(
                    name="synthetic-input",
                    file_name="synthetic-input.art",
                    digest=digest,
                    kind="input",
                    groups='["arrival", "gd_excess", "pre"]',
                ),
            )
            relative = Path(os.path.relpath(cli, cli.parent.parent))
            output, errors = io.StringIO(), io.StringIO()
            environment = {"ART_E2E_KEY": base64.b64encode(key).decode("ascii")}
            with (
                contextlib.chdir(cli.parent.parent),
                mock.patch.dict(os.environ, environment),
                contextlib.redirect_stdout(output),
                contextlib.redirect_stderr(errors),
            ):
                os.environ.pop("CI", None)
                os.environ.pop("GITHUB_ACTIONS", None)
                code = art_gate.main(
                    [
                        "run",
                        "--manifest", str(manifest),
                        "--cli", str(relative),
                        "--key-env", "ART_E2E_KEY",
                        "--summary", str(root / "summary.md"),
                    ]
                )
            self.assertEqual(code, 0, errors.getvalue())
            # run_gate writes to the stream it was defined with, so the
            # summary file is what this test reads.
            summary = (root / "summary.md").read_text(encoding="utf-8")
            self.assertIn("synthetic-input", summary)
            self.assertNotIn("계산 실패", summary)


if __name__ == "__main__":
    unittest.main()
