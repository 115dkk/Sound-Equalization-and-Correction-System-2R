#!/usr/bin/env python3
"""Pack, verify, and run the private ART reference similarity gate.

The encrypted measurements and numeric metric values are deliberately kept out
of normal output.  Only case names, metric names, verdicts, grades, and failure
stages are reported, as required by ADR 0026.
"""
from __future__ import annotations

import argparse
import base64
import binascii
import hashlib
import io
import json
import math
import os
import re
import shutil
import stat
import subprocess
import sys
import tarfile
import tempfile
import tomllib
from dataclasses import dataclass, field
from pathlib import Path, PurePosixPath, PureWindowsPath
from typing import Any, Sequence, TextIO

MAGIC = b"SECSART1"
NONCE_SIZE = 12
KEY_SIZE = 32
MAX_MEMBERS = 20_000
MAX_EXTRACTED_BYTES = 2 * 1024 * 1024 * 1024
CASE_NAME_RE = re.compile(r"^[a-z0-9][a-z0-9._-]{0,63}$")
VARIANT_NAME_RE = re.compile(r"^[a-z0-9][a-z0-9_+.-]{0,63}$")
SHA256_RE = re.compile(r"^[0-9a-f]{64}$")
VALUE_RE = re.compile(r"^[A-Za-z0-9.,-]{1,64}$")
KNOWN_GROUPS = ("arrival", "gd_excess", "pre", "decay")
ALLOWED_FLAGS = frozenset(
    {
        "--mimo",
        "--mimo-supports",
        "--mimo-early",
        "--mimo-seat-guard",
        "--lf-align",
        "--mode-decay",
        "--low-protect",
        "--hf-phase",
        "--dip-limit",
        "--input-hp",
        "--rc-bands",
    }
)
VERDICT_WORDS = {
    "similar": "비슷함",
    "different": "아님",
    "unknown": "판정 불가",
}
GRADE_WORDS = {"close": "가까움", "fair": "보통", "far": "멂", "none": "없음"}
KIND_WORDS = {"art": "ART 실측", "input": "다른 방의 ART 기준"}


class ConfigError(ValueError):
    """A usage or configuration error that should make the CLI return 2."""


class BundleError(ValueError):
    """An encrypted bundle could not be authenticated or safely extracted."""


@dataclass(frozen=True)
class Case:
    """One validated manifest entry, preserving variant order."""

    name: str
    file: str
    sha256: str
    kind: str
    groups: tuple[str, ...]
    variants: dict[str, tuple[str, ...]]


@dataclass
class VariantResult:
    """Internal result for one case variant; numeric scores are never printed."""

    case: Case
    variant: str
    score: dict[str, Any] | None = None
    verdicts: dict[str, str] = field(default_factory=dict)
    grade: str | None = None
    failed_stage: str | None = None


@dataclass
class GateResult:
    """Results returned by :func:`run_gate` for summaries and local values."""

    variants: list[VariantResult]
    pooled: dict[str, Any]

    @property
    def failed(self) -> bool:
        return any(result.failed_stage is not None for result in self.variants)

    @property
    def exit_code(self) -> int:
        return 1 if self.failed else 0


def _case_error(label: str, field_name: str, detail: str) -> ConfigError:
    return ConfigError(f"case {label}: field {field_name} {detail}")


