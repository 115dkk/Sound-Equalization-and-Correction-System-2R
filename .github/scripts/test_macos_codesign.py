"""Unit tests for macos_codesign.py (python -m unittest, no dependencies)."""
from __future__ import annotations

import base64
import io
import subprocess
import tempfile
import unittest
from contextlib import redirect_stdout
from pathlib import Path
from unittest.mock import patch

import macos_codesign as m


CERTIFICATE = """-----BEGIN CERTIFICATE-----
MIIC8zCCAdugAwIBAgIUOyEv/vOerV+D8zCm0giDLo9XTxgwDQYJKoZIhvcNAQEL
BQAwIjEQMA4GA1UEAwwHZml4dHVyZTEOMAwGA1UECgwFdGVzdHMwHhcNMjYxMTA2
MDIzNDIwWhcNMjYxMTA3MDIzNDIwWjAiMRAwDgYDVQQDDAdmaXh0dXJlMQ4wDAYD
VQQKDAV0ZXN0czCCASIwDQYJKoZIhvcNAQEBBQADggEPADCCAQoCggEBAMKQ5+XB
4Kgx2MmoEnrN5z8XfERJePj3n3az40SV+BR+fqDdiFZ5PcQBQk60bG4Abw6EkqRg
9rrGKFG5b7Kb8cHPOmBnWa526mHcjC1qO5xh7Xtj2EU1/SDgGHEsGcT9mtPTfqJx
BZveiAoU40tdA8h16cF4ajmoVl+wZbd9oPceHEeVEjmKxAcbAZJTDnkYdod40N0V
jmp2Bn4qgiVfECW1Oerr5bQXWniM0iyZLE4FqTERQiQwRN/R5qY1YAbm4fZZfE06
IAtoOVerRohfNDLdZt5RIxUQ8ismaT+wQoo5X/Q4e3u52/ibUg1nJk8C+CG2jVby
5GTctNkH/gChjtUCAwEAAaMhMB8wHQYDVR0OBBYEFE1oJ6kITx6t+XA1GQB3sLMr
SPvFMA0GCSqGSIb3DQEBCwUAA4IBAQAEfDDeJ4Rt1T7CNnK840P0JzS6MT9m/Gsk
Px2pmxMqhlzYWuixvDUol/cyL6IPJty/EhGnV3KXuEY1cmg4f1tZc4QfR30B6Uzw
uyad+Zsa/1W+NRqK6RX97OTvO3dZyH/4RLfcc313EcaGzfD2S4jpRcGdqtnCp4Em
2emUJJpmWzuhpgJYrkTNj9lEO6NX7PbBAec6BbEg8UItl89fyn5MXwlBla6MDqaS
hda4l9tfvv7peINPIK8Fp5UczDTWYa4LabS1axn1k8nKyRoXpLJQyttGsrJ4fbJY
JX2bk8sA/DD8RClGA25MGjxopb7H3VvK+W9q+fEY/G3rTDqsCvAh
-----END CERTIFICATE-----
"""
SHA1 = "382ab2b320c0cb45bcf24d3181ddb364687a6f70"
SHA256 = "d3cfff526a5ecdbe34e327658d8306a795ccc3ffd017ce92c222dc885dc5c662"
CN = "SECS 2R (self-signed)"
ORGANIZATION = "SECS 2R contributors"


def completed(command: list[str], stdout: str = "") -> subprocess.CompletedProcess[str]:
    return subprocess.CompletedProcess(command, 0, stdout, "")


class CertificateTest(unittest.TestCase):
    def test_openssl_config_has_the_portable_codesign_request(self):
        self.assertEqual(m.openssl_config("Name", "Organization"), """[req]
prompt = no
distinguished_name = subject
x509_extensions = codesign

[subject]
CN = Name
O = Organization

[codesign]
keyUsage = critical, digitalSignature
extendedKeyUsage = critical, codeSigning
subjectKeyIdentifier = hash
""")

    def test_der_and_fingerprints_match_openssl(self):
        der = m.der_of_pem("ignored\n" + CERTIFICATE + "ignored\n")
        self.assertEqual(m.fingerprints(der), {"sha1": SHA1, "sha256": SHA256})

    def test_der_needs_a_certificate_block(self):
        with self.assertRaisesRegex(ValueError, "no CERTIFICATE block"):
            m.der_of_pem("not a certificate")


