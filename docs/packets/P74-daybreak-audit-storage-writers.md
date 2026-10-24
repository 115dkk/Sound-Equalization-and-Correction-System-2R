# P74 (Daybreak Blue): audit #92 — project storage, writers, oracle helpers (L1)

Repository: `/home/user/Sound-Equalization-and-Correction-System-2R`. Work in
the main tree: `git switch -c wip/l1` from the current `HEAD`. Do not push, do
not switch branches again, do not spawn sub-agents. Commit once per finding
(subjects below); Claude reviews, re-runs the gates and re-authors the
commits.

Read first: `CLAUDE.md`, `CONTEXT.md`, `docs/adr/0002-unsafe-isolation-and-policy-gates.md`,
`docs/adr/0004-parity-and-deviations.md`, `docs/DEVIATIONS.md` (F1, F3, F5,
F7), `TECH_DEBT_AUDIT.md` (rows TD-15, TD-16, TD-17, TD-19, TD-20, TD-22,
TD-25, TD-31, TD-55), `crates/secs-project/src/{irs.rs,record.rs,config.rs,project.rs}`,
`crates/secs-project/tests/{record.rs,golden.rs}`, `crates/secs-camilla/src/lib.rs`
and `tests/golden.rs`, `crates/secs-filters/src/lib.rs` (export functions),
`crates/secs-oracle/src/lib.rs`.