def load_manifest(path: Path) -> list[Case]:
    """Load and validate a schema-1 ART case manifest without numeric imports."""
    try:
        with path.open("rb") as handle:
            document = tomllib.load(handle)
    except (OSError, tomllib.TOMLDecodeError) as error:
        raise ConfigError("manifest: cannot read TOML") from error

    schema = document.get("schema")
    if isinstance(schema, bool) or schema != 1:
        raise ConfigError("manifest: field schema must be 1")
    entries = document.get("case", [])
    if not isinstance(entries, list):
        raise ConfigError("manifest: field case must be an array")

    cases: list[Case] = []
    names: set[str] = set()
    for index, entry in enumerate(entries):
        if not isinstance(entry, dict):
            raise _case_error(f"#{index + 1}", "case", "must be a table")
        raw_name = entry.get("name")
        label = raw_name if isinstance(raw_name, str) and raw_name else f"#{index + 1}"
        if not isinstance(raw_name, str) or CASE_NAME_RE.fullmatch(raw_name) is None:
            raise _case_error(label, "name", "is invalid")
        if raw_name in names:
            raise _case_error(raw_name, "name", "is duplicated")
        names.add(raw_name)

        file_name = entry.get("file")
        if (
            not isinstance(file_name, str)
            or not file_name.endswith(".art")
            or file_name in {"", ".", ".."}
            or "/" in file_name
            or "\\" in file_name
        ):
            raise _case_error(raw_name, "file", "must be a plain .art file name")

        digest = entry.get("sha256")
        if not isinstance(digest, str) or SHA256_RE.fullmatch(digest) is None:
            raise _case_error(raw_name, "sha256", "must be 64 lowercase hex digits")

        kind = entry.get("kind")
        if kind not in {"art", "input"}:
            raise _case_error(raw_name, "kind", "must be art or input")

        groups = entry.get("groups")
        if not isinstance(groups, list) or not groups:
            raise _case_error(raw_name, "groups", "must be a non-empty array")
        for group in groups:
            if not isinstance(group, str) or group not in KNOWN_GROUPS:
                raise _case_error(raw_name, "groups", "contains an unknown group")

        raw_variants = entry.get("variants")
        if not isinstance(raw_variants, dict) or not raw_variants:
            raise _case_error(raw_name, "variants", "must be a non-empty table")
        variants: dict[str, tuple[str, ...]] = {}
        for variant_name, raw_arguments in raw_variants.items():
            if (
                not isinstance(variant_name, str)
                or VARIANT_NAME_RE.fullmatch(variant_name) is None
            ):
                raise _case_error(raw_name, "variants.name", "is invalid")
            if not isinstance(raw_arguments, list) or not all(
                isinstance(argument, str) for argument in raw_arguments
            ):
                raise _case_error(
                    raw_name, f"variants.{variant_name}", "must be an argument array"
                )
            if len(raw_arguments) % 2 != 0:
                raise _case_error(
                    raw_name,
                    f"variants.{variant_name}",
                    "must contain flag/value pairs",
                )
            for argument_index in range(0, len(raw_arguments), 2):
                flag = raw_arguments[argument_index]
                value = raw_arguments[argument_index + 1]
                if flag not in ALLOWED_FLAGS:
                    raise _case_error(
                        raw_name, f"variants.{variant_name}.flag", "is not allowed"
                    )
                if VALUE_RE.fullmatch(value) is None:
                    raise _case_error(
                        raw_name, f"variants.{variant_name}.value", "is invalid"
                    )
            variants[variant_name] = tuple(raw_arguments)

        cases.append(
            Case(
                name=raw_name,
                file=file_name,
                sha256=digest,
                kind=kind,
                groups=tuple(groups),
                variants=variants,
            )
        )
    return cases


def file_sha256(path: Path) -> str:
    """Return a file's SHA-256 while keeping large ciphertexts out of memory."""
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        while chunk := handle.read(1 << 20):
            digest.update(chunk)
    return digest.hexdigest()


def parse_key(value: str) -> bytes:
    """Decode a standard-base64 AES-256 key without echoing invalid input."""
    try:
        key = base64.b64decode(value.strip(), validate=True)
    except (binascii.Error, ValueError) as error:
        raise ConfigError("invalid ART reference key") from error
    if len(key) != KEY_SIZE:
        raise ConfigError("invalid ART reference key")
    return key


def encrypt_bundle(plaintext: bytes, name: str, key: bytes) -> bytes:
    """Encrypt one tar.xz plaintext with its case name as associated data."""
    if len(key) != KEY_SIZE:
        raise ConfigError("invalid ART reference key")
    try:
        from cryptography.hazmat.primitives.ciphers.aead import AESGCM
    except ImportError as error:
        raise ConfigError("cryptography is required") from error

    nonce = os.urandom(NONCE_SIZE)
    associated_data = MAGIC + name.encode("utf-8")
    return MAGIC + nonce + AESGCM(key).encrypt(nonce, plaintext, associated_data)


