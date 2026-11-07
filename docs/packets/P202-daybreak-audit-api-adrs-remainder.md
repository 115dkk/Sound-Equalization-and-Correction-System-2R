# P202 (Daybreak): audit #186, the API ADR steps P190 could not finish

Worktree `E:\wt\audit-api`, branch `audit/186-api-adr-2` (Claude creates it
from the then-current `fix/audit-186` and pops the stash that holds P190's
ignored Red tests for X-13 step 2 and the local API step 6 before handing you
this packet; `git status` then shows those uncommitted files and nothing
else). Every cargo command and every script that starts cargo runs with
`CARGO_TARGET_DIR=C:\cargo-target\audit-api`; after the first command check
that `E:\wt\audit-api\target` does not exist. No workspace-wide `cargo test`
or `cargo clippy`; per-package (`-p`) only. One commit per step, titles as
P190 gives them, trailer `Co-Authored-By: Claude Mythos 5.1 <noreply@anthropic.com>`.
Do not push, do not rebase, do not touch `audit-state.md` or ADRs.

## Why

P190 finished X-13 step 1 and X-18 steps 4 and 5. It stopped at the steps
whose edits fell outside its file list: the root `Cargo.lock` (every
dependency change touches it), and the service files another worker owned
at the time (`crates/secs-service/src/{calibration.rs,results.rs,curves.rs,jobs.rs,settings_edit.rs}`).
Both are yours now: P189 (service ADRs) and P201 (engine) have landed on the
base. Read `docs/packets/P190-daybreak-audit-api-adrs.md` for the step
texts; this packet only changes the order, the file list and what the base
already has.

## Steps, in this order

1. **X-13 step 2** (P190 "### X-13", step 2): `EngineBuilder::from_deps`, the
   CLI uses `secs-api` (`apps/secs2r-cli/Cargo.toml` gains `secs-api`,
   `runner.rs` drops its Condvar `Events` and direct `Service` use). The
   ignored Red test from the stash is the starting point; un-ignore it.
   `Cargo.lock` changes are part of the commit.
2. **X-12 step 3 + A-4** (P190 "### X-12", step 3): `secs-sys` depends on
   `secs-service` (ADR 0053; `crates/secs-policy/tests/gates.rs` gets the one
   allow-list line), `native_deps(offset)` in `crates/secs-sys/src/embed.rs`,
   the shared clock in `crates/secs-sys/src/clock.rs`, `HostPlatform`
   narrowing in `apps/secs2r-host-shell/src/main.rs`, the two `apply.rs`
   copies deleted. A-4 (TD-152): the CLI's `clock_offset_seconds` reaches
   `LocalTime` so `created_utc` of an export equals UTC under a +9 h clock;
   write that Red test against today's `runner.rs` (the P184 draft is at
   `C:\Users\32170336\AppData\Local\Temp\claude\8a2ed516e7e71310\9cfb4797-a42c-54d9-a68d-f128d938515e\scratchpad\a4-red-test.patch`
   for reference only). Commit A-4 separately:
   `fix(audit): A-4 시계 오프셋이 서비스에 전달되어 내보내기 시각이 UTC가 됩니다 (#186 TD-152)`.
3. **X-1 steps 6 and 7** (P190 "### X-1"): `CalibrationSummary.notes` and
   `MimoSummary.notes` become `Vec<secs_types::notes::Note>` on the wire; the
   service builds them from the engine's typed notes (`calibration.rs`,
   `results.rs`, `curves.rs`), `cargo test -p secs-types` regenerates the
   bindings (commit the content changes), the Python `.pyi` and the .NET
   types follow, and the API documentation says typed. The screen's
   consumers break at `npm run typecheck`: fix only the call sites that read
   `notes` (`screens/calibrate/text.ts`'s `noticeText`, fixtures, mock) to the
   typed shape, keep markup and styles, and list the files; P192 prepared
   `noteRuns`/`noticeText` for per-item records, so most of the parsing is
   already gone.
4. **X-11 steps 8 and 9** (P190 "### X-11"): the generated rules table
   `apps/secs2r/ui/src/ipc/bindings/rules.ts` from `secs-types` (the service's
   calibration key functions, `jobs.rs` and `settings_edit.rs` read the same
   constants), and the recorded answers under `apps/secs2r/ui/src/ipc/recorded/`
   (`calibration_settings_edit.json`, `results.json`, `result_lines.json`,
   `export.json`; `update_transitions.json` already exists from P191) written
   with `SECS_RECORD=1` by the recording tests in `crates/secs-service/tests/recorded.rs`.
   The UI mock rewrite that consumes them is a separate packet (Opus), not
   yours.

5. **X-5 residue** (A5, from P201 item 5): `Design.chain`, `secs_engine::mimo`
   and the two resampling constants are still `pub` because
   `crates/secs-service/src/{compute.rs,export_run.rs,lib.rs}` read
   `secs_engine::mimo::{AccelerationSelection, cap_text}` and `calibration.rs`
   and `secs-gpu` read the others. Re-export what the service needs from the
   engine root (`AccelerationSelection` is already on ADR 0046's list; add a
   root-level `mimo_cap_text` or move `cap_text` behind a `Design` method),
   switch the service and `secs-gpu` to those, make the three items
   `pub(crate)`, and extend `public_surface_is_the_adr_0046_list`. Commit:
   `refactor(audit): X-5 서비스와 GPU가 엔진 루트의 재수출만 읽고 남은 공개 항목을 비공개로 바꿉니다 (#186 A5)`.

Allowed files: P190's list plus `Cargo.lock`, `crates/secs-engine/src/**`, `crates/secs-engine/tests/**`, `crates/secs-gpu/src/**`, `crates/secs-service/src/{compute.rs,export_run.rs}`, `crates/secs-service/src/{calibration.rs,results.rs,curves.rs,jobs.rs,settings_edit.rs,lib.rs}`,
`crates/secs-service/tests/**`, `apps/secs2r/ui/src/ipc/bindings/**` and
`apps/secs2r/ui/src/ipc/recorded/**` (generated), and for step 3 only the
UI call sites named above. Forbidden: ADRs, `docs/**` other than the API
README files P190 allows, `CHANGELOG.md`, `audit-state.md`.

## Verification (foreground)

P190's verification list, with its workspace-wide commands replaced by the
touched packages (`secs-types`, `secs-api`, `secs-local-api`, `secs-sys`,
`secs-service`, `secs-dotnet-sys`, `secs-py`, `secs2r`, `secs2r-cli`,
`secs2r-host-shell`, `secs-policy`), then `cd apps/secs2r/ui && npm run typecheck && npm test`,
the wheel's pytest and the .NET tests as P190 says, and
`python3 .github/scripts/check_feature_tests.py` (only the two known Windows
blind spots may remain).

## Report

P190's report format: per step the Red test and its first failure line, the
commit hash, files, what was deleted; the public interfaces the bindings and
the screen consume (names and shapes); anything still blocked with the
reason; `git log --oneline fix/audit-186..audit/186-api-adr-2`.
