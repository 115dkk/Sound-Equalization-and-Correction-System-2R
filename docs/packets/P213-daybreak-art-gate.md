# P213 (Daybreak): ART gate runner, `art_gate.py`

Worktree `E:\wt\art-gate`, branch `claude/art-gate`. No Rust build is needed
and none may be started. ASTRA works in the same worktree on
`.github/scripts/art_metrics.py` and `test_art_metrics.py` (P212); do not
edit those (you may read `docs/packets/P212-astra-art-metrics.md` for the
interface). Your files, and the only ones you may create or edit:

- `.github/scripts/art_gate.py`
- `.github/scripts/test_art_gate.py`

Do not commit, do not push. Run every command in the foreground and end your
turn only after the report. Python 3.13. Third-party imports allowed:
`cryptography` (50.0.2 is installed) in `art_gate.py` only, imported lazily
inside the functions that encrypt or decrypt, so the module imports without
it. Read `docs/adr/0026-art-reference-gate.md` first: the '보완' section at
its end is the contract this packet implements.

**Privacy rule (ADR 0026).** The real measurements are private and are not in
this worktree. Work only with synthetic data your tests generate. Do not
look for measurement files elsewhere on this machine. The runner must never
print a metric value, a CLI output line, an exception message from the CLI
or from the metrics module, or an absolute path inside the work folder.

## Commands

```text
art_gate.py run    --manifest reference/art/cases.toml --cli PATH
                   [--key-env ART_REFERENCE_KEY] [--require-key] [--case NAME ...]
                   [--summary FILE] [--values-out FILE]
art_gate.py pack   --case-dir DIR --name NAME --out FILE [--key-env ART_REFERENCE_KEY]
art_gate.py verify --manifest reference/art/cases.toml
art_gate.py keygen --out FILE
```

Exit codes: 0 done (whatever the verdicts), 1 the gate could not compute
some case, 2 usage or configuration error (bad manifest, bad key, bad case
name). `run` without a key (variable unset or empty): print
`ART reference: key not set, skipped` and exit 0; with `--require-key`
exit 2 instead.

## Manifest (`reference/art/cases.toml`, read with `tomllib`)

```toml
schema = 1

[[case]]
name = "lab-4.2"                 # ^[a-z0-9][a-z0-9._-]{0,63}$
file = "lab-4.2.art"             # plain file name next to the manifest
sha256 = "<64 hex of the ciphertext file>"
kind = "art"                     # "art" or "input"
groups = ["arrival", "gd_excess", "pre", "decay"]

[case.variants]                  # name -> arguments for `secs2r-cli project set`
base = []
lf_align = ["--lf-align", "on"]
```

Validation (`load_manifest(path) -> list[Case]`, error text names the case
and the field, exit 2): `schema == 1`; unique names matching the regex;
`file` a plain name ending in `.art`; `sha256` 64 lowercase hex; `kind` one
of the two; `groups` non-empty, each in `art_metrics`'s group names
(`arrival`, `gd_excess`, `pre`, `decay`: keep your own tuple
`KNOWN_GROUPS` so this module validates without importing numpy); at least
one variant; variant names `^[a-z0-9][a-z0-9_+.-]{0,63}$`; variant
arguments come in flag/value pairs where the flag is one of `--mimo`,
`--mimo-supports`, `--mimo-early`, `--mimo-seat-guard`, `--lf-align`,
`--mode-decay`, `--low-protect`, `--hf-phase`, `--dip-limit`, `--input-hp`,
`--rc-bands` and the value matches `^[A-Za-z0-9.,-]{1,64}$`. An empty
manifest (no case) is valid.

`verify`: for every case the file exists next to the manifest and its
SHA-256 equals the manifest's. Needs no key and no third-party import.
Prints one line per case (`<name>: ok` or the mismatch) and exits 0 or 1.

## Bundle format

Ciphertext file: `b"SECSART1"` + 12-byte random nonce + AES-256-GCM
ciphertext with tag. Associated data: `b"SECSART1" + name.encode()`.
Key: the environment variable holds base64 (standard alphabet) of exactly
32 bytes; anything else is a configuration error that does not echo the
value. `keygen --out FILE` writes such a key (one line, no newline issues:
strip on read) with `os.open(..., 0o600)`, refuses to overwrite, prints only
the file name.

Plaintext: a tar.xz of the case folder, built in memory by `pack`
deterministically (members sorted by path, mtime 0, uid/gid 0, empty
uname/gname, mode 0644, regular files only; directories implied). `pack`
requires `case.json` at the root with `schema == 1`, a `project` path
ending in `.secs` that exists, a non-empty `inputs` list of strings,
`eval.raw` a non-empty object whose entries each name an existing `wav`
inside the folder with a numeric `t0`, the same for `art` when present
(`pack --name` cases need not say the kind; the manifest does). Every
referenced path must be relative, without `..`, inside the folder. `pack`
writes `--out`, then prints exactly two lines: `sha256=<hex>` and
`bytes=<n>`. It does not edit the manifest.