class ParsingTest(unittest.TestCase):
    def test_common_name_accepts_both_subject_spellings(self):
        for prefix in ("subject=", "subject= "):
            self.assertEqual(
                m.common_name(f"{prefix}CN={CN},O={ORGANIZATION}"), CN
            )

    def test_common_name_unescapes_comma_and_equals(self):
        self.assertEqual(
            m.common_name(r"subject=O=Example,CN=SECS\, 2R\=CI"),
            "SECS, 2R=CI",
        )

    def test_common_name_requires_cn(self):
        with self.assertRaisesRegex(ValueError, "no CN="):
            m.common_name(f"subject=O={ORGANIZATION}")

    def test_keychain_list_trims_real_security_output(self):
        output = """    \"/Users/runner/Library/Keychains/login.keychain-db\"
        \"/Library/Keychains/System.keychain\"

"""
        self.assertEqual(m.keychain_list(output), [
            "/Users/runner/Library/Keychains/login.keychain-db",
            "/Library/Keychains/System.keychain",
        ])

    def test_identity_needs_the_exact_quoted_name(self):
        listing = f'  1) ABCDEF1234 \"{CN}\"\n     1 valid identities found\n'
        self.assertTrue(m.identity_listed(listing, CN))
        self.assertFalse(m.identity_listed(listing, "SECS 2R"))
        self.assertFalse(m.identity_listed("0 valid identities found\n", CN))


