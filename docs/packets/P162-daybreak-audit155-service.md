# P162 (Daybreak): audit #155, group W2: service, project files, public API

Follow the **common rules** in `docs/packets/P161-daybreak-audit155-deploy.md`.

Worktree `E:\Sound-Equalization-and-Correction-System-2R\.claude\worktrees\audit-155-w2`,
branch `claude/audit-155-w2`. Files: in `crates/secs-service`: `src/session.rs`,
`src/export.rs` (only the mdat parts: cached mdat export and raw measurement
reading; P161 changes the index and convolution parts of the same file),
`src/calibration.rs`, `src/mic_calibration.rs`, `src/results.rs`,
`src/results_tests.rs`, `src/project_edit.rs`, `tests/*` for your items;
`crates/secs-project/**`; `crates/secs-local-api/**`; `crates/secs-api/**`;
`bindings/python/**` (only if A-28 changes what it forwards); `apps/secs2r/src/main.rs`,
`apps/secs2r/src/local_api.rs`; `fuzz/**`. Read ADR 0009, 0014, 0016, 0019,
0027, 0028, 0029 and `docs/DEVIATIONS.md` (F16 to F18).

`secs-project` reads untrusted files: if `cargo +nightly-2026-10-01 miri` is
installed, also run `cargo +nightly-2026-10-01 miri test --locked -p secs-project --lib`;
otherwise say in the report that Miri was left to CI.

## Items (in this order)

| Item | Finding | Cat | Seam and what is fixed |
|---|---|---|---|
| A-1 | TD-04 | A | `project_save_as` service request. Source `<tmp>/case.irs/room.secs` saved as `<tmp>/case.secs` is refused and the source project and every file in `case.irs` stay. Check containment both ways on the normalised measurement folders, and also refuse when the open project file is inside the destination's measurement folder. |
| A-8 | TD-112 | A | Project open in `secs-project`. A `.src.json` sidecar larger than its cap is refused with the existing format error (read at most cap + 1 bytes). Cap: 16 MiB per sidecar. Records that point at the same sidecar load it once and share it. |
| A-9 | TD-19 | A | `secs-project` JSON parser with an explicit allocation budget: every node charges `size_of` of the value plus string bytes and vector capacity; exceeding it is the existing parse error. Default budget: 4 times the input length plus 16 MiB. Test: a document under the file-size cap whose wide array of nulls exceeds a small test budget is refused; the existing fixtures still parse with the default budget. |
| A-11 | TD-109 | A | `secs-local-api` token file on Unix (`#[cfg(unix)]` test): a new token file has mode 0600 regardless of umask (`OpenOptionsExt::mode`, no unsafe); an existing token with group or other bits is tightened to 0600 before use. Windows keeps the profile folder's ACL (write that reason at the site). |
| A-12 | TD-116 | A | `export` service request for cached mdat: when the project's calibration inputs (settings, target, measurement identity, the same signature the result book compares) change between the request and the job's replay, the job fails with the error calibration uses for inputs that changed mid-job (`calibration.rs` near line 115), and nothing is written. When nothing changed the result is as before. |
| A-26 | TD-130 | A | Same service: the mdat export reads and widens the WAVs outside the session lock, after the job slot is reserved, with the job's cancel token and memory budget. Test: a `ProjectInfo` request is answered while the export reads (use the existing slow-reader test hooks if any; otherwise add one behind `#[cfg(test)]`), and a cancel during the read ends the job cancelled. |
| A-38 | TD-10 | A | Result book staleness: a cached result without a config key is stale for config ("unknown" is not "current"). Invert `results_tests.rs` near line 458 with a comment citing `audit #155 TD-10 (Q1)`. |
| A-24 | TD-128 | A | Mic calibration file choice: take the `ProjectTicket` before releasing the lock and store through `session::apply` with that ticket. Test: switching projects during the (test-delayed) read leaves project B unchanged and not dirty. |
| A-25 | TD-129 | A | `recommend_xo` job: when the XO values or `xo_rec_include` change while it runs, completion stores only `xo_rec` and leaves `xo` as the user set it. Test with an edit during the job. |
| A-19 | TD-115 | A | App settings: each save gets a revision; applying effects (updater policy, local API, tray) accepts only a revision newer than the last applied one. Put the ordering in one small type in `secs-service` (for example `AppEffects { last: u64 }` with `fn accept(&mut self, revision) -> bool`) used by both `apps/secs2r/src/main.rs` and `apps/secs2r/src/local_api.rs`; test the interleaving at that type and one service-level case. |
| A-27 | TD-126 | A | `secs-local-api` server: request reading has a deadline (keep it one constant, a few seconds), and stop returns within a bounded time even with a client that sent half of a declared body. Test: open such a socket, call stop, assert it returns within the bound. |
| A-28 | TD-127 | A | Public `Engine` in `secs-api`: finished-job events are kept for at most the service's job retention (32) and the oldest are dropped; subscriptions use a bounded queue (1024) that never blocks the emitter and counts dropped events behind an additive method on the subscription (no change to existing enum variants). Update the Python binding only if it must forward the count. |
| C-2 | TD-134 | C | `fuzz/fuzz_targets/mdat_load.rs` also calls `load_mdat_project` with a small budget. Gate: `cargo check` of the fuzz crate if it builds on Windows; otherwise report it for CI. |

Crates to gate: `secs-service`, `secs-project`, `secs-local-api`, `secs-api`,
`secs2r` (clippy for the Windows target too), and `secs2r-python` if touched.