`open_bundle(data: bytes, name: str, key: bytes, dest: Path) -> None`
decrypts and extracts: reject anything that is not a regular file or a
directory, absolute names, `..` components, more than 20000 members, more
than 2 GiB in total; extract with `tarfile`'s `data` filter as well.
A wrong key, a wrong name or a damaged file is one error:
`BundleError("cannot decrypt")`.

## `run`

For each selected case, in manifest order:

1. Read the file, check its SHA-256 against the manifest (mismatch: the
   case fails with stage `hash`).
2. Decrypt and extract into a fresh folder under a
   `tempfile.TemporaryDirectory(dir=os.environ.get("RUNNER_TEMP"))` that is
   removed when `run` ends, also on failure. Read `case.json`.
3. For each variant: copy the bundle's project folder (the folder that
   holds `case["project"]`) to a fresh variant folder, then run with
   `cwd` set to that copy's parent, paths relative, output captured and
   never printed, 900 s timeout each:
   - when the variant has arguments:
     `<cli> --quiet --compute cpu --deterministic-uuids project set <secs> <args...>`
   - `<cli> --quiet --compute cpu --deterministic-uuids calibrate <secs>`,
     with `--mimo` after `calibrate` when the project file (JSON) has
     `calibration.mimo_on == true` after the previous step (a fresh project
     has no `calibration` key: that is MIMO off)
   - `<cli> --quiet --compute cpu --deterministic-uuids export --kind convolution --out <variant>/export <secs>`

   A step fails when the exit code is not 0, when it times out, or when any
   stdout line parses as JSON with `reply.ok == false`. Stages are named
   `set`, `calibrate`, `export`.
4. `score = scorer(case_dir, export_dir, case_json)`; the default scorer is
   `art_metrics.score_case`, imported lazily so that `verify`, `pack` and
   the manifest tests work without numpy. An exception from it fails the
   variant with stage `score`.

After all cases are scored:

- `art` cases: `verdicts = art_metrics.judge(score["predicted"], score["art"], groups)`
  plus `art_metrics.judge_phase(score["predicted_phase"], score["art_phase"])`.
  An `art` case whose bundle has no `art` key fails with stage `case`.
- `pooled = art_metrics.pool([score["art"] ...])` over the `art` cases'
  `base`-independent data: the ART side is the same for every variant, so
  take it from each art case's first scored variant.
- `input` cases: `art_metrics.judge_pooled(score["predicted"], pooled, groups that are in art_metrics.CROSS_ROOM_GROUPS)`.
  With no `art` case scored, every verdict is `"unknown"`.
- `grade = art_metrics.grade(verdicts)`.

The functions that judge are reached through one injectable object so the
tests can replace scoring and judging without numpy:
`run_gate(cases, manifest_dir, cli, key, metrics=None, out=sys.stdout) -> GateResult`
where `metrics` defaults to the lazily imported `art_metrics` module and
only its `score_case`, `judge`, `judge_phase`, `pool`, `judge_pooled`,
`grade` and `CROSS_ROOM_GROUPS` are used.

### Output

Words (exactly these): verdicts `비슷함` (similar), `아님` (different),
`판정 불가` (unknown); grades `가까움` (close), `보통` (fair), `멂` (far),
`없음` (none). Kind labels: `ART 실측` for `art`, `다른 방의 ART 기준` for
`input`.

Stdout, one line each, nothing else about a case:

```text
case=<name> variant=<variant> metric=<metric> verdict=<word>
case=<name> variant=<variant> grade=<word> similar=<n> different=<n> unknown=<n>
case=<name> variant=<variant> failed stage=<stage>
```

`--summary FILE` appends Markdown: a heading `## ART reference (similarity)`,
one sentence saying the verdicts are reference information and no values
are shown (ADR 0026), a table
`| 사례 | 기준 | 설정 | 등급 | 비슷함 | 아님 | 판정 불가 |` with one row per
case and variant (a failed variant shows `계산 실패 (<stage>)` in the grade
column), then per case a table `| 지표 | <variant> | <variant> ... |` with
the verdict words. Metric names are the keys as they are (`arrival_80`,
`phase:L/R`).

`--values-out FILE` writes JSON
`{case: {variant: {"predicted": ..., "predicted_phase": ..., "art": ..., "art_phase": ..., "verdicts": ..., "grade": ...}}}`
plus `"pooled"` at the top level, NaN written as `null`. Refuse it (exit 2,
before doing anything) when `GITHUB_ACTIONS` is `true` or `CI` is set.

`run` returns 1 when any variant failed, after finishing the other cases.

## Tests (`test_art_gate.py`)

Follow the neighbours (`test_integrity.py`). Tests that need `cryptography`
skip themselves when it is missing (`unittest.skipUnless`); the manifest,
`verify`, tar-safety and output tests must run with the standard library
only, because the lint job has neither numpy nor cryptography.

