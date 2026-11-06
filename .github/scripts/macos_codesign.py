#!/usr/bin/env python3
"""Create and install the stable self-signed identity used to sign the macOS app.

The certificate command makes the same code-signing certificate shape on every
CI operating system. The keychain command proves that the imported PKCS#12
contains that public certificate and that macOS accepts its identity for code
signing. Pure parsing and fingerprint helpers are unit-tested in
``test_macos_codesign.py``.
"""
from __future__ import annotations

import argparse
import base64
import binascii
import hashlib
import os
import re
import secrets
import subprocess
import sys
from pathlib import Path


class MacOSCodesignError(RuntimeError):
    """A certificate, keychain, trust or identity failure."""


class CodesignArgumentParser(argparse.ArgumentParser):
    """Make command-line mistakes use the script's one-line error contract."""

    def error(self, message: str) -> None:
        raise MacOSCodesignError(message)


_CERTIFICATE = re.compile(
    r"-----BEGIN CERTIFICATE-----\s*(.*?)\s*-----END CERTIFICATE-----",
    re.DOTALL,
)


def openssl_config(name: str, organization: str) -> str:
    """The portable OpenSSL request configuration for the signing certificate."""
    return f"""[req]
prompt = no
distinguished_name = subject
x509_extensions = codesign

[subject]
CN = {name}
O = {organization}

[codesign]
keyUsage = critical, digitalSignature
extendedKeyUsage = critical, codeSigning
subjectKeyIdentifier = hash
"""


def _decode_certificate(payload: str) -> bytes:
    encoded = re.sub(r"\s+", "", payload)
    if not encoded:
        raise ValueError("CERTIFICATE block is empty")
    try:
        return base64.b64decode(encoded, validate=True)
    except (binascii.Error, ValueError) as error:
        raise ValueError("CERTIFICATE block is not valid base64") from error


def der_of_pem(text: str) -> bytes:
    """Return the DER in the first PEM CERTIFICATE block."""
    match = _CERTIFICATE.search(text)
    if match is None:
        raise ValueError("PEM has no CERTIFICATE block")
    return _decode_certificate(match.group(1))


def _all_der_of_pem(text: str) -> list[bytes]:
    return [_decode_certificate(match.group(1)) for match in _CERTIFICATE.finditer(text)]


def fingerprints(der: bytes) -> dict[str, str]:
    """Return the lowercase SHA-1 and SHA-256 certificate fingerprints."""
    return {
        "sha1": hashlib.sha1(der).hexdigest(),
        "sha256": hashlib.sha256(der).hexdigest(),
    }


def common_name(subject_line: str) -> str:
    """Extract CN from an OpenSSL RFC2253 subject line."""
    subject = subject_line.strip()
    if subject.startswith("subject="):
        subject = subject[len("subject="):].lstrip()

    components: list[str] = []
    start = 0
    escaped = False
    for index, character in enumerate(subject):
        if escaped:
            escaped = False
        elif character == "\\":
            escaped = True
        elif character == ",":
            components.append(subject[start:index])
            start = index + 1
    components.append(subject[start:])

    for component in components:
        component = component.lstrip()
        if component.startswith("CN="):
            value = component[len("CN="):]
            return value.replace(r"\,", ",").replace(r"\=", "=")
    raise ValueError("certificate subject has no CN=")


def keychain_list(stdout: str) -> list[str]:
    """Parse ``security list-keychains`` output without its indentation or quotes."""
    paths = []
    for line in stdout.splitlines():
        value = line.strip()
        if not value:
            continue
        if len(value) >= 2 and value[0] == value[-1] == '"':
            value = value[1:-1]
        paths.append(value)
    return paths


def identity_listed(stdout: str, cn: str) -> bool:
    """Whether a security identity listing contains the exact quoted name."""
    return f'"{cn}"' in stdout


def _password_positions(command: list[str]) -> set[int]:
    security = None
    if len(command) >= 2 and command[0] == "security":
        security = 1
    elif len(command) >= 3 and command[:2] == ["sudo", "security"]:
        security = 2
    if security is None:
        return set()

    subcommand = command[security]
    option = {
        "create-keychain": "-p",
        "unlock-keychain": "-p",
        "import": "-P",
        "set-key-partition-list": "-k",
    }.get(subcommand)
    if option is None:
        return set()
    return {index + 1 for index, value in enumerate(command[:-1]) if value == option}


