# P88: audit #92 C13 — the mock as a contract double (ADR 0014), and band-output warning rows

Repository worktree: `/home/user/wt-ui`, branch `wip/c13` (already checked
out). Do not switch branches, do not push, do not spawn sub-agents. Commit
once per item below with the subject given; Claude reviews, re-runs the gates
and re-authors the commits.

Read first: `CLAUDE.md`, `docs/adr/0003-stack-and-ipc.md`,
`docs/adr/0014-generated-service-rules-for-the-screen.md` (the decision this
packet carries out), `docs/ARCHITECTURE.md` section 7, `CONTEXT.md`, the C13
card in `TECH_DEBT_AUDIT.md`'s companion issue (summary below),
`apps/secs2r/ui/src/ipc/{mock.ts,fixtures.ts,mock.test.ts,mockWiring.test.ts}`,
`apps/secs2r/ui/src/screens/calibrate/settingsView.ts`,
`apps/secs2r/ui/src/gallery/tour/scenes.ts`,
`crates/secs-service/src/settings_edit.rs`, `crates/secs-types/src/{lib.rs,role.rs,settings_edit.rs}`,
`crates/secs-project/src/plan.rs`, `crates/secs-service/src/{project_edit.rs,export_folder.rs}`,
`crates/secs-types/.cargo/config.toml` or the workspace `.cargo/config.toml`
(`TS_RS_EXPORT_DIR`).

C13 in one paragraph: `mock.ts` re-implements service rules (setting clamps,
mode specs, presets, export folder names, XO group keys, recent-project
count) and three copies had drifted from the service; screen tests pinned the
drifted values. ADR 0014 moves rule constants into `secs_types::rules`,
generates their values into `apps/secs2r/ui/src/ipc/bindings/rules.ts`, has a
service test record no-input responses into `apps/secs2r/ui/src/ipc/recorded/`,
and leaves the mock only the contract (envelope, job scheduler and progress
events, failure injection, undo, open-project state).

## Items, in order