def decrypt_bundle(data: bytes, name: str, key: bytes) -> bytes:
    """Authenticate and decrypt a bundle, coalescing all auth failures."""
    if len(key) != KEY_SIZE:
        raise ConfigError("invalid ART reference key")
    try:
        from cryptography.hazmat.primitives.ciphers.aead import AESGCM
    except ImportError as error:
        raise ConfigError("cryptography is required") from error

    minimum_size = len(MAGIC) + NONCE_SIZE + 16
    if len(data) < minimum_size or not data.startswith(MAGIC):
        raise BundleError("cannot decrypt")
    nonce_start = len(MAGIC)
    nonce = data[nonce_start : nonce_start + NONCE_SIZE]
    ciphertext = data[nonce_start + NONCE_SIZE :]
    associated_data = MAGIC + name.encode("utf-8")
    try:
        return AESGCM(key).decrypt(nonce, ciphertext, associated_data)
    except Exception as error:
        # Authentication errors intentionally share one message so callers do
        # not learn whether the key, name, or ciphertext was wrong.
        raise BundleError("cannot decrypt") from error


def _relative_parts(value: str, label: str) -> tuple[str, ...]:
    """Validate a portable relative path and return its components."""
    if not isinstance(value, str) or not value:
        raise ConfigError(f"case.json: field {label} must be a relative path")
    normalized = value.replace("\\", "/")
    posix = PurePosixPath(normalized)
    windows = PureWindowsPath(value)
    if posix.is_absolute() or windows.is_absolute() or windows.drive:
        raise ConfigError(f"case.json: field {label} must be a relative path")
    if ".." in posix.parts:
        raise ConfigError(f"case.json: field {label} must not contain ..")
    parts = tuple(part for part in posix.parts if part not in {"", "."})
    if not parts:
        raise ConfigError(f"case.json: field {label} must be a relative path")
    return parts


def _referenced_file(root: Path, value: str, label: str) -> Path:
    parts = _relative_parts(value, label)
    candidate = root.joinpath(*parts)
    try:
        resolved = candidate.resolve(strict=True)
    except OSError as error:
        raise ConfigError(f"case.json: field {label} names a missing file") from error
    try:
        resolved.relative_to(root.resolve())
    except ValueError as error:
        raise ConfigError(f"case.json: field {label} leaves the case folder") from error
    if not resolved.is_file():
        raise ConfigError(f"case.json: field {label} must name a file")
    return candidate


def _numeric(value: Any) -> bool:
    return (
        isinstance(value, (int, float))
        and not isinstance(value, bool)
        and math.isfinite(float(value))
    )


def _validate_measurements(root: Path, value: Any, label: str) -> None:
    if not isinstance(value, dict) or not value:
        raise ConfigError(f"case.json: field {label} must be a non-empty object")
    for measurement_name, entry in value.items():
        item_label = f"{label}.{measurement_name}"
        if not isinstance(measurement_name, str) or not isinstance(entry, dict):
            raise ConfigError(f"case.json: field {label} has an invalid entry")
        wav = entry.get("wav")
        if not isinstance(wav, str):
            raise ConfigError(f"case.json: field {item_label}.wav must be a path")
        _referenced_file(root, wav, f"{item_label}.wav")
        if not _numeric(entry.get("t0")):
            raise ConfigError(f"case.json: field {item_label}.t0 must be numeric")


def validate_case_dir(root: Path) -> dict[str, Any]:
    """Validate case.json and every path that pack or run relies on."""
    case_path = root / "case.json"
    if not case_path.is_file():
        raise ConfigError("case.json: missing")
    try:
        document = json.loads(case_path.read_text(encoding="utf-8"))
    except (OSError, UnicodeError, json.JSONDecodeError) as error:
        raise ConfigError("case.json: invalid JSON") from error
    if not isinstance(document, dict):
        raise ConfigError("case.json: root must be an object")
    schema = document.get("schema")
    if isinstance(schema, bool) or schema != 1:
        raise ConfigError("case.json: field schema must be 1")

    project = document.get("project")
    if not isinstance(project, str) or not project.endswith(".secs"):
        raise ConfigError("case.json: field project must name a .secs file")
    _referenced_file(root, project, "project")

    inputs = document.get("inputs")
    if not isinstance(inputs, list) or not inputs or not all(
        isinstance(item, str) for item in inputs
    ):
        raise ConfigError("case.json: field inputs must be a non-empty string array")

    evaluation = document.get("eval")
    if not isinstance(evaluation, dict) or "raw" not in evaluation:
        raise ConfigError("case.json: field eval.raw is missing")
    _validate_measurements(root, evaluation["raw"], "eval.raw")
    if "art" in document:
        _validate_measurements(root, document["art"], "art")
    return document