def printable_command(command: list[str]) -> str:
    """Render a command exactly as ``run`` logs it, with passwords hidden."""
    password_positions = _password_positions(command)
    shown = ["***" if index in password_positions else value
             for index, value in enumerate(command)]
    return "$ " + " ".join(shown)


def _password_values(command: list[str]) -> list[str]:
    return [command[index] for index in _password_positions(command) if command[index]]


def _redact(text: str, values: list[str]) -> str:
    for value in values:
        text = text.replace(value, "***")
    return text


def run(command: list[str], **kwargs) -> subprocess.CompletedProcess[str]:
    """Run a command, printing its password-free command line and output."""
    shown = printable_command(command)
    password_values = _password_values(command)
    print(shown, flush=True)
    try:
        result = subprocess.run(command, capture_output=True, text=True, **kwargs)
    except subprocess.TimeoutExpired as error:
        timeout = error.timeout if error.timeout is not None else kwargs.get("timeout")
        raise MacOSCodesignError(f"{shown[2:]} timed out after {timeout} seconds") from None
    for stream in (result.stdout, result.stderr):
        if stream.strip():
            print(_redact(stream.rstrip(), password_values), flush=True)
    if result.returncode:
        raise MacOSCodesignError(f"{shown[2:]} failed with exit code {result.returncode}")
    return result


def _private_mode(path: Path) -> None:
    if os.name == "posix" and path.exists():
        path.chmod(0o600)


def _ensure_private_directory(path: Path) -> None:
    was_missing = not path.exists()
    path.mkdir(mode=0o700, parents=True, exist_ok=True)
    if not path.is_dir():
        raise MacOSCodesignError(f"keychain parent is not a directory: {path}")
    if os.name == "posix" and was_missing:
        path.chmod(0o700)


def _write_exclusive(path: Path, data: bytes, mode: int) -> None:
    descriptor = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, mode)
    try:
        with os.fdopen(descriptor, "wb") as stream:
            stream.write(data)
    except BaseException:
        path.unlink(missing_ok=True)
        raise


def _validate_config_value(label: str, value: str) -> None:
    if not value or any(character in value for character in "\r\n\0"):
        raise MacOSCodesignError(f"{label} must be a non-empty single line")


def make_certificate(name: str, organization: str, out_dir: Path, stem: str,
                     days: int, bits: int) -> None:
    """Create one private key, self-signed certificate and portable PKCS#12."""
    _validate_config_value("certificate name", name)
    _validate_config_value("organization", organization)
    if not stem or stem in {".", ".."} or "/" in stem or "\\" in stem:
        raise MacOSCodesignError("stem must be a file name without a directory")
    if days <= 0:
        raise MacOSCodesignError("days must be greater than zero")
    if bits <= 0:
        raise MacOSCodesignError("bits must be greater than zero")

    directory_was_missing = not out_dir.exists()
    out_dir.mkdir(mode=0o700, parents=True, exist_ok=True)
    if not out_dir.is_dir():
        raise MacOSCodesignError(f"output directory is not a directory: {out_dir}")
    if os.name == "posix" and directory_was_missing:
        out_dir.chmod(0o700)

    key = out_dir / f"{stem}.key"
    certificate = out_dir / f"{stem}.pem"
    p12 = out_dir / f"{stem}.p12"
    password_file = out_dir / f"{stem}.p12.password"
    config = out_dir / f"{stem}.cnf"
    outputs = (key, certificate, p12, password_file, config)
    existing = next((path for path in outputs if os.path.lexists(path)), None)
    if existing is not None:
        raise MacOSCodesignError(f"refusing to overwrite existing output: {existing}")

    password = secrets.token_urlsafe(32)
    config_created = False
    try:
        _write_exclusive(config, openssl_config(name, organization).encode("utf-8"), 0o600)
        config_created = True
        _write_exclusive(password_file, password.encode("utf-8"), 0o600)
        run([
            "openssl", "req", "-x509", "-new", "-newkey", f"rsa:{bits}",
            "-sha256", "-nodes", "-days", str(days), "-keyout", str(key),
            "-out", str(certificate), "-config", str(config),
        ])
        _private_mode(key)
        run([
            "openssl", "pkcs12", "-export", "-inkey", str(key), "-in",
            str(certificate), "-name", name, "-out", str(p12), "-passout",
            f"file:{password_file}", "-keypbe", "PBE-SHA1-3DES", "-certpbe",
            "PBE-SHA1-3DES", "-macalg", "sha1",
        ])
        _private_mode(p12)
    finally:
        _private_mode(key)
        _private_mode(p12)
        _private_mode(password_file)
        if config_created:
            config.unlink(missing_ok=True)

    run([
        "openssl", "x509", "-in", str(certificate), "-noout", "-subject",
        "-enddate", "-nameopt", "RFC2253",
    ])
    for algorithm, value in fingerprints(der_of_pem(
            certificate.read_text(encoding="ascii"))).items():
        print(f"{algorithm}={value}", flush=True)


