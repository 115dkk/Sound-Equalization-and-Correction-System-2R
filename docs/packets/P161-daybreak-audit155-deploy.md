# P161 (Daybreak): audit #155, group W1: deploying file sets (A1) and the export writers

This packet also holds the **common rules** that P162 to P165 point to.

## Common rules (P161 to P165)

- The audit is issue #155. Read its two reports first: the tech debt audit at
  `C:\Users\32170336\AppData\Local\Temp\claude\E--Sound-Equalization-and-Correction-System-2R\a241f0ba-0f2d-4875-a552-204969fa6951\scratchpad\audit\TECH_DEBT_AUDIT.md`
  and the architecture review `ARCHITECTURE_AUDIT.md` in the same folder. Each
  item below names its finding (`TD-nn`) and, for behaviour changes, the seam
  the maintainer agreed the test goes through (the classification table in
  `audit-state.md` in the same folder). Line numbers in the reports refer to
  commit `21a1e981`; find the code by name if they moved.
- Read `CLAUDE.md` (rules and gates), `CONTEXT.md` (frozen terms) and the ADRs
  each item names. Interface ids never change: `data-ui` ids, IPC method names,
  `features.toml` ids, crate names.
- Work only in your own worktree and branch (named in your packet), only on the
  files your packet lists. Other workers change other files at the same time.
- Do **not** edit `CHANGELOG.md`, `features.toml`, `docs/adr/**` or
  `docs/ARCHITECTURE.md`. Report what they need (new test names per feature id,
  user-visible changes, ADR sentences) and Claude applies them.
- **Category A (behaviour change), test first.** For each A item: write the
  failing test at the agreed seam, run it and confirm it fails for the reason
  the finding describes (keep the failure line for your report), write the
  smallest fix, rerun, tidy only the lines you wrote. Test names describe the
  behaviour. Expected values come from literals or the spec, never from the
  code under test. Check the effect (what is on disk, what was sent), not that
  a call returned Ok.
- **Category B (refactor)**: no new tests; the existing suite is the gate and
  its pass count must not drop.
- **Category C (config, docs, comments)**: compiler or linter is the gate. A
  comment that records why odd code is right cites `audit #155 TD-nn` (or the
  question id `Q8` / `Q10`).
- **One commit per item**, in this form (Korean, 합니다체, no em-dash):
  `fix(audit): A-3 Equalizer APO 적용을 세대 폴더와 진입 파일 전환으로 바꿉니다 (#155 TD-105)`
  (`refactor(audit): B-n ...` for B, `chore(audit): C-n ...` for C), ending with
  the line `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`. Never
  amend, rebase or push. Commit only your own files (`git add <paths>`).
- **Gates before every commit** (foreground; finish them before you report):
  `cargo fmt --all -- --check`, `cargo clippy -p <your crates> --all-targets --locked -- -D warnings`,
  `cargo test -p <your crates> --locked`. Windows-only code: also
  `cargo clippy -p <crate> --all-targets --locked --target x86_64-pc-windows-msvc -- -D warnings`.
  To save disk, set `CARGO_PROFILE_DEV_DEBUG=0` and `CARGO_PROFILE_TEST_DEBUG=0`
  in the environment of every cargo command. If you change `crates/secs-types`,
  run `cargo test -p secs-types` to regenerate `apps/secs2r/ui/src/ipc/bindings/`
  and commit the bindings with that item.
- This machine is the maintainer's Windows PC: never install, start or stop a
  Windows service, never write ProgramData, Program Files, HKLM or HKCU Run,
  never run the app or drivers. Tests use temp folders and fakes.
- Check any API you are not sure of before using it (docs.rs, the Equalizer APO
  configuration reference). Use WebFetch when a packet names a URL.
- **Report**: commits (sha, item, finding), each A item's red failure line, new
  test names grouped by `features.toml` id, gates run with results, anything not
  done and why, and the CHANGELOG/ADR sentences you need.

## W1

Worktree `E:\Sound-Equalization-and-Correction-System-2R\.claude\worktrees\audit-155-w1`,
branch `claude/audit-155-w1`. Files: `crates/secs-graph/**`, `crates/secs-camilla/**`,
`crates/secs-apply/**`, `crates/secs-filters/**`, in `crates/secs-service`:
`src/apply.rs`, `src/design_export.rs`, a new `src/deploy.rs`, `src/lib.rs` (only
to declare `deploy`), the index and convolution parts of `src/export.rs`
(`design_export` index calls, `begin_commit` for convolution; P162 changes the
mdat parts of the same file, so keep your hunks to those functions), `tests/apply.rs`,
`tests/export*.rs` as needed; `apps/secs2r/src/update.rs` (B-1 only).
Read ADR 0016, 0017, 0018, `docs/CAMILLADSP_INTEROP.md`, `docs/EQUALIZER_APO_INTEROP.md`.

### Design (fixed)

Two different problems, two mechanisms:

1. **Export to a user folder** (`secs_graph::files::write_bundle_observed` and
   the writers on top of it): the folder layout is the user's contract (CamillaDSP
   or APO points at it), so it does not change. Instead the install becomes
   all-or-nothing: before an existing destination file is replaced, move it
   aside into a backup folder created next to the staging folder; if any rename
   fails, put every moved file back (and remove the new ones already installed),
   then return the error; on success remove the backups. Add a second observer
   to the hidden test hook, `before_install: &dyn Fn(usize, &Path) -> std::io::Result<()>`,
   called before each install rename, so a test can fail the n-th install.