def _regular_files(root: Path) -> list[Path]:
    """List regular files in deterministic path order and reject links."""
    files: list[Path] = []
    for current, directories, names in os.walk(root, followlinks=False):
        directories.sort()
        names.sort()
        current_path = Path(current)
        for directory in directories:
            if (current_path / directory).is_symlink():
                raise ConfigError("case folder contains a symbolic link")
        for name in names:
            path = current_path / name
            try:
                mode = path.lstat().st_mode
            except OSError as error:
                raise ConfigError("case folder contains an unreadable file") from error
            if not stat.S_ISREG(mode):
                raise ConfigError("case folder contains a non-regular file")
            files.append(path)
    files.sort(key=lambda path: path.relative_to(root).as_posix())
    return files


def build_plaintext(case_dir: Path) -> bytes:
    """Build the deterministic in-memory tar.xz plaintext for a case folder."""
    validate_case_dir(case_dir)
    files = _regular_files(case_dir)
    output = io.BytesIO()
    with tarfile.open(fileobj=output, mode="w:xz") as archive:
        for path in files:
            relative = path.relative_to(case_dir).as_posix()
            data = path.read_bytes()
            info = tarfile.TarInfo(relative)
            info.size = len(data)
            info.mtime = 0
            info.uid = 0
            info.gid = 0
            info.uname = ""
            info.gname = ""
            info.mode = 0o644
            info.type = tarfile.REGTYPE
            archive.addfile(info, io.BytesIO(data))
    return output.getvalue()


def _safe_member_name(name: str) -> bool:
    if not name or "\\" in name:
        return False
    posix = PurePosixPath(name)
    windows = PureWindowsPath(name)
    return (
        not posix.is_absolute()
        and not windows.is_absolute()
        and not windows.drive
        and ".." not in posix.parts
    )


def open_bundle(data: bytes, name: str, key: bytes, dest: Path) -> None:
    """Decrypt and safely extract one authenticated tar.xz bundle."""
    plaintext = decrypt_bundle(data, name, key)
    try:
        with tarfile.open(fileobj=io.BytesIO(plaintext), mode="r:xz") as archive:
            members = archive.getmembers()
            if len(members) > MAX_MEMBERS:
                raise BundleError("bundle has too many members")
            total = 0
            for member in members:
                if not (member.isfile() or member.isdir()):
                    raise BundleError("bundle contains a non-regular member")
                if not _safe_member_name(member.name):
                    raise BundleError("bundle contains an unsafe path")
                if member.isfile():
                    if member.size < 0:
                        raise BundleError("bundle contains an invalid size")
                    total += member.size
                    if total > MAX_EXTRACTED_BYTES:
                        raise BundleError("bundle is too large")
            dest.mkdir(parents=True, exist_ok=True)
            archive.extractall(path=dest, members=members, filter="data")
    except BundleError:
        raise
    except (OSError, tarfile.TarError, ValueError) as error:
        raise BundleError("invalid bundle") from error


def pack_case(case_dir: Path, name: str, out_path: Path, key: bytes) -> tuple[str, int]:
    """Validate, deterministically archive, and randomly encrypt one case."""
    if CASE_NAME_RE.fullmatch(name) is None:
        raise ConfigError("pack: invalid case name")
    plaintext = build_plaintext(case_dir)
    ciphertext = encrypt_bundle(plaintext, name, key)
    out_path.write_bytes(ciphertext)
    return hashlib.sha256(ciphertext).hexdigest(), len(ciphertext)


def verify_cases(cases: Sequence[Case], manifest_dir: Path, out: TextIO = sys.stdout) -> int:
    """Check every ciphertext's existence and manifest SHA-256."""
    failed = False
    for case in cases:
        path = manifest_dir / case.file
        if not path.is_file():
            print(f"{case.name}: missing", file=out)
            failed = True
            continue
        try:
            digest = file_sha256(path)
        except OSError:
            print(f"{case.name}: unreadable", file=out)
            failed = True
            continue
        if digest != case.sha256:
            print(f"{case.name}: sha256 mismatch", file=out)
            failed = True
        else:
            print(f"{case.name}: ok", file=out)
    return 1 if failed else 0


