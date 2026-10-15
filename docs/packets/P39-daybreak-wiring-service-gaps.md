# P39 (Daybreak Blue): service gaps found while wiring the screens

Worktree: `/home/user/wt/p39` (branch `m5/p39`). Do not commit; Claude
reviews the diff and commits.

P33 wired the screens and found three things the service does not answer.
Each is small; together they let the app stop guessing.

Read first: `CLAUDE.md`, `CONTEXT.md`, `docs/adr/0003-stack-and-ipc.md`,
`crates/secs-types/src/ipc.rs`, `crates/secs-service/src/{monitor.rs,measurement.rs,app_settings.rs or wherever AppSettings is stored}`,
`crates/secs-service/tests/{monitor.rs,measure_ipc.rs}`, and the hooks that
will use the answers: `apps/secs2r/ui/src/screens/connect/useConnectScreen.ts`
and `apps/secs2r/ui/src/App.tsx` (read only).

## 1. Read the stored measurement settings

A reopened project keeps its measurement method and settings in the
`.secs` (`extra.devices`), but the only way to read them is
`measure_configure`, which changes them. Add:

```rust
Request::MeasureConfigGet            // no params
Response::MeasureConfigGet(MeasureConfigResult)
```

It returns exactly what `measure_configure` would return for the stored
settings, without changing the project or marking it dirty. A project
without a stored method reads as upstream does today (the compatibility
default the service already uses, `native` for old projects, `none` for new
ones; see U10). No project open: `no_project`. Register `ipc.measure_config_get`
in `features.toml` with a test.

## 2. The theme preference

`AppSettings` gains `theme: AppTheme` (`system` | `light` | `dark`,
`#[serde(default)]` = `system`) so the choice survives a restart. Old
settings files without the field read as `system`. Test the round trip and
the old-file default. This is new (upstream has one theme); add a line to
`docs/DEVIATIONS.md` only if the table lists screen-only additions (look at
how U7 and U9 are written and follow that).

## 3. The end of a level burst

`level_play` returns at once and the monitor job plays the 2.5 s burst.
The screen has no way to know when it ended, so "play every channel"
waits a fixed time. After each burst ends — finished, cancelled by
`level_stop`, or replaced by a newer `level_play` — the monitor job sends

```rust
JobEvent::Log { job, code: "level_play_end", params: { "role": <role>, "outcome": "done" | "stopped" } }
```

on both monitor loops (sampled and REW). The level stored by a finished
burst is stored before the event goes out, so a `levels_get` after the
event sees it. Tests: a mock-method monitor plays one role and the events
arrive in the order Meter…, `level_play_end{done}`; `level_stop` during a
burst gives `level_play_end{stopped}` and the monitor keeps running; a
second `level_play` during a burst ends the first with `stopped`. Add
`level_play_end` to the log codes the screen knows: the catalog key is
`log.level_play_end` (ko: "레벨 재생 끝", en: "Level burst ended") in both
`apps/secs2r/ui/src/i18n/{ko,en}.json`; check how other log codes are keyed
before adding.

## Files you may change

`crates/secs-types/src/**`, `crates/secs-service/src/**`,
`crates/secs-service/tests/**`, `apps/secs2r/ui/src/ipc/bindings/**`
(regenerated only, by `cargo test -p secs-types`), `apps/secs2r/ui/src/ipc/mock.ts`
(answer `measure_config_get`, add `theme` to the mock settings, send
`level_play_end` from the mock monitor), `apps/secs2r/ui/src/ipc/fixtures.ts`,
`apps/secs2r/ui/src/i18n/{ko,en}.json` (the one log key), `features.toml`,
`docs/DEVIATIONS.md`. Do not change the screens, hooks, `App.tsx` or the
kit: Claude wires the hooks onto these answers.

## Verification (foreground; finish before you report)

```sh
cd /home/user/wt/p39
export CARGO_TARGET_DIR=/home/user/target-p39 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo clippy --workspace --exclude secs2r --all-targets --locked --target x86_64-pc-windows-msvc -- -D warnings
cargo test --locked -p secs-types -p secs-service -p secs-policy
python3 .github/scripts/check_feature_tests.py
cd apps/secs2r/ui && export PATH=/opt/nvm/versions/node/v24.22.0/bin:$PATH && npm ci --no-audit --no-fund && npm run typecheck && npm test
```

Tests must not write outside a temporary folder (CI runs as a normal user).
Delete `/home/user/target-p39` when you are done; disk is tight.

## Report

Per item: the types and methods added, where the event is sent, the tests,
and the checks with their results.
