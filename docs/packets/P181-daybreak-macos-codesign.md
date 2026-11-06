# P181 (Daybreak): macos_codesign.py, a self-signed signing certificate and the keychain that holds it

Tree: `E:\Sound-Equalization-and-Correction-System-2R` (branch `claude/public-api`).
Work only there, and create only these two files:

- `.github/scripts/macos_codesign.py`
- `.github/scripts/test_macos_codesign.py`

Do not touch any other file: Claude edits `.github/workflows/*.yml`,
`.github/scripts/macos_app_check.py`, its tests and the docs in the same tree
at the same time. Do not commit, do not push, do not spawn sub-agents.

Read first: `CLAUDE.md`, `.github/scripts/macos_app_check.py` (the style to
copy: a module docstring that says what the script proves, pure functions
that the unit tests cover, a `run()` helper that prints the command and its
output), `.github/scripts/test_macos_app_check.py`, `.github/scripts/blackhole.py`
(a macOS-only script with argparse subcommands).

## Why

The released macOS app was ad-hoc signed, so macOS treated every version as
a different program and asked for the microphone again after each update.
From now on the app is signed with the project's own self-signed certificate
(ADR 0041, which Claude writes). `release.yml` gets the certificate's private
half as a PKCS#12 secret; `ci.yml` makes a throwaway certificate on every run
so that pull requests exercise the same path. Both go through this script,
which puts the identity into a keychain on the runner; Tauri then signs with
`codesign -s "<common name>"` through the default keychain search list.
Tauri's own `APPLE_CERTIFICATE` import is not used: it only recognises
Apple-issued certificate names.

## CLI (fixed; the workflows call exactly this)

```
python -I -u .github/scripts/macos_codesign.py make-certificate --name "<CN>" --out-dir DIR --stem STEM [--organization "SECS 2R contributors"] [--days 7305] [--bits 3072]
python -I -u .github/scripts/macos_codesign.py prepare-keychain (--p12 FILE | --p12-env VAR) (--password-file FILE | --password-env VAR) --certificate PEM --keychain PATH [--github-env PATH]
python -I -u .github/scripts/macos_codesign.py fingerprint PEM
```

Exit 0 on success, 1 on any problem, with the problem on one `::error::`
line as `macos_app_check.py` does. Never print, log or write anywhere but
the named output files a private key, a p12 or a password; the `run()`
helper prints the command with every password argument replaced by `***`.

### make-certificate

Runs on Windows (Git for Windows, OpenSSL 3.5), on the macos-15 runner
(`/usr/bin/openssl` is LibreSSL 3.3; Homebrew OpenSSL 3 may come first on
PATH) and on Linux, with whatever `openssl` PATH resolves. Drive it with a
config file, not `-addext`, and name the PKCS#12 algorithms explicitly, so
one command works on all of them.

Writes into DIR (created if missing, mode 0o700 on POSIX; the key, the p12
and the password file 0o600 on POSIX). Refuses to overwrite an existing
output file.

- `STEM.key`: PEM private key, RSA of `--bits` (default 3072), unencrypted.
- `STEM.pem`: the self-signed X.509 certificate, SHA-256, valid `--days`
  (default 7305) from now.
- `STEM.p12`: PKCS#12 with key and certificate, friendly name = CN.
- `STEM.p12.password`: the p12 password, `secrets.token_urlsafe(32)`, one
  line, no trailing newline.

Subject `CN=<name>`, `O=<organization>`. Extensions: `keyUsage = critical,
digitalSignature`, `extendedKeyUsage = critical, codeSigning`,
`subjectKeyIdentifier = hash`. No basicConstraints: this matches the
"Self Signed Root, Code Signing" certificate Keychain Access makes, which
codesign accepts as its own anchor.

```
[req]
prompt = no
distinguished_name = subject
x509_extensions = codesign

[subject]
CN = <name>
O = <organization>

[codesign]
keyUsage = critical, digitalSignature
extendedKeyUsage = critical, codeSigning
subjectKeyIdentifier = hash
```

```
openssl req -x509 -new -newkey rsa:<bits> -sha256 -nodes -days <days> -keyout STEM.key -out STEM.pem -config STEM.cnf
openssl pkcs12 -export -inkey STEM.key -in STEM.pem -name "<name>" -out STEM.p12 -passout file:STEM.p12.password -keypbe PBE-SHA1-3DES -certpbe PBE-SHA1-3DES -macalg sha1
```