def _write_environment_p12(variable: str, keychain: Path,
                           environ: dict[str, str] | os._Environ[str] | None = None) -> Path:
    environment = os.environ if environ is None else environ
    encoded = environment.get(variable)
    if encoded is None or not encoded.strip():
        raise MacOSCodesignError(f"environment variable {variable} is unset or empty")
    compact = "".join(encoded.split())
    try:
        data = base64.b64decode(compact, validate=True)
    except (binascii.Error, ValueError) as error:
        raise MacOSCodesignError(
            f"environment variable {variable} is not valid base64"
        ) from error
    if not data:
        raise MacOSCodesignError(f"environment variable {variable} decodes to an empty p12")

    _ensure_private_directory(keychain.parent)
    target = keychain.parent / "signing.p12"
    target.unlink(missing_ok=True)
    _write_exclusive(target, data, 0o600)
    _private_mode(target)
    return target


def _password_from_file(path: Path) -> str:
    with path.open("r", encoding="utf-8") as stream:
        first_line = stream.readline()
    if first_line == "":
        raise MacOSCodesignError(f"password file is empty: {path}")
    password = first_line.rstrip("\r\n")
    if not password:
        raise MacOSCodesignError(f"password file has an empty first line: {path}")
    return password


def _password_from_environment(variable: str,
                               environ: dict[str, str] | os._Environ[str] | None = None) -> str:
    environment = os.environ if environ is None else environ
    password = environment.get(variable)
    if password is None or not password:
        raise MacOSCodesignError(f"environment variable {variable} is unset or empty")
    return password