def resolve_cli(cli: Path) -> str:
    """Return the CLI as every stage can start it.

    A stage runs with its own work folder as the current folder, so a path
    given relative to the caller's folder (``target/release/secs2r-cli`` in
    ci.yml) is made absolute here.  A bare command name that is not a file in
    the caller's folder is left for the PATH lookup.  A path that names no
    file is a configuration error: without this check every stage of every
    case would fail the same way.
    """
    if not cli.is_absolute():
        candidate = Path.cwd() / cli
        if len(cli.parts) == 1 and not candidate.is_file():
            return str(cli)
        cli = candidate
    if not (cli.is_file() or cli.with_name(cli.name + ".exe").is_file()):
        raise ConfigError("--cli: no such file")
    return str(cli.resolve())


def _command_prefix(cli: str | Path | Sequence[str]) -> list[str]:
    if isinstance(cli, (str, Path)):
        return [str(cli)]
    return [str(part) for part in cli]


def _cli_succeeded(command: list[str], cwd: Path) -> bool:
    """Run a CLI stage with fully captured output and inspect JSON failures."""
    try:
        completed = subprocess.run(
            command,
            cwd=cwd,
            capture_output=True,
            text=True,
            encoding="utf-8",
            errors="replace",
            timeout=900,
            check=False,
        )
    except (OSError, subprocess.TimeoutExpired):
        return False
    if completed.returncode != 0:
        return False
    for line in completed.stdout.splitlines():
        try:
            value = json.loads(line)
        except json.JSONDecodeError:
            continue
        if (
            isinstance(value, dict)
            and isinstance(value.get("reply"), dict)
            and value["reply"].get("ok") is False
        ):
            return False
    return True


def _case_failures(case: Case, stage: str) -> list[VariantResult]:
    return [
        VariantResult(case=case, variant=variant, failed_stage=stage)
        for variant in case.variants
    ]


def _run_variant(
    case: Case,
    variant_name: str,
    arguments: tuple[str, ...],
    case_dir: Path,
    case_json: dict[str, Any],
    variant_root: Path,
    cli: str | Path | Sequence[str],
    metrics: Any,
) -> VariantResult:
    result = VariantResult(case=case, variant=variant_name)
    project_parts = _relative_parts(case_json["project"], "project")
    source_project = case_dir.joinpath(*project_parts)
    source_folder = source_project.parent
    # The packet requires a fresh relative copy but does not prescribe its
    # name.  A stable "project" folder keeps every CLI argument relative.
    copied_folder = variant_root / "project"
    shutil.copytree(source_folder, copied_folder)
    copied_project = copied_folder / source_project.name
    relative_project = Path("project") / source_project.name
    export_dir = variant_root / "export"

    prefix = _command_prefix(cli) + [
        "--quiet",
        "--compute",
        "cpu",
        "--deterministic-uuids",
    ]
    if arguments:
        command = prefix + ["project", "set", str(relative_project), *arguments]
        if not _cli_succeeded(command, variant_root):
            result.failed_stage = "set"
            return result

    try:
        project_json = json.loads(copied_project.read_text(encoding="utf-8"))
        calibration = project_json.get("calibration", {})
        mimo_on = isinstance(calibration, dict) and calibration.get("mimo_on") is True
    except (OSError, UnicodeError, json.JSONDecodeError, AttributeError):
        result.failed_stage = "calibrate"
        return result

    calibrate = prefix + ["calibrate"]
    if mimo_on:
        calibrate.append("--mimo")
    calibrate.append(str(relative_project))
    if not _cli_succeeded(calibrate, variant_root):
        result.failed_stage = "calibrate"
        return result

    export = prefix + [
        "export",
        "--kind",
        "convolution",
        "--out",
        "export",
        str(relative_project),
    ]
    if not _cli_succeeded(export, variant_root):
        result.failed_stage = "export"
        return result

    try:
        result.score = metrics.score_case(case_dir, export_dir, case_json)
    except Exception:
        result.failed_stage = "score"
    return result


def _valid_verdicts(verdicts: Any) -> bool:
    return isinstance(verdicts, dict) and all(
        isinstance(metric, str) and verdict in VERDICT_WORDS
        for metric, verdict in verdicts.items()
    )


