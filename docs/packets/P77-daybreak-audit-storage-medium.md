# P77 (Daybreak Blue): audit #92 — storage and writer Medium findings, shared atomic write (L3)

Repository: `/home/user/Sound-Equalization-and-Correction-System-2R`. Work in
the main tree: `git switch -c wip/l3` from the current `HEAD`. Do not push,
do not switch branches again, do not spawn sub-agents. Commit once per
finding (subjects below); Claude reviews, re-runs the gates and re-authors
the commits.

Read first: `CLAUDE.md` (dependency rules), `docs/ARCHITECTURE.md` section 3,
`docs/DEVIATIONS.md` (F1, F5), `TECH_DEBT_AUDIT.md` (rows TD-42 to TD-47,
TD-74, TD-78, TD-88, TD-89), `crates/secs-project/src/{pyjson.rs,record.rs,project.rs,plan.rs,calibration.rs,irs.rs}`,
`crates/secs-camilla/src/lib.rs`, `crates/secs-export/src/lib.rs`,
`crates/secs-service/src/{project_edit.rs,session.rs,cache.rs}`,
`crates/secs-compute/src/lib.rs`.

A items: test first (red at the seam, confirm the reason, minimum green).
B items (refactor): no new tests; run the touched crates' full tests before
and after and report both counts — they must match, with zero new failures.
C items: compiler and linters are the gate.

## Findings, in order

1. **TD-43** (A) — `record.rs:408` `load_project_sources` joins raw paths
   and reads `*.src.json` without `confined_ir_path` (F5). Apply the same
   confinement to the IR path and to the sidecar path (symlinks included, as
   P74 did for writes). Seam: a project whose measurement points at
   `../outside.wav` with an `../outside.wav.src.json` → the sidecar is not
   read (the source is absent) and a migration warning or `None` follows the
   existing F5 behaviour. Extend F5's text. Commit:
   `fix(audit): A-41 출처 사이드카 읽기도 .irs 안으로 제한합니다 (#92 TD-43)`
2. **TD-44** (A) — `IrCache` (`record.rs:497-542`) keeps a single entry
   larger than its cap and evicts in insertion order (no LRU touch on hit).
   Fix: an entry larger than the cap is returned but not stored; a hit moves
   the key to most-recent. Seam: `IrCache` API: cap 8 bytes, insert 12 →
   retained bytes 0; insert A, B, get A, insert C → B evicted, A kept.
   Commit: `fix(audit): A-42 IR 캐시가 상한을 지키고 최근 사용 순서로 비웁니다 (#92 TD-44)`
3. **TD-45** (A) — `secs-camilla/src/lib.rs:572` checks FIR taps for f64
   finiteness, then `wav_bytes` (`:344`) narrows to f32; `1e100` becomes ∞.
   Validate the f32 value (and the expanded taps of low-rate FIRs) before
   writing; the Equalizer APO writer shares `wav_bytes`, so the check covers
   it. Seam: export with one tap `1e100` → error before any file is written,
   for CamillaDSP and Equalizer APO. Commit:
   `fix(audit): A-43 f32로 넘치는 FIR 계수를 쓰기 전에 거부합니다 (#92 TD-45)`
4. **TD-46** (A) — CamillaDSP chain validation (`lib.rs:552`) accepts
   duplicate roles (YAML duplicate keys, overwritten coefficients); the
   filter writer rejects them (`secs-filters/src/lib.rs:1867`). Reject
   duplicate main/sub roles and the reserved names the filter writer rejects,
   with the same error wording. Seam: chain with two mains named `L` → error
   before any file. Commit:
   `fix(audit): A-44 CamillaDSP 작성기도 중복 역할을 거부합니다 (#92 TD-46)`