class PrepareKeychainTest(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.work = Path(self.temporary.name)
        self.p12 = self.work / "identity.p12"
        self.p12.write_bytes(b"private p12 fixture")
        self.certificate = self.work / "identity.pem"
        self.certificate.write_text(CERTIFICATE, encoding="ascii")
        self.keychain = self.work / "codesign" / "build.keychain-db"
        self.github_env = self.work / "github.env"
        self.existing = [
            "/Users/runner/Library/Keychains/login.keychain-db",
            "/Library/Keychains/System.keychain",
        ]

    def tearDown(self):
        self.temporary.cleanup()

    def successful_run(self, command: list[str], **kwargs) -> subprocess.CompletedProcess[str]:
        if command[:2] == ["openssl", "x509"]:
            return completed(command, f"subject=CN={CN},O={ORGANIZATION}\n")
        if command[:4] == ["security", "list-keychains", "-d", "user"] and len(command) == 4:
            stdout = "".join(f'    "{path}"\n' for path in self.existing)
            return completed(command, stdout)
        if command[:2] == ["sudo", "security"]:
            raise m.MacOSCodesignError("system trust test failure")
        if command[:2] == ["security", "find-certificate"]:
            return completed(command, CERTIFICATE)
        if command[:3] == ["security", "find-identity", "-v"]:
            return completed(command, f'  1) ABCD "{CN}"\n')
        return completed(command)

    def test_prepare_runs_in_order_redacts_passwords_and_appends_on_success(self):
        with (patch.object(m.sys, "platform", "darwin"),
              patch.object(m.secrets, "token_urlsafe", return_value="keychain-secret"),
              patch.object(m, "run", side_effect=self.successful_run) as run,
              redirect_stdout(io.StringIO())):
            result = m.prepare_keychain(
                self.p12, "p12-secret", self.certificate, self.keychain,
                self.github_env,
            )

        expected = [
            ["openssl", "x509", "-in", str(self.certificate), "-noout", "-subject",
             "-nameopt", "RFC2253"],
            ["security", "create-keychain", "-p", "keychain-secret", str(self.keychain)],
            ["security", "set-keychain-settings", str(self.keychain)],
            ["security", "unlock-keychain", "-p", "keychain-secret", str(self.keychain)],
            ["security", "import", str(self.p12), "-k", str(self.keychain), "-f", "pkcs12",
             "-P", "p12-secret", "-T", "/usr/bin/codesign", "-T", "/usr/bin/security"],
            ["security", "set-key-partition-list", "-S", "apple-tool:,apple:,codesign:",
             "-s", "-k", "keychain-secret", str(self.keychain)],
            ["security", "list-keychains", "-d", "user"],
            ["security", "list-keychains", "-d", "user", "-s", str(self.keychain),
             *self.existing],
            ["sudo", "security", "add-trusted-cert", "-d", "-r", "trustRoot", "-p",
             "codeSign", "-k", "/Library/Keychains/System.keychain", str(self.certificate)],
            ["security", "add-trusted-cert", "-r", "trustRoot", "-p", "codeSign", "-k",
             str(self.keychain), str(self.certificate)],
            ["security", "find-certificate", "-c", CN, "-a", "-p", str(self.keychain)],
            ["security", "find-identity", "-v", "-p", "codesigning", str(self.keychain)],
        ]
        commands = [call.args[0] for call in run.call_args_list]
        self.assertEqual(commands, expected)
        self.assertEqual(run.call_args_list[8].kwargs, {"timeout": 90})
        self.assertEqual(run.call_args_list[9].kwargs, {"timeout": 90})
        for command in commands:
            printed = m.printable_command(command)
            self.assertNotIn("keychain-secret", printed)
            self.assertNotIn("p12-secret", printed)
        self.assertIn("-p codeSign", m.printable_command(commands[8]))
        self.assertTrue(self.keychain.parent.is_dir())
        self.assertEqual(result, CN)
        self.assertEqual(
            self.github_env.read_text(encoding="utf-8"),
            f"APPLE_SIGNING_IDENTITY={CN}\n",
        )

    def test_github_environment_is_unchanged_when_certificate_differs(self):
        self.github_env.write_text("KEEP=1\n", encoding="utf-8")

        def different_certificate(command: list[str], **kwargs):
            if command[:2] == ["openssl", "x509"]:
                return completed(command, f"subject=CN={CN},O={ORGANIZATION}\n")
            if command[:4] == ["security", "list-keychains", "-d", "user"] and len(command) == 4:
                return completed(command, "")
            if command[:2] == ["security", "find-certificate"]:
                return completed(command, "no certificate")
            return completed(command)

        with (patch.object(m.sys, "platform", "darwin"),
              patch.object(m, "run", side_effect=different_certificate),
              redirect_stdout(io.StringIO()),
              self.assertRaisesRegex(m.MacOSCodesignError, "different certificate")):
            m.prepare_keychain(
                self.p12, "p12-secret", self.certificate, self.keychain,
                self.github_env,
            )
        self.assertEqual(self.github_env.read_text(encoding="utf-8"), "KEEP=1\n")


class CommandLineTest(unittest.TestCase):
    def test_p12_environment_strips_whitespace_and_decodes(self):
        with tempfile.TemporaryDirectory() as folder:
            work = Path(folder)
            encoded = base64.b64encode(b"p12 bytes").decode("ascii")
            wrapped = f"  {encoded[:4]}\n{encoded[4:]}  "
            with (patch.object(m.sys, "platform", "darwin"),
                  patch.dict(m.os.environ, {"P12_DATA": wrapped, "P12_PASSWORD": "secret"}),
                  patch.object(m, "prepare_keychain", return_value=CN) as prepare):
                status = m.main([
                    "prepare-keychain", "--p12-env", "P12_DATA",
                    "--password-env", "P12_PASSWORD", "--certificate",
                    str(work / "public.pem"), "--keychain", str(work / "build.keychain"),
                ])
            self.assertEqual(status, 0)
            written = work / "signing.p12"
            self.assertEqual(written.read_bytes(), b"p12 bytes")
            self.assertEqual(prepare.call_args.args[0], written)
            self.assertEqual(prepare.call_args.args[1], "secret")

    def test_empty_p12_environment_fails_without_printing_its_value(self):
        with tempfile.TemporaryDirectory() as folder:
            work = Path(folder)
            output = io.StringIO()
            with (patch.object(m.sys, "platform", "darwin"),
                  patch.dict(m.os.environ, {"EMPTY_P12": "", "P12_PASSWORD": "secret"}),
                  redirect_stdout(output)):
                status = m.main([
                    "prepare-keychain", "--p12-env", "EMPTY_P12",
                    "--password-env", "P12_PASSWORD", "--certificate",
                    str(work / "public.pem"), "--keychain", str(work / "build.keychain"),
                ])
            self.assertEqual(status, 1)
            self.assertEqual(
                output.getvalue(),
                "::error::environment variable EMPTY_P12 is unset or empty\n",
            )


if __name__ == "__main__":
    unittest.main()