def _judge_results(results: list[VariantResult], metrics: Any) -> dict[str, Any]:
    references: list[dict[str, Any]] = []
    seen_art_cases: set[str] = set()
    for result in results:
        if (
            result.case.kind == "art"
            and result.failed_stage is None
            and result.score is not None
            and result.case.name not in seen_art_cases
        ):
            art = result.score.get("art")
            if art is not None:
                references.append(art)
                seen_art_cases.add(result.case.name)

    pooled: dict[str, Any] = {}
    pool_failed = False
    if references:
        try:
            pooled = metrics.pool(references)
            if not isinstance(pooled, dict):
                raise TypeError
        except Exception:
            pooled = {}
            pool_failed = True

    for result in results:
        if result.failed_stage is not None or result.score is None:
            continue
        try:
            score = result.score
            if result.case.kind == "art":
                if score.get("art") is None:
                    raise ValueError
                verdicts = dict(
                    metrics.judge(score["predicted"], score["art"], result.case.groups)
                )
                verdicts.update(
                    metrics.judge_phase(
                        score.get("predicted_phase", {}), score.get("art_phase", {})
                    )
                )
            else:
                if pool_failed:
                    raise ValueError
                cross_room = tuple(
                    group
                    for group in result.case.groups
                    if group in metrics.CROSS_ROOM_GROUPS
                )
                verdicts = dict(
                    metrics.judge_pooled(score["predicted"], pooled, cross_room)
                )
            if not _valid_verdicts(verdicts):
                raise ValueError
            grade = metrics.grade(verdicts)
            if grade not in GRADE_WORDS:
                raise ValueError
            result.verdicts = verdicts
            result.grade = grade
        except Exception:
            result.verdicts = {}
            result.grade = None
            result.failed_stage = "score"
    return pooled


def _print_results(results: Sequence[VariantResult], out: TextIO) -> None:
    for result in results:
        if result.failed_stage is not None:
            print(
                f"case={result.case.name} variant={result.variant} "
                f"failed stage={result.failed_stage}",
                file=out,
            )
            continue
        for metric, verdict in result.verdicts.items():
            print(
                f"case={result.case.name} variant={result.variant} "
                f"metric={metric} verdict={VERDICT_WORDS[verdict]}",
                file=out,
            )
        counts = {
            verdict: sum(value == verdict for value in result.verdicts.values())
            for verdict in VERDICT_WORDS
        }
        print(
            f"case={result.case.name} variant={result.variant} "
            f"grade={GRADE_WORDS[result.grade or 'none']} "
            f"similar={counts['similar']} different={counts['different']} "
            f"unknown={counts['unknown']}",
            file=out,
        )


def run_gate(
    cases: Sequence[Case],
    manifest_dir: Path,
    cli: str | Path | Sequence[str],
    key: bytes,
    metrics: Any | None = None,
    out: TextIO = sys.stdout,
) -> GateResult:
    """Run all cases and variants, redacting values and child-process output."""
    if len(key) != KEY_SIZE:
        raise ConfigError("invalid ART reference key")
    if metrics is None:
        # Numeric dependencies are deliberately absent from verify/pack jobs.
        import art_metrics as metrics_module

        metrics = metrics_module

    results: list[VariantResult] = []
    temp_parent = os.environ.get("RUNNER_TEMP") or None
    with tempfile.TemporaryDirectory(dir=temp_parent) as temporary:
        work_root = Path(temporary)
        for case_index, case in enumerate(cases):
            bundle_path = manifest_dir / case.file
            try:
                data = bundle_path.read_bytes()
            except OSError:
                results.extend(_case_failures(case, "hash"))
                continue
            if hashlib.sha256(data).hexdigest() != case.sha256:
                results.extend(_case_failures(case, "hash"))
                continue

            case_dir = work_root / f"case-{case_index}"
            try:
                open_bundle(data, case.name, key, case_dir)
            except (BundleError, ConfigError):
                # The packet names hash and CLI stages but leaves bundle
                # authentication unnamed; report it as the "decrypt" stage.
                results.extend(_case_failures(case, "decrypt"))
                continue
            try:
                case_json = validate_case_dir(case_dir)
            except ConfigError:
                results.extend(_case_failures(case, "case"))
                continue
            if case.kind == "art" and "art" not in case_json:
                results.extend(_case_failures(case, "case"))
                continue

            for variant_index, (variant_name, arguments) in enumerate(
                case.variants.items()
            ):
                variant_root = work_root / f"variant-{case_index}-{variant_index}"
                variant_root.mkdir()
                try:
                    result = _run_variant(
                        case,
                        variant_name,
                        arguments,
                        case_dir,
                        case_json,
                        variant_root,
                        cli,
                        metrics,
                    )
                except Exception:
                    result = VariantResult(
                        case=case, variant=variant_name, failed_stage="case"
                    )
                results.append(result)

    pooled = _judge_results(results, metrics)
    _print_results(results, out)
    return GateResult(variants=results, pooled=pooled)