5. **TD-47** (A, test debt) — `secs-project/tests/golden.rs:373` exercises
   the WAV readers only with the crate's own float32 writer. Add
   writer-independent minimal WAV byte fixtures (hand-built headers in the
   test) for PCM16, PCM24, PCM32, float32, float64, WAVE_FORMAT_EXTENSIBLE
   (PCM and float GUIDs), valid-bits < container bits, and rejections
   (truncated `fmt `, unknown GUID, 2 channels where mono is required, block
   align mismatch). Expected sample values are computed by hand in the test
   (e.g. `i16::MAX` → `32767.0 / 32768.0`), not by calling the reader. If a
   fixture exposes a reader bug, stop and report before fixing. Commit:
   `fix(audit): A-45 WAV 읽기를 작성기와 무관한 형식별 픽스처로 검사합니다 (#92 TD-47)`
6. **TD-42** (B) — `pyjson.rs:212` linear key lookup per inserted key (O(n²)).
   Keep the insertion-ordered vector and add a key → index map used only
   while parsing; semantics unchanged (duplicate keys: last value wins at the
   first key's position, as today). Commit:
   `refactor(audit): B-1 pyjson 객체 읽기를 키 색인으로 바꿉니다 (#92 TD-42)`
7. **TD-74** (B) — six copies of temp-file-then-rename:
   `secs-service/src/project_edit.rs:380`, `session.rs:305-342`,
   `cache.rs:167-231`, `secs-project/src/record.rs:1007`,
   `secs-camilla/src/lib.rs:1443` (pub), `secs-export/src/lib.rs:792-831`.
   Design (fixed): one helper in `secs-project` (the storage crate, which
   `secs-service` already depends on) as `secs_project::fs::write_atomic(path, |file| -> io::Result<()>)`
   plus `write_atomic_bytes(path, &[u8])`: sibling temp name
   `.{name}.tmp-{pid}-{counter}` with `create_new` retry, `sync_all`,
   `rename`, remove the temp on error. `secs-camilla` and `secs-export` do
   not depend on `secs-project` today: check `docs/ARCHITECTURE.md` §3 and
   the crates' manifests; if adding `secs-project` as a dependency of the
   writer crates would create a cycle or violate the documented direction,
   instead put the helper in `secs-types` under `secs_types::fs` (it already
   sits below every crate; it is std-only) and say which you chose and why.
   Replace all six; the cache's size-limited writer passes its own closure.
   Update ARCHITECTURE §3 if a dependency edge is added. Commit:
   `refactor(audit): B-2 원자적 파일 쓰기를 공용 도우미 하나로 모읍니다 (#92 TD-74)`
8. **TD-78** (B) — Python truthiness is implemented four times
   (`project.rs:88`, `plan.rs:366`, `calibration.rs:322`, `record.rs:981`).
   One `pub(crate) fn` on `PyJson` (e.g. `is_truthy`) used by all four;
   behaviour identical. Commit:
   `refactor(audit): B-4 파이썬 참거짓 판정을 한 곳으로 모읍니다 (#92 TD-78)`
9. **TD-89** (B) — unused `secs_camilla::Options.fsiso` (validated at
   `:522`, never used): remove the field and its validation, update call
   sites. `MemoryBudget::from_probe` (secs-compute) is unused while
   `secs-service/src/calibration.rs:304-312` recomputes the same value: use
   `from_probe` there (same numbers). Commit:
   `refactor(audit): B-7 쓰이지 않는 옵션을 지우고 예산 계산을 한 곳에서 합니다 (#92 TD-89)`
10. **TD-88** (C) — add to `crates/secs-dsp/Cargo.toml`:
    `[package.metadata.cargo-machete] ignored = ["pulp", "gemm", "faer-traits"]`
    with a one-line comment pointing at the workspace `Cargo.toml` note on
    faer's runtime SIMD detection. Gate: `cargo machete` reports nothing
    (`~/.cargo/bin/cargo-machete` is installed). Commit:
    `chore(audit): C-13 의도한 secs-dsp 의존성을 machete 무시 목록에 적습니다 (#92 TD-88)`
11. **TD-92** (A) — ADR 0004 (file compatibility: unknown keys are kept in
    order) is broken for nested objects: `store_calibration`
    (`calibration.rs:77`) replaces the whole `calibration` object, and the
    `SpeakerConfig`, `DspProfile` and `TargetCurve` conversions drop keys they
    do not know. First read how upstream saves these objects
    (`src/secs/project/` and `src/secs/wizard/`) and say in the report whether
    upstream keeps or drops nested unknown keys. Then, for every object the
    writer rebuilds from a typed value, merge with the object read from the
    file: known keys in the writer's order and with the writer's values,
    followed by the old object's unknown keys in their original order;
    recurse into nested objects the writer owns; arrays are replaced unless
    both sides are arrays of objects of the same length (then merge element
    by element). Keep the merge in one `pub(crate)` helper next to `PyJson`.
    Tests: a `.secs` whose `calibration`, a speaker entry, the DSP profile
    and a target curve each carry an unknown nested key keeps them, in
    order, after open → change a setting → save; every existing golden byte
    test stays unchanged. If upstream drops them, add the next free number
    in `docs/DEVIATIONS.md` (file-compatibility section). Commit:
    `fix(audit): A-83 중첩 객체의 모르는 키를 저장할 때 보존합니다 (#92 TD-92)`
12. **TD-93** (A) — `irs.rs:132` clamps a declared chunk end to the file
    length, so a truncated `data` chunk whose remainder is a multiple of the
    sample size reads as a shorter IR (upstream `session.py:84` does the
    same). For the `data` chunk only, reject `declared_end > data.len()` with
    `ProjectError::Wav` naming both lengths; other chunks keep today's
    handling. Test: a valid IR with its last 8 bytes cut → error; the intact
    file → unchanged samples. Add a deviation entry to `docs/DEVIATIONS.md`.
    Commit:
    `fix(audit): A-84 data 청크가 잘린 IR WAV를 거부합니다 (#92 TD-93)`
13. **TD-94** (A) — negative delays: the CamillaDSP writer emits a negative
    `Delay`, the Equalizer APO writer drops it (`secs-filters/src/lib.rs:941`),
    and chains revived from the cache are not validated. In both writers,
    reject a non-finite or negative delay before writing anything (an error
    of the writer's existing error type naming the channel). Also
    `plan_responses` (`:589`) computes FIR responses on a uniform grid built
    from `f.len()` rather than on `f`: document that precondition on the
    function and check it (return the existing error type when `f` is not
    that grid). Tests: a chain with `Delay { ms: -1.0 }` → both writers
    fail, no file written; a non-uniform `f` → error; existing goldens
    unchanged. Commit:
    `fix(audit): A-85 음수 지연을 두 작성기가 똑같이 거부합니다 (#92 TD-94)`

## Allowed files

`crates/secs-project/**`, `crates/secs-camilla/**`, `crates/secs-export/**`,
`crates/secs-filters/**`, `crates/secs-service/src/**`
(call-site replacement), `crates/secs-compute/**`, `crates/secs-types/src/**`
(only if the helper goes there; then `cargo test -p secs-types` and commit
any binding change), `crates/secs-dsp/Cargo.toml`, `docs/ARCHITECTURE.md` §3,
`docs/DEVIATIONS.md`, `features.toml`, `Cargo.lock`.

## Gates (foreground; do not end the turn before they finish)

After each finding: `cargo fmt --all -- --check`, clippy with `-D warnings`
on the touched crates (`--all-targets --locked`), `cargo test -p <touched crates> --locked`.
For B items record the test counts before and after. At the end
`cargo test --workspace --locked` once and `python3 .github/scripts/check_feature_tests.py`.

## Report (final answer)

Per finding: commit hash, files, test names (A), before/after counts (B),
red failure message (A). Then the last gate output tail and anything left
undone.