2. **Applying to a running consumer** (`apply_camilla`, `apply_eqapo` in
   `secs-service/src/apply.rs`): every apply installs a complete new
   **generation** folder and switches to it with one step; the previous
   generation stays until the switch succeeded. New module
   `crates/secs-service/src/deploy.rs`:
   ```rust
   pub(crate) struct Generation { pub name: String, pub dir: PathBuf }
   /// Moves the fully written `stage` folder to `root/<prefix>-<k>` (smallest
   /// k >= 1 whose folder does not exist), creating `root`. Never touches other
   /// entries. Refuses when `root` is a link or junction.
   pub(crate) fn install(root: &Path, prefix: &str, stage: &Path) -> Result<Generation, IpcError>;
   /// Removes every entry directly under `root` except `keep` (file or folder
   /// names). A link or junction is removed as a link, never followed.
   pub(crate) fn prune(root: &Path, keep: &[&str]) -> Result<(), IpcError>;
   /// Replaces `path` atomically with `bytes` (temp file in the same folder +
   /// rename; reuse `secs_types::fs` if it already does this).
   pub(crate) fn write_entry(path: &Path, bytes: &[u8]) -> Result<(), IpcError>;
   ```
   - CamillaDSP: root `applied/camilladsp`, prefix the first 16 characters of
     the design id (as 0.45.1). Install, then validate and load the new config
     path through the `ApplyAdapter`; only after success `prune(root, &[new])`.
     On failure remove only the new generation; the previous generation's
     folder and files are still there, so the adapter's path restore works.
     Every apply, even of the same design, gets a different folder, which also
     keeps 0.45.1's guarantee that CamillaDSP re-reads FIRs.
   - Equalizer APO: root `<config>/SECS2R`, prefix as above. The main
     `config.txt` keeps its managed `Include: SECS2R\SECS.txt` line (unchanged).
     `SECS2R/SECS.txt` becomes the entry file whose only content is
     `Include: <generation>\config.txt` (CRLF, like the other APO text). Install
     the generation with the exporter's `config.txt` under its own name (no
     rename to `SECS.txt` any more, so the copied `secs-design.json` manifest
     stays true), `write_entry` SECS.txt, ensure the main include, then
     `prune(root, &["SECS.txt", new])`. The first apply after upgrading meets the
     old flat layout (FIRs and the old SECS.txt directly in `SECS2R`): prune
     removes those after the switch. Verify with the Equalizer APO configuration
     reference (https://sourceforge.net/p/equalizerapo/wiki/Configuration%20reference/)
     that an `Include` path and a `Convolution` path are relative to the
     directory of the file that contains them; report what you found.
   - Drop `collect_files`, `deploy_eqapo_bundle`, `remove_empty_dirs`,
     `mapped_eqapo_path` and `prune_applied` when nothing uses them.

### Items (in this order)

| Item | Finding | Cat | Seam / gate |
|---|---|---|---|
| A-5 | TD-22 | A | `secs_graph::files::write_bundle_observed` with `before_install` failing the 2nd install: the destination still holds every old file byte for byte and no new file. Add the same case for one writer (`secs-camilla` golden test file has the staging case at `tests/golden.rs` near line 1189). |
| A-3 | TD-105 | A | `apply_eqapo` service request in `tests/apply.rs` with the test ApplyAdapter and a temp APO config folder: after a successful apply the entry points at the new generation and the old generation is gone; when installing the generation fails (inject it), `SECS.txt` and the previous generation are unchanged. |
| A-4 | TD-106 | A | Same seam: a link (Unix symlink; on Windows a junction via `std::os::windows::fs::symlink_dir` if it succeeds, else skip that half with a message) inside `SECS2R` pointing to an outside folder with a sentinel file; after apply the sentinel still exists. |
| A-7 | TD-117 | A | Same seam: every file listed in the applied generation's `secs-design.json` exists with the listed size and hash. |
| A-6 | TD-108 | A | `apply_camilla` service request with a fake adapter that rejects the new config: the previous generation folder and its config file still exist with their bytes, and the adapter was told to restore the previous path. Also: re-applying the same design with only a device option changed gets a new folder. |
| A-10 | TD-110 | A | `export` service request to an explicit unnamed folder `<tmp>/parent/out`: a sibling `<tmp>/parent/locked` that cannot be read (Unix: mode 000; Windows: skip with a message) and a sibling link do not make the export fail, and the index does not walk outside the export base or the chosen set. Limit the walk to what `design_export` owns (read the index code first; never follow links; bounded depth). |
| A-31 | TD-133 | A | Convolution export job cancelled while computing (render/DTFT loops): the job ends cancelled and the destination folder has no new files. Move `begin_commit` to just before files are installed; pass the cancel check into the render and DTFT loops in `secs-filters` without changing arithmetic order. |
| A-29 | TD-131 | A | `secs_apply::eqapo` pure functions: a managed include line with trailing spaces is `Present`; repair and revert of a damaged file leave exactly one or zero include lines of `SECS2R\SECS.txt`; user includes stay. |
| A-30 | TD-132 | A | `secs_apply::camilla` client against a local test WebSocket server that only sends Ping frames: the request returns a timeout error within the deadline (choose the deadline from the existing connect timeout; keep it one constant). |
| A-40 | TD-144 | A | Convolution (REAPER) export at 96 kHz: the generated script's success text names 96000 Hz, and the script gets the routing sample rate as data. |
| B-1 | TD-74 | B | `export.rs` provenance write and `apps/secs2r/src/update.rs` use the shared atomic write in `secs_types::fs`; no behaviour change, suite unchanged. |

Crates to gate: `secs-graph`, `secs-camilla`, `secs-apply`, `secs-filters`,
`secs-service`, `secs2r` (B-1; clippy for the Windows target too).