def append_summary(path: Path, result: GateResult) -> None:
    """Append the value-free Markdown summary required by ADR 0026."""
    lines = [
        "## ART reference (similarity)",
        "",
        "판정은 참고 정보이며 지표값은 표시하지 않습니다.",
        "",
        "| 사례 | 기준 | 설정 | 등급 | 비슷함 | 아님 | 판정 불가 |",
        "| --- | --- | --- | --- | ---: | ---: | ---: |",
    ]
    for variant in result.variants:
        if variant.failed_stage is not None:
            grade = f"계산 실패 ({variant.failed_stage})"
            counts = ("-", "-", "-")
        else:
            grade = GRADE_WORDS[variant.grade or "none"]
            counts = tuple(
                str(sum(value == verdict for value in variant.verdicts.values()))
                for verdict in ("similar", "different", "unknown")
            )
        lines.append(
            f"| {variant.case.name} | {KIND_WORDS[variant.case.kind]} | "
            f"{variant.variant} | {grade} | {counts[0]} | {counts[1]} | {counts[2]} |"
        )

    case_order: list[str] = []
    by_case: dict[str, list[VariantResult]] = {}
    for variant in result.variants:
        if variant.case.name not in by_case:
            case_order.append(variant.case.name)
            by_case[variant.case.name] = []
        by_case[variant.case.name].append(variant)
    for case_name in case_order:
        variants = by_case[case_name]
        metric_order: list[str] = []
        for variant in variants:
            for metric in variant.verdicts:
                if metric not in metric_order:
                    metric_order.append(metric)
        lines.extend(
            [
                "",
                f"### {case_name}",
                "",
                "| 지표 | " + " | ".join(item.variant for item in variants) + " |",
                "| --- | " + " | ".join("---" for _ in variants) + " |",
            ]
        )
        for metric in metric_order:
            cells = []
            for variant in variants:
                if variant.failed_stage is not None:
                    cells.append(f"계산 실패 ({variant.failed_stage})")
                elif metric in variant.verdicts:
                    cells.append(VERDICT_WORDS[variant.verdicts[metric]])
                else:
                    cells.append("-")
            lines.append(f"| {metric} | " + " | ".join(cells) + " |")

    prefix = ""
    if path.exists() and path.stat().st_size:
        prefix = "\n"
    with path.open("a", encoding="utf-8", newline="\n") as handle:
        handle.write(prefix + "\n".join(lines) + "\n")


def _json_safe(value: Any) -> Any:
    if isinstance(value, float):
        return value if math.isfinite(value) else None
    if isinstance(value, dict):
        return {str(key): _json_safe(item) for key, item in value.items()}
    if isinstance(value, (list, tuple)):
        return [_json_safe(item) for item in value]
    if value is None or isinstance(value, (str, int, bool)):
        return value
    item = getattr(value, "item", None)
    if callable(item):
        return _json_safe(item())
    return value


def write_values(path: Path, result: GateResult) -> None:
    """Write local-only numeric details, converting non-finite floats to null."""
    document: dict[str, Any] = {}
    for variant in result.variants:
        if variant.failed_stage is not None or variant.score is None:
            continue
        case_values = document.setdefault(variant.case.name, {})
        score = variant.score
        case_values[variant.variant] = {
            "predicted": score.get("predicted"),
            "predicted_phase": score.get("predicted_phase"),
            "art": score.get("art"),
            "art_phase": score.get("art_phase"),
            "verdicts": variant.verdicts,
            "grade": variant.grade,
        }
    document["pooled"] = result.pooled
    path.write_text(
        json.dumps(_json_safe(document), ensure_ascii=False, indent=2, allow_nan=False)
        + "\n",
        encoding="utf-8",
    )


