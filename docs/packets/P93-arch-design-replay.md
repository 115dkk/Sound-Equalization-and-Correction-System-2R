# P93: audit #92 C8 — the Design owns its replay

Claude names the worktree and branch when it hands this over (it builds on
P89's `calibrate`). Do not switch branches, push, or spawn sub-agents.
Commit once per step with the subjects given; Claude reviews, re-runs the
gates and re-authors the commits.

Read first: `CLAUDE.md`, `CONTEXT.md` (the time-axis convention is the
measurement module's), `docs/ENGINE.md`, `docs/adr/0007-compute-and-gpu.md`
(memory budget), `docs/adr/0012-resource-optimizations.md` (reuse must be
bit-identical), `docs/DEVIATIONS.md` (planned D24 if listed), then
`crates/secs-engine/src/{design.rs,measurement.rs,export_view.rs,preview.rs,xo.rs,chain.rs,display.rs,calibration.rs,lib.rs}`,
`crates/secs-engine/src/mimo/{prepared.rs,run.rs,design.rs}`,
`crates/secs-export/src/lib.rs`, `crates/secs-service/src/{calibration.rs,export.rs,curves.rs}`.

## Problem

`design` runs `measurement::prepare` (2^17-point spectra, arrival times,
`base_s`, alignment) and keeps only the 500 ms spectra. Downstream consumers
therefore carry (`Design`, measurements, config) together and prepare
again: `export_view` (each call), `mimo::compute` (calls `export_view` *and*
`prepare` again, `mimo/design.rs:~1340`), the XO preview and scores,
`secs-export`, which also redeclares `N_FFT` and recomputes the time axis
(`t0 = base_s − latency`, `secs-export/src/lib.rs:~346, ~527`), and the
service, which keeps the three in `CalibrationRun` and duplicates the
engine's applied-MIMO response for cached results
(`secs-service/src/export.rs` `cached_mimo_responses`, `delay_response`,
`cached_mimo_filter_response`, about 150 lines; IIR proven byte-identical by
`reopened_fresh_cache_keeps_mimo_in_mdat`).

## Design (fixed)

```rust
// crates/secs-engine/src/replay.rs (new), re-exported from lib.rs
/// What a design was made from, kept with it: the normalised measurements,
/// config and Setup, and the preparation's timing (arrival times, `base_s`,
/// alignment). Immutable and shared (`Arc`). Long-window spectra are
/// computed on request, never cached (ADR 0007: about 1 MiB per IR cell).
pub struct DesignReplay { /* private */ }

impl DesignReplay {
    pub fn measurements(&self) -> &MeasurementSet;
    pub fn config(&self) -> &EngineConfig;
    pub fn base_s(&self) -> f64;
    /// The 2 s export spectra (today's `export_view`).
    pub fn export_view(&self, design: &Design) -> Result<ExportView, EngineError>;
    /// One role's IR at one position on an export axis, with its t0: the
    /// time-axis rule `secs-export` computes today, written once here.
    pub fn export_ir(&self, design: &Design, role: &str, position: i64, /* window, axis as secs-export needs */) -> Result<(Vec<f64>, f64), EngineError>;
    /// The applied MIMO filter response of a cached result (today's service
    /// copy in `export.rs`), for IIR and FIR/FIR-low-rate paths.
    pub fn mimo_responses(&self, design: &Design, mimo: &MimoPaths, /* as the service needs */) -> Result<…, EngineError>;
}

pub struct Design { …, pub replay: Arc<DesignReplay> }
```

Exact method parameters follow what the callers need today; keep the set
small and name it after what the caller gets, not how. `design` builds the
replay from the `Prepared` it already has (no extra preparation).

- `export_view(design, measurements, config)` and the re-preparations in
  `mimo::compute` and the XO preview/scores that use the *same* config read
  the replay instead. An XO candidate with a different config still
  prepares its own (that is a different input); say which sites stay.
- `secs-export` takes IRs and t0 through `DesignReplay::export_ir` and stops
  redeclaring `N_FFT` or computing `base_s − latency` itself.
- The service's `CalibrationRun` keeps the `Design` only (measurements and
  config come from its replay); `cached_mimo_responses` and its helpers move
  into the engine as `mimo_responses` and the service copy is deleted.
- Bit identity: a test that every replay-derived value (export view
  spectra, export IR and t0 per role/position, MIMO responses, XO preview)
  equals the value from a fresh `prepare` on the same inputs, comparing
  `to_bits()`, on at least two oracle cases including one with MIMO. All
  goldens and `_before` tests pass unchanged.
- Memory: `design`'s reservation (TD-24) covers what the replay keeps; the
  replay holds the normalised measurements by `Arc`, not a copy (check
  `MeasurementSet`'s sample storage; if it cannot be shared without a copy,
  say what it costs per case and keep the copy only if the reservation
  already counts it).

## Steps and commits

Deepening (resolve-audit category A): tests at the replay's interface first.

1. `DesignReplay` built by `design`, the bit-identity tests, `export_view`
   and MIMO/XO same-config sites on it. Commit:
   `refactor(audit): A-70 Design이 준비 결과를 replay로 갖고 엔진 소비자가 다시 준비하지 않습니다 (#92 C8)`
2. `secs-export` reads IRs and t0 through the replay; its `N_FFT` and time
   axis go. Commit:
   `refactor(audit): A-70 secs-export가 시간 축을 replay에서 받습니다 (#92 C8)`
3. The service: `CalibrationRun` keeps only the `Design`;
   `cached_mimo_responses` moves into the engine. Commit:
   `refactor(audit): A-70 캐시 MIMO 응답 계산을 엔진 replay로 옮깁니다 (#92 C8)`

Report the test counts before step 1 and after each step.

## Allowed files

`crates/secs-engine/**`, `crates/secs-export/**`, `crates/secs-service/src/**`,
`crates/secs-service/tests/**`, `crates/secs-gpu/**` (call sites only),
`docs/ENGINE.md`, `CONTEXT.md` (only if a term changes), `Cargo.lock`.

## Gates (foreground; do not end the turn before they finish)

Export `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=line-tables-only`.
After each step: `cargo fmt --all -- --check`;
`cargo clippy -p secs-engine -p secs-export -p secs-service -p secs-gpu --all-targets --locked -- -D warnings`;
`cargo test -p secs-engine -p secs-export -p secs-service -p secs-gpu --locked`.
At the end `cargo test --workspace --locked` and
`python3 .github/scripts/check_feature_tests.py`. Check `df -h /home/user`
before the workspace test; if under 5 GB is free, delete older duplicate
executables in this worktree's `target/debug/deps` (newest per name and
source kept) and nothing else.

## Report (final answer)

Per step: commit hash, files, tests added/deleted, counts; the sites that
still prepare and why; the memory answer; tail of the last gate output;
anything left undone.