1. Manifest: a valid file loads; each rule above rejected with the case
   and field named (bad name, duplicate, bad sha, unknown kind, unknown
   group, no variant, a flag outside the list, an odd number of arguments,
   a value with a space or a slash); an empty manifest is valid.
2. The committed manifest: `load_manifest` on
   `reference/art/cases.toml` of this repository (skip when the file does
   not exist yet) and `verify` on it pass. This is the check that keeps the
   committed ciphertexts and the manifest in step.
3. Crypto: pack then open round trips a small folder byte for byte; a
   wrong key, a wrong name and a flipped byte each raise
   `BundleError("cannot decrypt")`; two packs of the same folder differ
   (random nonce) while their plaintext tars are identical; a key that is
   not 32 bytes is a configuration error whose text does not contain the
   key.
4. Tar safety: hand-built plaintexts with an absolute name, a `..`
   component, a symlink and a hard link are rejected.
5. `pack` validation: a missing `case.json`, a missing wav, a path with
   `..` and a missing `eval.raw` are rejected.
6. `run` with a fake CLI and fake metrics. The fake CLI is a small Python
   script the test writes and passes as `--cli` through a wrapper
   (`sys.executable script.py` on every OS: accept `--cli` as a single
   path and add an optional repeated `--cli-arg` for leading arguments, or
   write a `.cmd`/shell shim; choose one and say which). It records its
   argument lists to a file, writes an empty export folder on `export`,
   and can be told through an environment variable to fail a given stage
   or to print `{"reply":{"ok":false}}`. Check: the argument lists and
   their order for a variant with and without arguments; `--mimo` present
   only when the copied project has `mimo_on` true; an `art` case uses
   `judge` and `judge_phase`, an `input` case uses `judge_pooled` with
   only the cross-room groups; a failing stage gives the `failed stage=`
   line, exit code 1, and the other case still runs; a hash mismatch fails
   with stage `hash` before decrypting; the work folder is gone afterwards.
7. Redaction: the fake metrics return distinctive numbers (for example
   123.456789) and the fake CLI prints a distinctive token on stdout and
   stderr; neither appears in stdout, stderr or the summary, and no
   temporary path does.
8. `--values-out` writes the numbers, and is refused when `GITHUB_ACTIONS`
   is `true`.
9. No key: `run` prints the skip line and exits 0; with `--require-key`
   it exits 2.
10. End to end with the real CLI, only when the environment variable
    `SECS2R_CLI` names an executable and numpy, cryptography and
    `art_metrics` import (skip otherwise): in a temp folder run
    `<cli> --quiet --deterministic-uuids project new --preset 2.0 case.secs`
    and `<cli> --quiet --deterministic-uuids sample case.secs --positions 2`
    (Claude tried both on 0.56.1: the project then has speakers L and R,
    two positions, `case.irs/L_P1.wav` and so on, `t0` 0.0, and no
    `calibration` key until a setting is changed; `calibrate` and
    `export --kind convolution` then write `export/routing.json` with
    inputs and outputs `["L", "R"]`),
    read the project JSON to find position 1's IR files and `t0` values
    (`measurements[0]["ir"]` and `["t0"]`, keyed by role), write a
    `case.json` whose `eval.raw` points at those files, `inputs` `["L", "R"]`,
    no `art`; pack it as an `input` case with a generated key, write a
    manifest with variants `base = []` and
    `lf_align = ["--lf-align", "on"]`, run the gate and check exit code 0,
    one grade line per variant and verdict lines for the cross-room groups
    (all `판정 불가`, since there is no ART case). Then build a second case
    from the same folder with `art` set to the same position-1 files of L
    and R (a made-up 'ART measurement') and kind `art`, and check that the
    run prints verdict lines for the four groups and exits 0. If a step
    does not behave as described in your try, report what it printed and
    leave the test skipped with a clear reason rather than guessing other
    commands. The CLI for your own
    trial is `E:\Sound-Equalization-and-Correction-System-2R\target\release\secs2r-cli.exe`
    (read-only: run it, never rebuild it); `--help` on any subcommand shows
    its options. ASTRA's module may not be finished when you get here: in
    that case write the test, state that it could not be run, and Claude
    runs it.

## Verification (foreground)

```sh
cd /e/wt/art-gate
python -m unittest discover -s .github/scripts -p "test_art_gate.py" -v
SECS2R_CLI=/e/Sound-Equalization-and-Correction-System-2R/target/release/secs2r-cli.exe python -m unittest discover -s .github/scripts -p "test_art_gate.py" -v
python -m unittest discover -s .github/scripts -p "test_*.py" 2>&1 | tail -5
python -m py_compile .github/scripts/art_gate.py
git status --short
```

## Report

Files changed with line counts; the full output of the first two commands;
the last lines of the third; every place where you had to decide something
this packet left open (state the decision and where it is in the code);
known limits.