Write the password file before the pkcs12 command. Delete `STEM.cnf` at the
end. Then print the certificate's subject, `notAfter` and both fingerprints
(the `fingerprint` subcommand's output) so the workflow log shows which
certificate signed.

### fingerprint

Prints `sha1=<40 lowercase hex>` and `sha256=<64 lowercase hex>` of the DER
of the first `CERTIFICATE` block in PEM, in pure Python (`base64`,
`hashlib`; no openssl). Pure functions `der_of_pem(text: str) -> bytes`
(raises `ValueError` without a CERTIFICATE block) and
`fingerprints(der: bytes) -> dict[str, str]`. The SHA-1 is what a code
signing requirement's `certificate leaf = H"..."` carries, so other scripts
import these two functions.

### prepare-keychain (macOS only)

On another platform exit with `prepare-keychain runs on macOS`.

Inputs:

- the p12: `--p12 FILE`, or `--p12-env VAR` holding its base64 (strip all
  whitespace before `base64.b64decode`; an unset or empty variable is a
  problem, reported without the variable's value);
- its password: `--password-file FILE` (first line, trailing newline
  stripped) or `--password-env VAR`;
- `--certificate PEM`: the public half expected inside the p12;
- `--keychain PATH`: created new (delete an existing file first);
- `--github-env PATH`: when given, append `APPLE_SIGNING_IDENTITY=<CN>` on
  success and only then.

Steps, each through `run()`, each failing the script with a clear message on
a non-zero exit:

1. `kc_pw = secrets.token_urlsafe(24)`. A p12 taken from the environment is
   written to `<keychain dir>/signing.p12` (0o600).
2. `security create-keychain -p <kc_pw> <keychain>`
3. `security set-keychain-settings <keychain>` (no `-t`, no `-l`: the
   keychain never locks; the build takes about twenty minutes)
4. `security unlock-keychain -p <kc_pw> <keychain>`
5. `security import <p12> -k <keychain> -f pkcs12 -P <p12_pw> -T /usr/bin/codesign -T /usr/bin/security`
6. `security set-key-partition-list -S apple-tool:,apple:,codesign: -s -k <kc_pw> <keychain>`
7. `security list-keychains -d user` → parse the quoted paths (pure function
   `keychain_list(stdout) -> list[str]`: one path per line, whitespace and
   quotes trimmed, empty lines dropped) → `security list-keychains -d user -s <keychain> <existing...>`
8. Trust the certificate for code signing. First
   `sudo security add-trusted-cert -d -r trustRoot -p codeSign -k /Library/Keychains/System.keychain <PEM>`
   with `timeout=90` (the runner's sudo asks no password). If it fails or
   times out, print a `::warning::` and try the user domain:
   `security add-trusted-cert -r trustRoot -p codeSign -k <keychain> <PEM>`,
   `timeout=90`. If that fails too, the script fails.
9. The keychain must hold the expected certificate:
   `security find-certificate -c "<CN>" -a -p <keychain>` → every PEM block
   → SHA-1 of its DER; one must equal the SHA-1 of `--certificate`, else
   fail with "the p12 holds a different certificate than <PEM>".
10. The identity must be valid for code signing:
    `security find-identity -v -p codesigning <keychain>` → stdout must
    contain `"<CN>"` (pure function `identity_listed(stdout, cn) -> bool`).
    Otherwise run `security find-identity -p codesigning <keychain>` (no
    `-v`, so an invalid identity is still listed and a trust problem can be
    told from a missing key) and fail.
11. Print `identity=<CN>` and append to `--github-env` when given.

The CN comes from the certificate PEM:
`openssl x509 -in PEM -noout -subject -nameopt RFC2253` → pure function
`common_name(subject_line) -> str` that takes `subject=CN=SECS 2R (self-signed),O=SECS 2R contributors`
(and the same with a space after `subject=`), returns the CN up to the next
unescaped comma, unescaping `\,` and `\=`, and raises `ValueError` when
there is no `CN=`.

### Tests (`test_macos_codesign.py`)

`unittest`, no network, no `openssl`, no `security`: the lint job runs them
on Ubuntu. Cover:

- the openssl config text for a name and organization (every line above);
- `der_of_pem` and `fingerprints` against an embedded PEM fixture you make
  once with `openssl req -x509 ... -days 1` on this PC (any throwaway
  subject), asserting the SHA-1 and SHA-256 that `openssl x509 -fingerprint
  -sha1` / `-sha256` printed for it; and the `ValueError` for text without a
  CERTIFICATE block;
- `common_name` for both subject spellings, an escaped comma, and the error;
- `keychain_list` on real-looking `list-keychains` output (quoted,
  indented paths);
- `identity_listed` on a `find-identity` listing with and without the name;
- the command sequence of `prepare_keychain` with `run` patched
  (`patch.object(m, "run")` as the existing tests do): the order of steps
  2 to 11, the `sudo` trust attempt before the user-domain one, that the
  printed form of every command (what `run()` prints) contains no password,
  and that `APPLE_SIGNING_IDENTITY=` is appended only on success;
- `--p12-env` decoding and the empty-variable failure.

## Verification (foreground; do not end the turn before they finish)

```
python3 -m unittest discover -s .github/scripts -p "test_*.py" -v
python3 -I .github/scripts/macos_codesign.py make-certificate --name "SECS 2R (packet test)" --out-dir "<a new folder under %TEMP%>" --stem t --days 1
openssl x509 -in <folder>/t.pem -noout -text
openssl pkcs12 -in <folder>/t.p12 -passin file:<folder>/t.p12.password -nokeys -noout
python3 -I .github/scripts/macos_codesign.py fingerprint <folder>/t.pem
```

The `x509 -text` output must show the subject, the three extensions and
`Signature Algorithm: sha256WithRSAEncryption`; the `pkcs12` command must
read the file with no error; the `fingerprint` SHA-1 must equal
`openssl x509 -in t.pem -noout -fingerprint -sha1` (colons removed, lowercase).
Delete the folder afterwards.

## Report

The two files, the output of each verification command, and anything the
packet left open.