1. **Rule constants and generated values** (A-78 part 1). Add
   `crates/secs-types/src/rules.rs` (`pub mod rules` in `lib.rs`) with:
   `CalibrationLimits` (one `(f64, f64)` field per clamp range that
   `crates/secs-service/src/settings_edit.rs` applies; field names are the
   snake_case of the keys of `LIMITS` in `settingsView.ts` where one exists,
   otherwise the setting's field name) and `pub const CALIBRATION_LIMITS`;
   `pub const SWEEP_LENGTHS: [&str; 7]` (moved from `secs-project/src/plan.rs`;
   `secs-project` keeps `pub use secs_types::rules::SWEEP_LENGTHS` so its API
   is unchanged); `pub const RECENT_PROJECTS_MAX: usize = 10`. Add any other
   constant you find copied by hand in `mock.ts`, `fixtures.ts` or a screen
   file whose Rust source is a plain constant, and list them in the report.
   Derive `ts_rs::TS` and `serde::Serialize` on the struct types. A test in
   `secs-types` writes `rules.ts` next to the other bindings (read the export
   directory the same way ts-rs does; check how in the ts-rs source under
   `~/.cargo/registry/src` before writing it), deterministic text:
   `import type { CalibrationLimits } from "./CalibrationLimits";` then
   `export const CALIBRATION_LIMITS: CalibrationLimits = {…} as const;` and
   one `export const` per other constant. CI's existing bindings diff then
   covers it. The service uses the constants instead of literals
   (`settings_edit.rs` clamps, `project_edit.rs` `truncate(10)` ×2); results
   bit-identical (the existing golden and service tests must pass untouched).
   Test first: a secs-types unit test asserting the generated text for a
   small constant, and a service test that a clamp at each limit edge equals
   the constant. Run `cargo test -p secs-types` and commit the bindings.
   Commit: `refactor(audit): A-78 서비스 규칙 상수를 secs-types에 두고 화면용 값을 생성합니다 (#92 C13)`

2. **Recorded no-input responses** (A-78 part 2). Add
   `crates/secs-service/tests/recorded.rs`: build a service the way the
   existing integration tests do (find the helper in `crates/secs-service/tests/`),
   and for each case call `Service::call` and compare the JSON `data` with
   `apps/secs2r/ui/src/ipc/recorded/<name>.json` (path from
   `env!("CARGO_MANIFEST_DIR")`). Mismatch fails with a message naming the
   file and `SECS_RECORD=1`; with `SECS_RECORD=1` the test writes the files
   (pretty JSON, trailing newline) and passes. Cases: `mode_spec` for every
   mode (default ultra ms; `mimo_fir` false and true where the spec differs),
   `config_apply_preset` for every preset on a freshly created project,
   `target_apply_preset` for every preset, `calibration_settings_get` on a
   new project. Responses that contain a machine path or a time are not
   recorded, or are recorded with that field replaced by a fixed token the
   mock substitutes (say which). Commit:
   `test(audit): A-78 입력 없는 서비스 응답을 화면용으로 기록합니다 (#92 C13)`

3. **Mock and fixtures on the generated values** (A-78 part 3, deepening:
   tests at the mock's IPC interface first). In `mock.ts`, `fixtures.ts` and
   `settingsView.ts`: replace every hand copy covered by items 1–2 with
   imports from `ipc/bindings/rules` and `ipc/recorded/*.json` (Vite and
   Vitest import JSON; check `tsconfig` `resolveJsonModule`). `LIMITS` in
   `settingsView.ts` becomes the generated table; update its readers
   (call sites only; no markup or style change). Fix the drift the audit
   found and say for each whether it still existed: export folder names
   (`mock.ts` export case vs `crates/secs-service/src/export_folder.rs`), XO
   group keys (`fixtures.ts` vs `crates/secs-types/src/role.rs`), recent
   project count (UI vs `RECENT_PROJECTS_MAX`). Then delete the mock tests
   whose only job was to pin a hand copy, and add one table test that the
   mock returns each recorded response unchanged. Report the UI test count
   before and after. Commit:
   `refactor(audit): A-78 모의 서비스가 계약만 맡고 규칙은 생성 값을 씁니다 (#92 C13)`

4. **Tour reopen is not dirty** (A-78 note). In the tour scene
   `export.stale-cache` (`gallery/tour/scenes.ts`) a reopened project shows
   unsaved changes immediately. Find why (mock `project_save`/`project_open`
   snapshot, or a scene edit after reopening) with a failing test at the
   mock or scene level, then fix the cause. Commit:
   `fix(audit): A-78 견본 투어에서 다시 연 프로젝트가 저장된 상태로 열립니다 (#92 C13)`

5. **Band-output warning rows** (UI follow-up of TD-31; screen work). The
   storage warning `config.band.out.outside` has entries
   `role {role}, band {band}, output {original}` (`crates/secs-project/src/config.rs:154`).
   Today `app/projectWarnings.ts` shows them as raw English lines. Parse them
   into a row `{ role, band, output }` (role through `roleCode` like the
   position rows), render each as one localized line, and give the group one
   action that opens the System screen's output patch, wired the same way
   `onRemeasure` is wired from `App.tsx` through `AppFrame` to
   `ProjectStatus` (find how the System screen is opened elsewhere and reuse
   that). New keys (add to both `ko.json` and `en.json`):
   `frame.status.warnings.band_output` — ko `{role} {band} 대역: 원래 출력 {output}`,
   en `{role} {band} band: was output {output}`;
   `frame.status.warnings.open_patch` — ko `출력 배선 열기`, en `Open output patch`.
   Keep the existing component structure and classes; follow the
   make-interfaces-feel-better rules in `CLAUDE.md` (14 px floor, noun-phrase
   labels). Tests in `AppFrame.test.tsx`: the rows render in both locales and
   the action calls its handler. Commit:
   `fix(audit): A-31 출력 번호 경고를 역할·대역·출력 줄로 보이고 출력 배선으로 이동합니다 (#92 TD-31)`

## Allowed files

`crates/secs-types/**` and the regenerated `apps/secs2r/ui/src/ipc/bindings/**`,
`crates/secs-project/src/{plan.rs,lib.rs}` (the re-export only),
`crates/secs-service/src/{settings_edit.rs,project_edit.rs}` (constants only),
`crates/secs-service/tests/recorded.rs` and `apps/secs2r/ui/src/ipc/recorded/**`,
`apps/secs2r/ui/src/ipc/**`, `apps/secs2r/ui/src/screens/calibrate/{settingsView.ts,Settings.tsx}`
(call sites of `LIMITS` only), `apps/secs2r/ui/src/gallery/**` (item 4),
`apps/secs2r/ui/src/app/{projectWarnings.ts,ProjectStatus.tsx,AppFrame.tsx,AppFrame.test.tsx}`,
`apps/secs2r/ui/src/App.tsx` (wiring for item 5), `apps/secs2r/ui/src/i18n/{ko,en}.json`,
any `*.test.ts(x)` beside a file you change, `Cargo.lock`.
Do not change CSS files. Other agents are working in
`/home/user/Sound-Equalization-and-Correction-System-2R`, `/home/user/wt-rust2`
and `/home/user/wt-l3`; never touch those trees.

## Gates (foreground; do not end the turn before they finish)

Export `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=line-tables-only` for
cargo. UI commands run with Node 24: `export PATH=/opt/node24/bin:$PATH`.
After each item: `cargo fmt --all -- --check`;
`cargo clippy -p secs-types -p secs-project -p secs-service --all-targets --locked -- -D warnings`;
`cargo test -p secs-types -p secs-service --locked` (items 1–2; also
`-p secs-project` for item 1); for UI items
`(cd apps/secs2r/ui && npm ci --no-audit --no-fund && npm run typecheck && npm test && npm run build)`
(`npm ci` once is enough unless `package-lock.json` changes). At the end
`git status --short apps/secs2r/ui/src/ipc/bindings` must be clean after a
fresh `cargo test -p secs-types`, and `python3 .github/scripts/check_feature_tests.py`
must pass. Check `df -h /home/user` before the service tests; if under 5 GB
is free, report it and stop rather than deleting anything outside
`/home/user/wt-ui`.

## Report (final answer)

Per item: commit hash, files, test names, the red failure (A items) and the
green result; for item 3 the UI test counts before/after and which drifts
still existed; for item 2 the list of recorded files and how paths/times were
handled. Then the tail of the last gate output and anything left undone.