def generate_key(path: Path) -> None:
    """Create a new base64 key file with mode 0600 and no overwrite."""
    encoded = base64.b64encode(os.urandom(KEY_SIZE)) + b"\n"
    flags = os.O_WRONLY | os.O_CREAT | os.O_EXCL
    try:
        descriptor = os.open(path, flags, 0o600)
    except FileExistsError as error:
        raise ConfigError("keygen: output file already exists") from error
    try:
        with os.fdopen(descriptor, "wb") as handle:
            handle.write(encoded)
    except Exception:
        try:
            os.close(descriptor)
        except OSError:
            pass
        raise


def _select_cases(cases: list[Case], names: list[str] | None) -> list[Case]:
    if not names:
        return cases
    available = {case.name for case in cases}
    for name in names:
        if name not in available:
            raise ConfigError(f"case {name}: unknown case name")
    selected = set(names)
    return [case for case in cases if case.name in selected]


def _key_from_environment(variable: str) -> bytes | None:
    value = os.environ.get(variable)
    if value is None or not value.strip():
        return None
    return parse_key(value)


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    commands = parser.add_subparsers(dest="command", required=True)

    run = commands.add_parser("run", help="run encrypted ART reference cases")
    run.add_argument("--manifest", type=Path, required=True)
    run.add_argument("--cli", type=Path, required=True)
    # Repeating --cli-arg supplies launch arguments before the SECS CLI's own
    # flags, chiefly ``--cli python --cli-arg fake_cli.py`` in portable tests.
    run.add_argument("--cli-arg", action="append", default=[])
    run.add_argument("--key-env", default="ART_REFERENCE_KEY")
    run.add_argument("--require-key", action="store_true")
    run.add_argument("--case", dest="case_names", action="append")
    run.add_argument("--summary", type=Path)
    run.add_argument("--values-out", type=Path)

    pack = commands.add_parser("pack", help="pack and encrypt one ART case")
    pack.add_argument("--case-dir", type=Path, required=True)
    pack.add_argument("--name", required=True)
    pack.add_argument("--out", type=Path, required=True)
    pack.add_argument("--key-env", default="ART_REFERENCE_KEY")

    verify = commands.add_parser("verify", help="verify ciphertext hashes")
    verify.add_argument("--manifest", type=Path, required=True)

    keygen = commands.add_parser("keygen", help="generate a new ART key")
    keygen.add_argument("--out", type=Path, required=True)
    return parser


def utf8_stdio() -> None:
    """Korean verdict words need UTF-8 on Windows runners."""
    for stream in (sys.stdout, sys.stderr):
        reconfigure = getattr(stream, "reconfigure", None)
        if reconfigure is not None:
            reconfigure(encoding="utf-8", errors="replace")


def main(argv: list[str] | None = None) -> int:
    utf8_stdio()
    parser = build_parser()
    args = parser.parse_args(argv)
    try:
        if args.command == "verify":
            cases = load_manifest(args.manifest)
            return verify_cases(cases, args.manifest.parent)

        if args.command == "keygen":
            generate_key(args.out)
            print(args.out.name)
            return 0

        if args.command == "pack":
            key = _key_from_environment(args.key_env)
            if key is None:
                raise ConfigError("ART reference: key not set")
            digest, size = pack_case(args.case_dir, args.name, args.out, key)
            print(f"sha256={digest}")
            print(f"bytes={size}")
            return 0

        if args.values_out is not None and (
            os.environ.get("GITHUB_ACTIONS") == "true" or "CI" in os.environ
        ):
            raise ConfigError("--values-out is not allowed in CI")
        cases = load_manifest(args.manifest)
        cases = _select_cases(cases, args.case_names)
        key = _key_from_environment(args.key_env)
        if key is None:
            if args.require_key:
                raise ConfigError("ART reference: key not set")
            print("ART reference: key not set, skipped")
            return 0
        cli = [resolve_cli(args.cli), *args.cli_arg]
        result = run_gate(cases, args.manifest.parent, cli, key)
        if args.summary is not None:
            append_summary(args.summary, result)
        if args.values_out is not None:
            write_values(args.values_out, result)
        return result.exit_code
    except ConfigError as error:
        print(str(error), file=sys.stderr)
        return 2
    except Exception:
        # Child and metric exception messages can contain private values or
        # absolute work paths.  Unexpected failures therefore stay generic.
        print("ART reference: operation failed", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