Method: test first for every finding. Red at the named seam (confirm the
failure is the finding's), minimum green, crate tests. Expected values come
from the scenario. Every behaviour that now differs from upstream SECS 2 (a
new bound, a rejected input) gets a numbered entry in `docs/DEVIATIONS.md`
(next free F number, with the test name), per ADR 0004.

## Findings, in order

### 1. TD-15 — IR WAV writes are atomic

`irs.rs:261` overwrites an existing WAV with `fs::write`. Use a sibling
temporary file, `sync_all`, then `rename` (same pattern as
`record.rs::atomic_write`; call that helper — make it `pub(crate)` if needed —
rather than adding a seventh copy). Seam: `save_ir` with an existing WAV and
a failure injected after the temporary write (e.g. destination made a
non-empty directory so the rename fails) → the old bytes are intact and no
temporary file remains.

Commit: `fix(audit): A-15 IR WAV를 원자적으로 씁니다 (#92 TD-15)`

### 2. TD-16 — IR writes stay inside the project's `.irs`

Reads are confined (F5, `confined_ir_path`); writes are not. Before writing,
reject (a) an `.irs` directory that is a symlink, (b) any symlink component
between the project folder and the file, (c) a destination file that is a
symlink — check with `symlink_metadata`; return a `ProjectError` with a clear
message. The atomic rename of finding 1 must not follow a link. Unix-only
tests (`#[cfg(unix)]`, `std::os::unix::fs::symlink`): `MeasurementStore::record`
into a project whose `.irs` is a link to an outside dir, and whose
`L_P1.wav` is a link to an outside file → error, the outside targets are
unchanged. DEVIATIONS: extend F5's text to cover writes.

Commit: `fix(audit): A-16 IR 쓰기를 프로젝트의 .irs 안으로 제한합니다 (#92 TD-16)`

### 3. TD-19 — bounded `.secs` and WAV reads

`record.rs:91` reads a project with unbounded `read_to_string`; WAV reads
(`record.rs:500`, `irs.rs` readers) read the whole file and convert without
a bound. Add `pub const MAX_PROJECT_BYTES: u64 = 64 * 1024 * 1024` and
`pub const MAX_IR_BYTES: u64 = 256 * 1024 * 1024` (same envelope as mdat
import) in `secs-project`; read with `take(limit + 1)` and fail with a
`ProjectError` naming the file and the limit; check the WAV data chunk's
declared length against the limit before allocating. Tests: limit+1 bytes →
error for both; a normal project and IR unchanged.

Commit: `fix(audit): A-19 .secs와 WAV 읽기에 상한을 둡니다 (#92 TD-19)`

### 4. TD-20 — a malformed `spl_offset` does not panic

`record.rs:973`: an existing measurement row with `"spl_offset": null` (or a
non-object) panics in `setdefault_nested`'s `expect` when a result with an
SPL offset is recorded — after the WAV and sidecar were already written.
Fix: repair it like the other maps in `repair_entry` (replace a non-object
with an empty object) before any file is written. Seam:
`MeasurementStore::record` on a project whose row has `"spl_offset": null` →
success, the offset stored, upstream bytes for a normal row unchanged
(existing golden tests).

Commit: `fix(audit): A-20 잘못된 spl_offset 행에서 패닉하지 않습니다 (#92 TD-20)`

### 5. TD-31 (storage part) — output numbers have a bound

The UI builds one option per output number up to the largest `out` in the
project (`SystemScreen.tsx:344`), and `config.rs:216-219` accepts any
integer. Check upstream (`src/secs/model.py`, speaker/output handling) for an
existing bound; if none, reject `out` outside `1..=256` when loading a
project (keep the project loadable: drop the offending band's output to the
default and add a migration warning, the same channel F5 uses for rejected IR
paths — find the warning type in `project.rs`). DEVIATIONS entry. Seam:
`Project::from_json` / project open with `out: 1000000000` → warning, output
reset, no panic; normal projects byte-identical (goldens).

Commit: `fix(audit): A-31 출력 번호에 상한을 두고 범위 밖은 경고로 바꿉니다 (#92 TD-31)`

### 6. TD-17 — CamillaDSP identifiers cannot leave the export folder

`secs-camilla/src/lib.rs:415` `check_ascii_identifier` allows `/`, `\`,
`..`; roles become FIR file names (`:823`, writes `:1515-1517`). A crafted
project cache can make an export write `*_fir.wav` outside the folder.
Fix: identifiers used in file names (roles, `fir_prefix`) must match
`[A-Za-z0-9_-]+` (check what the oracle goldens use — role names like
`SW1`, `Tfl`, sanitized ASCII names — and keep every golden passing); and
before writing, assert every coefficient path's parent is the coefficient
directory. Tests: a chain whose role is `../x` → error before any file is
written; nothing outside the folder.

Commit: `fix(audit): A-17 CamillaDSP 식별자가 내보내기 폴더를 벗어나지 못하게 합니다 (#92 TD-17)`

### 7. TD-22 — re-exporting into a folder is all-or-nothing

Re-exporting into an existing folder replaces coefficient files one by one
and the YAML (CamillaDSP, `lib.rs:1513`) or `config.txt` (Equalizer APO,
`secs-filters/src/lib.rs:1724`) last; a failure in between leaves the old
entry file pointing at a mix of new and old coefficients. File names must
stay as they are (oracle goldens). Design (fixed): write the complete bundle
into a sibling staging directory first (`.{folder}.stage-{pid}-{n}`, same
relative layout); only when every file is written and synced, move each file
into place with `rename` (entry file last), then remove the staging
directory; on any write failure remove the staging directory and leave the
folder untouched. Apply to CamillaDSP, Equalizer APO and filter text
(`:1686`). Tests (each writer): pre-populate a folder with an earlier export,
inject a failure on the second coefficient (e.g. a directory where that file
must go inside the staging area, or a writer hook used only by tests) → the
folder's bytes equal the earlier export; a normal re-export equals a fresh
export byte for byte.

Commit: `fix(audit): A-22 같은 폴더로 다시 내보내기를 전부 아니면 전무로 바꿉니다 (#92 TD-22)`

### 8. TD-25 — `assert_close` rejects infinities that do not match

`secs-oracle/src/lib.rs:406`: with `e = +∞`, `g` finite and `rtol > 0`, both
sides are ∞ and the check passes; opposite infinities pass too. Fix: if
either value is non-finite, require `same` (identical, or both NaN); apply the
tolerance only when both are finite. Unit tests in `secs-oracle`: (1.0, +∞),
(+∞, −∞), (NaN, 1.0) must panic (`#[should_panic]`), (+∞, +∞) and
(NaN, NaN) pass. Then run the workspace tests that use the oracle — if any
golden comparison now fails, stop and report it (it would be a hidden
regression).

Commit: `fix(audit): A-25 오라클 비교가 맞지 않는 무한대를 통과시키지 않습니다 (#92 TD-25)`

### 9. TD-55 — the fixture loader reads chunked arrays with an inline tail

`oracle/common.py:156` splits arrays over `CHUNK` into a list whose last
piece is inline when it is short; `Fixture::f64s` (`secs-oracle/src/lib.rs:147-151`)
assumes every piece is a descriptor. Fix the loader: each list element is
either a descriptor object or an inline array. Unit test with a hand-built
fixture JSON (descriptor + inline tail) in a temp dir.

Commit: `fix(audit): A-52 골든 로더가 인라인 꼬리 조각을 읽습니다 (#92 TD-55)`

## Allowed files

`crates/secs-project/**`, `crates/secs-camilla/**`, `crates/secs-filters/**`,
`crates/secs-oracle/**`, `docs/DEVIATIONS.md`, `features.toml` (point entries
at new tests where they name the behaviour), `Cargo.lock` as cargo changes
it. If finding 5 needs a new migration-warning code that the UI must
translate, add the key to `apps/secs2r/ui/src/i18n/{ko,en}.json` only.

## Gates (foreground; do not end the turn before they finish)

After each finding:

```sh
cargo fmt --all -- --check
cargo clippy -p secs-project -p secs-camilla -p secs-filters -p secs-oracle --all-targets --locked -- -D warnings
cargo test -p <crates you touched> --locked
```

After finding 8 run `cargo test --workspace --locked` once (oracle users).
At the end: `python3 .github/scripts/check_feature_tests.py`, and because
`secs-project` reads untrusted files:
`cargo +nightly-2026-10-01 miri test --locked -p secs-jos -p secs-mdat -p secs-dirac`
only if that toolchain is installed (`rustup toolchain list`); otherwise say so.

## Report (final answer)

Per finding: commit hash, files, test names, red failure message, green
result, DEVIATIONS entries added. Then the last gate output tail and anything
left undone.
