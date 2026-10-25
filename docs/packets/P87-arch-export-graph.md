# P87: audit #92 C11 — one export graph behind the signal chain

Claude names the worktree and branch when it hands this over (it builds on
P77's storage and writer fixes). Do not switch branches, push, or spawn
sub-agents. Commit once per step with the subjects given; Claude reviews,
re-runs the gates and re-authors the commits.

Read first: `CLAUDE.md`, `CONTEXT.md` (신호 사슬), `docs/ARCHITECTURE.md`
sections 2–4, `docs/adr/0004-parity-and-deviations.md`,
`docs/adr/0012-resource-optimizations.md`, `docs/DEVIATIONS.md` row F13,
then `crates/secs-types/src/chain.rs`, `crates/secs-camilla/src/lib.rs`,
`crates/secs-filters/src/{lib.rs,playback.rs,optimization_tests.rs}`,
`crates/secs-filters/tests/*`, `crates/secs-camilla/tests/*`,
`crates/secs-export/src/lib.rs` (what it borrows from secs-camilla), and
the service call sites (`grep -n "secs_camilla::\|secs_filters::" crates/secs-service/src/*.rs`).

## Problem

Every consumer of (`SignalChain`, `MimoPaths`) validates them itself and
builds the same topology itself, and the rules have drifted: three
validators (`secs-camilla` `validate`/`validate_mimo`, `secs-filters`
`validate_chain`/`validate_run`, `MimoFirPlayback::new`), three support
grouping keys (Camilla `(output, low_hz, fir)` with the minimum delay;
filters `(support, signature)`; playback's groups), four low-rate FIR
checks (`lowrate_len_48k` with the `fs_low` bound), and `secs-filters`
borrows formatting, WAV and file helpers from `secs-camilla`. CONTEXT says
the signal chain is one value including its MIMO paths; in code the paths
travel separately.

## Design (fixed)

A new crate `crates/secs-graph` (`#![forbid(unsafe_code)]`, `[lints]
workspace = true`, dependencies `secs-types`, `secs-dsp`, `thiserror`; add it
to the workspace `Cargo.toml` members and `[workspace.dependencies]` the way
the other crates are). Dependency direction: `secs-camilla → secs-graph`,
`secs-filters → secs-graph`; nothing in `secs-graph` depends on a writer.

```rust
/// The LFE input the export includes, as upstream's `make_plan` takes it.
pub enum LfeChoice { … }          // whatever make_plan_with_lfe takes today

/// A validated signal chain with its MIMO paths and the topology every
/// export writer shares: inputs, outputs, per-role operations, sub feeds,
/// MIMO feeds and support groups. Building it is the only validation.
pub struct ExportGraph { /* private */ }

impl ExportGraph {
    pub fn build(chain: &SignalChain, mimo: Option<&MimoPaths>, lfe: LfeChoice) -> Result<Self, GraphError>;
    pub fn chain(&self) -> &SignalChain;
    pub fn mimo(&self) -> Option<&MimoPaths>;
    pub fn plan(&self) -> &FilterPlan;            // the upstream make_plan topology
    pub fn support_groups(&self) -> &[SupportGroup];
}
```

- **One validation.** `build` applies the union of today's rules with the
  strictest version of each: 48 kHz, every number finite, identifiers ASCII
  and safe, no duplicate or reserved roles (P77's TD-46 wording),
  MIMO input is a main and support an output, polarity ±1, non-negative
  delays (P77's TD-94), the low-rate FIR rule written once
  (`fs_low ≤ 48 kHz` and `lowrate_len_48k` fits). `GraphError` names the
  field as the writers' messages do today. Writer-only checks stay in the
  writer: CamillaDSP `Options` (mode, devices, LFE gain, latency), the
  filter run's label and stamp.
- **The plan moves down.** `Op`, `SubFeed`, `MimoFeed`, `FilterPlan`,
  `make_plan`/`make_plan_with_lfe`'s construction, `plan_responses` and
  `headroom_db` (the as-written transfer functions over the plan) move
  from `secs-filters` to `secs-graph`. `secs-filters` re-exports the moved
  public names so its public API does not break for the service; the
  service moves to `ExportGraph::build` where it builds a plan today.
- **One support grouping.** `support_groups` is computed once in `build`.
  Write down the key you use. Each writer then reads the groups instead of
  regrouping. If a writer's byte output needs a different grouping than the
  others (F13: CamillaDSP rounds the total delay to integer samples first,
  APO shares only identical serialized ms), keep that writer's delay
  decomposition in the writer on top of the shared groups, and say exactly
  why in a comment and the report. Do not change any writer's output.
- **Writers serialise.** `secs-camilla::make_config(_observed)`, the filter
  text and Equalizer APO writers, and `MimoFirPlayback::new` take or build
  an `ExportGraph` and stop validating chain and MIMO themselves. Their
  public entry points may keep their current parameters and build the
  graph internally if changing them would ripple through the service; say
  which you chose. `secs-filters` stops importing helpers from
  `secs-camilla` other than `pyfmt` (the Python number formatter, which the
  architecture review left in place); WAV bytes and any other shared helper
  it needs come from `secs-graph` or its own module.
- **Docs.** `CONTEXT.md` gains the term 내보내기 그래프 (export graph) in
  the existing table format; `docs/ARCHITECTURE.md` section 2 lists the
  crate, section 3's diagram shows the new edges, section 4's "신호 사슬
  소비자" row names the graph.

## Steps and commits

This is a deepening (resolve-audit category A): tests at the graph's
interface first, then move the behaviour, then delete the old tests the new
ones cover.

1. Create `secs-graph` with `ExportGraph::build`, the validation and the
   moved plan, with tests at its interface: one table test of invalid
   inputs (one row per rule above, each failing with its field named) and
   the existing plan/response unit tests moved from `secs-filters`
   unchanged in their assertions. Commit:
   `refactor(audit): A-72 내보내기 그래프 crate를 만들고 검증과 계획을 옮깁니다 (#92 C11)`
2. Move the filter text and APO writers and `MimoFirPlayback` onto the
   graph; delete their validators. Commit:
   `refactor(audit): A-72 필터 작성기와 FIR 재생기가 내보내기 그래프를 읽습니다 (#92 C11)`
3. Move the CamillaDSP writer onto the graph (validation and support
   groups); delete its chain/MIMO validators. Commit:
   `refactor(audit): A-72 CamillaDSP 작성기가 내보내기 그래프를 읽습니다 (#92 C11)`
4. Docs (CONTEXT, ARCHITECTURE). Commit:
   `docs(audit): A-72 내보내기 그래프를 용어와 구조 문서에 적습니다 (#92 C11)`

After each step every oracle golden in `secs-camilla`, `secs-filters`,
`secs-export` and `secs-service` passes with unchanged assertions, and the
CamillaDSP/APO byte goldens are byte-identical. Report the test counts
before step 1 and after each step; the only count changes allowed are the
moved tests (same number) and the deleted validator tests that the step-1
table replaces (name each deleted test and the table row that covers it).

## Allowed files

`crates/secs-graph/**` (new), `crates/secs-camilla/**`, `crates/secs-filters/**`,
`crates/secs-export/**` (imports only), `crates/secs-service/src/**` and
`crates/secs-service/tests/**` (call sites only), `Cargo.toml` (workspace
members and dependencies), `Cargo.lock`, `deny.toml` only if `cargo deny`
needs the new crate listed, `CONTEXT.md`, `docs/ARCHITECTURE.md`,
`docs/DEVIATIONS.md` (only to point F13 at the graph).

## Gates (foreground; do not end the turn before they finish)

Export `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=line-tables-only`.
After each step: `cargo fmt --all -- --check`;
`cargo clippy -p secs-graph -p secs-camilla -p secs-filters -p secs-export -p secs-service --all-targets --locked -- -D warnings`;
`cargo test -p secs-graph -p secs-camilla -p secs-filters -p secs-export -p secs-service --locked`.
At the end: `cargo test --workspace --locked`, `cargo deny --all-features check`,
`cargo run -p secs-policy --locked` if the policy gate exists as a binary
(check `crates/secs-policy`; otherwise its tests), and
`python3 .github/scripts/check_feature_tests.py`. Check `df -h /home/user`
before the workspace test; if under 5 GB is free, delete older duplicate
executables in this worktree's `target/debug/deps` (newest per name and
source kept) and nothing else.

## Report (final answer)

Per step: commit hash, files, tests moved/added/deleted, counts; the
support-group key and any writer-specific decomposition kept; whether the
writers' entry points changed; tail of the last gate output; anything left
undone.
