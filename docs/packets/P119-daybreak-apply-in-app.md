# P119 (Daybreak): the 적용 level in the app (ADR 0018)

Worktree: `E:\Sound-Equalization-and-Correction-System-2R\.claude\worktrees\apply-levels`
(branch `claude/apply-levels`; P113 added `crates/secs-apply`). Do not
commit, do not push. Work only in this worktree. The screen comes later
(P121 Opus, P122 ASTRA); the managed service and its shell are P120. Do not
build them here.

Read first: `CLAUDE.md`, `docs/adr/0018-applying-to-camilladsp-and-equalizer-apo.md`,
`crates/secs-apply/src/**` (as P113 left it), `crates/secs-types/src/ipc.rs`
(`AppSettings`, `Request`/`Response`, `ExportParams`, `ExportResult`,
`IpcError`), `crates/secs-service/src/lib.rs` (`Deps`, how `audio_backend`
and `host` are injected), `crates/secs-service/src/export.rs` and
`design_export.rs` (export bundles, `secs-design.json`, design ID),
`crates/secs-sys/src/lib.rs` and `Cargo.toml`, `apps/secs2r/src/main.rs`
and `apps/secs2r-cli/src/main.rs` (where `Deps` are built),
`features.toml` (how IPC methods are registered).

## Fixed design

1. **Settings.** `AppSettings` gains

   ```rust
   #[serde(default)] pub apply: ApplySettings,
   pub struct ApplySettings {
       pub camilla: CamillaApply,   // level, address
       pub eqapo: EqApoApply,       // level
   }
   pub struct CamillaApply { pub level: ApplyLevel, pub address: String /* default "127.0.0.1:1234" */ }
   pub struct EqApoApply { pub level: ApplyLevel }
   #[serde(rename_all = "snake_case")] pub enum ApplyLevel { Files, Apply, Manage }
   ```

   Default `Files` everywhere. Old settings files load with the defaults.
   `Manage` is stored but this packet treats it like `Apply` (P120 adds the
   service); `apply_*` calls with `Manage` on a non-Windows host return a
   typed `unsupported` error.
2. **IPC methods** (types in `secs-types`, regenerate bindings with
   `cargo test -p secs-types`, register each as `ipc.<method>` in
   `features.toml`):
   - `apply_status` → `ApplyStatus { camilla: CamillaApplyStatus, eqapo: EqApoApplyStatus }`.
     `CamillaApplyStatus { reachable: bool, error: Option<IpcError>, status: Option<secs_apply::camilla::CamillaStatus> (mirrored as a secs-types struct), design: DesignMatch }`,
     `DesignMatch = This { design_id } | Other { title } | Unknown` (compare
     the running config's title and the design ID in its description, which
     0.31.0 writes, with the current result's design ID). Connect with a 2 s
     timeout; an unreachable CamillaDSP is a normal state, not an error reply.
     `EqApoApplyStatus { installed: bool, variant: EqApoVariant (Official | Xt | Unknown), config_path: Option<String>, include: IncludeState, applied_design_id: Option<String> }`
     (read `ConfigPath\SECS2R\secs-design.json` for the applied ID).
   - `apply_camilla` (a job, like exports): export the current design's
     CamillaDSP bundle into `<app data>\applied\camilladsp\` (replacing it,
     same writer and options as the CamillaDSP export, including the chosen
     devices), then `Camilla::apply(<that yml>, <its text>)`. Result:
     `ApplyResult { outcome: Applied | RolledBack { error }, design_id, folder }`.
     A stale result applies with the same stale flag exports carry.
   - `apply_eqapo` (a job): require an Equalizer APO install; export the
     Equalizer APO bundle into `ConfigPath\SECS2R\` (write every file to a
     temp name in that folder, then rename; remove files of the previous
     SECS2R bundle that the new one does not have), then edit `config.txt`
     with `secs_apply::eqapo::with_include` (keep `config.txt.secs2r-backup`
     from the first change; write temp then rename). The bundle's own
     `config.txt` becomes `SECS.txt` (look at what the Equalizer APO export
     writes and map its main file to `SECS.txt`; FIR paths inside it must be
     relative to `SECS2R\` or absolute, check which Equalizer APO resolves
     for an `Include`d file and test it against the XT engine the
     `eqapo-playback` CI job already downloads, if you can run it locally).
     Permission errors come back as a typed `permission_denied` error with
     the path.
   - `revert_eqapo`: `without_include`, leave the `SECS2R` folder.
   - `apply_settings_set(ApplySettings)`: validates the address
     (`host:port`), stores, returns the settings.
3. **OS access.** Add to `secs-sys` (Windows only, safe crates only, no new
   `unsafe`): `pub fn eqapo_install() -> Option<EqApoInstall>` reading
   `HKLM\SOFTWARE\EqualizerAPO` (`ConfigPath`, `InstallPath`) with
   `windows-registry`. Decide `variant` from what the XT installer leaves
   (check the XT sources at https://github.com/115dkk/EqualizerAPO-XT for its
   registry writes, version resource or install path; write what you found
   into ADR 0018's Equalizer APO section, replacing the "구현할 때 확인"
   sentence). Other platforms return `None`. The service gets it through a
   new `Deps` field (`Arc<dyn Fn() -> Option<EqApoInstall> + Send + Sync>`,
   default `None`), wired in the desktop app and the CLI.
4. **CLI.** `secs2r-cli apply camilla [--address host:port]`,
   `secs2r-cli apply eqapo`, `secs2r-cli apply eqapo --revert`,
   `secs2r-cli apply status`.
5. **Local API.** Nothing to do if the local API forwards every `Request`
   (check `secs-local-api` once `main` has it; this branch may not); say
   what you found.

## Tests

- Settings round trip and old files.
- `apply_camilla` against a fake CamillaDSP websocket server (reuse P113's
  in-process server helper): applied, validation rejected, reload rolled
  back; the written folder replaced; `DesignMatch` cases.
- `apply_eqapo` / `revert_eqapo` against a temp folder posing as
  `ConfigPath` through the injected detector: fresh install, re-apply
  without change (no write), damaged include repaired, backup made once,
  stale files removed, permission error mapped (simulate with a read-only
  file where the platform allows; otherwise test the mapping function).
- CLI argument parsing.

## Files

You may edit: `crates/secs-types/**`, `crates/secs-service/**`,
`crates/secs-apply/**` (only to fix what P113 got wrong; say what),
`crates/secs-sys/**`, `apps/secs2r/src/**` (wiring `Deps`),
`apps/secs2r-cli/**`, `apps/secs2r/ui/src/ipc/bindings/**` (generated; if
the screen's typecheck then needs fixture fields, add defaults to
`ipc/mock.ts` and `ipc/fixtures.ts` only), workspace `Cargo.toml`,
`Cargo.lock`, `features.toml`, `docs/adr/0018-applying-to-camilladsp-and-equalizer-apo.md`
(the one sentence above), `CHANGELOG.md`.

## Verification (foreground; do not end the turn before they finish)

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo clippy --workspace --exclude secs2r --all-targets --locked --target x86_64-pc-windows-msvc -- -D warnings
cargo test --workspace --all-targets --locked
cargo deny --all-features check
python3 .github/scripts/check_feature_tests.py
(cd apps/secs2r/ui && npm run typecheck)
```

## Report

The XT detection finding, the Equalizer APO include path finding, the IPC
types as built, files changed, and the commands' last lines.
