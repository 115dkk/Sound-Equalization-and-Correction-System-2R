# P81 (Daybreak Blue): audit #92 — WASAPI capture, probe and diagnostics (W1)

Repository: `/home/user/Sound-Equalization-and-Correction-System-2R`. Work in
the main tree: `git switch -c wip/w1` from the current `HEAD`. Do not push, do
not switch branches again, do not spawn sub-agents. Commit once per finding
(subjects below); Claude reviews, re-runs the gates and re-authors the
commits.

`crates/secs-sys/src/wasapi.rs` is compiled on Windows only. This machine is
Linux: you can type-check and lint it with the Windows target
(`rustup target add x86_64-pc-windows-msvc` once, then the clippy command
below), but its unit tests run only in CI's Windows `test` job. Write the
tests anyway (`#[cfg(test)]` in `wasapi.rs`, following the module's existing
tests at the end of the file, which use fakes rather than devices) and keep
them deterministic; say in the report that they were not executed locally.

Read first: `CLAUDE.md`, `docs/adr/0002-unsafe-isolation-and-policy-gates.md`,
`docs/adr/0011-taskbar-icon-unsafe.md`, `docs/UNSAFE_BOUNDARIES.md`,
`docs/HARDWARE.md`, `TECH_DEBT_AUDIT.md` (rows TD-56, TD-57, TD-58, TD-80),
`crates/secs-sys/src/wasapi.rs` (whole file), `crates/secs-sys/src/cpal.rs`
(its stall check), `crates/secs-audio/src/audio.rs` (the error contract).

Rules: no new `unsafe` (the crate's budget is 3, all in `taskbar.rs`); no
allocation, lock or blocking I/O added to the MMCSS capture/render loops
beyond what is there; keep the existing tests passing.

## Findings, in order

1. **TD-56** — capture (`wasapi.rs:1855`) and render waits (`:360-392`) loop
   forever while the device stays silent without an error HRESULT: the 10 ms
   event timeout is treated as success (`:759-763`) and no progress clock
   exists. Track the last instant a packet arrived / frames were accepted /
   padding fell; when nothing progressed for 2 s (the same limit cpal uses —
   share one constant name and value with `cpal.rs` if both are in this crate),
   return the stall error the audio contract defines (find the variant cpal
   uses). Test: a fake packet source that never delivers → stall error after
   the limit (inject a clock so the test does not sleep 2 s); one that is slow
   but progressing → no error. Commit:
   `fix(audit): A-53 WASAPI 녹음과 재생이 멈춘 장치에서 무한히 기다리지 않습니다 (#92 TD-56)`
2. **TD-57** — the capture queue reserves one endpoint buffer (`:1511`) but
   the read loop allows two buffers of backlog (`:1868-1884`), so
   `VecDeque::extend` can reallocate on the MMCSS thread. Reserve the full
   allowed backlog when the session opens and check remaining capacity before
   pushing (drop and flag a discontinuity like cpal's overflow path if it
   would exceed). Test: fill to the backlog limit with several packets after
   the destination is full → `capacity()` unchanged. Commit:
   `fix(audit): A-54 WASAPI 녹음 큐를 미리 예약해 실시간 스레드에서 늘리지 않습니다 (#92 TD-57)`
3. **TD-58** — the shared-mode probe (`:1361`) turns every trial-initialise
   failure into `supported = false`; device removal or a stopped audio
   service reads as "format rejected". Use the same HRESULT classification as
   the exclusive probe and open (`:1021-1029`, `:1198-1207`): format
   rejections → unsupported, everything else → probe error. Factor the
   classification into one function if it is duplicated. Test: the
   classification function over a table of HRESULTs. Commit:
   `fix(audit): A-55 WASAPI 공유 모드 탐색이 장치 오류를 형식 거부로 바꾸지 않습니다 (#92 TD-58)`
4. **TD-80** (C) — document the diagnostic variables
   `SECS_PA05_SHARED_PERIODS` (2, 3 or 4; default 4) and
   `SECS_PA05_TRACE_PREFIX` (absolute path prefix for CSV traces) in
   `docs/HARDWARE.md`: purpose, accepted values, default, file names, how to
   unset; and report trace file create/write/flush failures on stderr once
   per session instead of discarding them (`:719`) — no change to measurement
   behaviour. Commit:
   `chore(audit): C-8 WASAPI 진단 변수를 문서에 적고 trace 오류를 알립니다 (#92 TD-80)`
5. **TD-99** — `taskbar.rs:26` allocates the `CreateIcon` AND mask as
   `width * height` bytes, but a monochrome bitmap row is word-aligned:
   `ceil(width / 16) * 2 * height` bytes. The public check
   (`lib.rs:56`) allows width 1, where Windows reads past the buffer. Compute
   the length with a small pure function and use it; no change to the unsafe
   blocks or their count. Test (runs on Linux if the function is outside the
   Windows-only module): widths 1, 16, 17, 256 → 2, 2, 4, 32 bytes per row.
   Commit:
   `fix(audit): A-88 작업 표시줄 아이콘 AND mask 크기를 규칙대로 셉니다 (#92 TD-99)`

## Allowed files

`crates/secs-sys/**`, `docs/HARDWARE.md`, `Cargo.lock` as cargo changes it.

## Gates (foreground; do not end the turn before they finish)

```sh
cargo fmt --all -- --check
cargo clippy -p secs-sys --all-targets --locked -- -D warnings
cargo clippy -p secs-sys --all-targets --locked --target x86_64-pc-windows-msvc -- -D warnings
cargo test -p secs-sys --locked
cargo test -p secs-policy --locked
```

(`secs-policy` checks the unsafe budget.)

## Report (final answer)

Per finding: commit hash, files, test names, the Windows-only tests you
could not execute, the clippy output for the Windows target. Then anything
left undone.