def prepare_keychain(p12: Path, p12_password: str, certificate: Path,
                     keychain: Path, github_env: Path | None = None) -> str:
    """Put the expected identity in a never-locking macOS build keychain."""
    if sys.platform != "darwin":
        raise MacOSCodesignError("prepare-keychain runs on macOS")
    if not p12.is_file():
        raise MacOSCodesignError(f"p12 file is missing: {p12}")
    if not p12_password:
        raise MacOSCodesignError("p12 password is empty")

    certificate_text = certificate.read_text(encoding="ascii")
    expected_sha1 = fingerprints(der_of_pem(certificate_text))["sha1"]
    kc_password = secrets.token_urlsafe(24)
    subject = run([
        "openssl", "x509", "-in", str(certificate), "-noout", "-subject",
        "-nameopt", "RFC2253",
    ]).stdout
    cn = common_name(subject)

    _ensure_private_directory(keychain.parent)
    keychain.unlink(missing_ok=True)
    run(["security", "create-keychain", "-p", kc_password, str(keychain)])
    run(["security", "set-keychain-settings", str(keychain)])
    run(["security", "unlock-keychain", "-p", kc_password, str(keychain)])
    run([
        "security", "import", str(p12), "-k", str(keychain), "-f", "pkcs12",
        "-P", p12_password, "-T", "/usr/bin/codesign", "-T",
        "/usr/bin/security",
    ])
    run([
        "security", "set-key-partition-list", "-S",
        "apple-tool:,apple:,codesign:", "-s", "-k", kc_password, str(keychain),
    ])
    listed = keychain_list(run(["security", "list-keychains", "-d", "user"]).stdout)
    run(["security", "list-keychains", "-d", "user", "-s", str(keychain), *listed])

    system_trust = [
        "sudo", "security", "add-trusted-cert", "-d", "-r", "trustRoot",
        "-p", "codeSign", "-k", "/Library/Keychains/System.keychain",
        str(certificate),
    ]
    try:
        run(system_trust, timeout=90)
    except MacOSCodesignError as error:
        print(
            f"::warning::System.keychain trust failed: {' '.join(str(error).splitlines())}; "
            "trying the build keychain",
            flush=True,
        )
        run([
            "security", "add-trusted-cert", "-r", "trustRoot", "-p",
            "codeSign", "-k", str(keychain), str(certificate),
        ], timeout=90)

    imported = run([
        "security", "find-certificate", "-c", cn, "-a", "-p", str(keychain),
    ]).stdout
    actual_sha1 = {fingerprints(der)["sha1"] for der in _all_der_of_pem(imported)}
    if expected_sha1 not in actual_sha1:
        raise MacOSCodesignError(
            f"the p12 holds a different certificate than {certificate}"
        )

    valid = run([
        "security", "find-identity", "-v", "-p", "codesigning", str(keychain),
    ]).stdout
    if not identity_listed(valid, cn):
        all_identities = run([
            "security", "find-identity", "-p", "codesigning", str(keychain),
        ]).stdout
        if identity_listed(all_identities, cn):
            raise MacOSCodesignError(
                f'the identity "{cn}" is present but is not trusted for code signing'
            )
        raise MacOSCodesignError(f'the code-signing identity "{cn}" is missing')

    if github_env is not None:
        with github_env.open("a", encoding="utf-8", newline="\n") as stream:
            stream.write(f"APPLE_SIGNING_IDENTITY={cn}\n")
    print(f"identity={cn}", flush=True)
    return cn


def _build_parser() -> CodesignArgumentParser:
    parser = CodesignArgumentParser(description=__doc__.splitlines()[0])
    commands = parser.add_subparsers(dest="command", required=True)

    command = commands.add_parser("make-certificate")
    command.add_argument("--name", required=True)
    command.add_argument("--out-dir", type=Path, required=True)
    command.add_argument("--stem", required=True)
    command.add_argument("--organization", default="SECS 2R contributors")
    command.add_argument("--days", type=int, default=7305)
    command.add_argument("--bits", type=int, default=3072)

    command = commands.add_parser("prepare-keychain")
    p12 = command.add_mutually_exclusive_group(required=True)
    p12.add_argument("--p12", type=Path)
    p12.add_argument("--p12-env")
    password = command.add_mutually_exclusive_group(required=True)
    password.add_argument("--password-file", type=Path)
    password.add_argument("--password-env")
    command.add_argument("--certificate", type=Path, required=True)
    command.add_argument("--keychain", type=Path, required=True)
    command.add_argument("--github-env", type=Path)

    command = commands.add_parser("fingerprint")
    command.add_argument("pem", type=Path)
    return parser


def main(argv: list[str] | None = None) -> int:
    try:
        args = _build_parser().parse_args(argv)
        if args.command == "make-certificate":
            make_certificate(
                args.name, args.organization, args.out_dir, args.stem,
                args.days, args.bits,
            )
        elif args.command == "fingerprint":
            values = fingerprints(der_of_pem(args.pem.read_text(encoding="ascii")))
            for algorithm, value in values.items():
                print(f"{algorithm}={value}", flush=True)
        else:
            if sys.platform != "darwin":
                raise MacOSCodesignError("prepare-keychain runs on macOS")
            password = (_password_from_file(args.password_file)
                        if args.password_file is not None
                        else _password_from_environment(args.password_env))
            p12 = (args.p12 if args.p12 is not None
                   else _write_environment_p12(args.p12_env, args.keychain))
            prepare_keychain(
                p12, password, args.certificate, args.keychain, args.github_env,
            )
    except (MacOSCodesignError, OSError, ValueError) as error:
        problem = " ".join(str(error).splitlines()) or error.__class__.__name__
        print(f"::error::{problem}", flush=True)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
