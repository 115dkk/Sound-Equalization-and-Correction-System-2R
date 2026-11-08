# P201 (Daybreak): audit #186, the publishing gate after ADR 0050, then B-31, X-32, B-36 and the X-5 residue

Worktree `E:\wt\audit-engine`, branch `audit/186-testkit-fix` (Claude creates it
from the then-current `fix/audit-186` and checks it out before handing you
this packet; check `git log -1` and `git status` first and stop if the branch
or the tree is not as described). Every cargo command, and every script that
starts cargo (`check_feature_tests.py` included), runs with
`CARGO_TARGET_DIR=C:\cargo-target\audit-engine`; after the first command check
that `E:\wt\audit-engine\target` does not exist. No workspace-wide `cargo test`
or `cargo clippy`; per-package (`-p`) only. One commit per item, titles as
given, trailer `Co-Authored-By: Claude Mythos 5.1 <noreply@anthropic.com>`. Do
not push, do not rebase, do not touch `audit-state.md` or ADRs.

## Why

P188 (ADR 0050, X-9) moved the oracle-driven test loader into
`secs_engine::testkit` behind the `test-support` feature and made `secs-oracle`
an optional normal dependency of `secs-engine`
(`crates/secs-engine/Cargo.toml`: `test-support = ["dep:secs-oracle", ...]`).
`secs-engine` is published on crates.io and `secs-oracle` is not, so the
policy gate `secs-policy::published_versions_and_dependencies_are_closed`
(`crates/secs-policy/tests/publishing.rs:303`) now fails with
`secs-engine [dependencies] secs-oracle: unpublished workspace dependency`,
and `cargo publish --dry-run` would refuse the manifest. P195 could not do
B-31 and X-32 because that gate was red before its changes. Fix the gate
first, keeping ADR 0050's one loader.

## 1 · X-9 · the loader stays in `secs_engine::testkit`, the oracle stays outside

Design (ADR 0050 amended by Claude afterwards; implement this exactly):

- `crates/secs-engine/Cargo.toml`: `test-support = ["dep:serde_json", "secs-compute/test-support"]`;
  delete the optional `secs-oracle` normal dependency; keep `secs-oracle` as a
  dev-dependency (path only, as before X-9). `serde_json` stays optional and
  normal (it is published).
- `crates/secs-engine/src/testkit.rs` no longer names `secs_oracle`. It
  defines
  `pub trait FixtureStore { fn load(&self, dir: &str, name: &str) -> serde_json::Value; fn names(&self, dir: &str, prefix: &str) -> Vec<String>; fn calls(&self, dir: &str, prefix: &str) -> Vec<(String, serde_json::Value)>; fn platform_rtol(&self) -> f64; fn oracle_libm(&self) -> <the type secs_oracle::oracle_libm returns>; }`
  (adapt the exact shapes to what `secs_oracle::Fixture::{load, all, all_calls}`,
  `PLATFORM_RTOL` and `oracle_libm` provide today; read
  `crates/secs-oracle/src/lib.rs` first). Every testkit function that reads
  fixtures takes `store: &dyn FixtureStore` as its first parameter
  (`names(store)`, `load(store, name)`, `load_fixture(store, ..)`,
  `production_case(store, name, threads)`, `replay(store, name)`,
  `replay_case(store, name)`, `RecordedFits::for_case(store, name)`,
  `mimo_acoustic_metrics(store, ..)` and the rest). The small JSON accessors
  `number`, `integer`, `errors` that testkit uses from `secs-oracle` become
  private functions of testkit with a doc comment saying they copy
  `secs-oracle`'s because a published crate cannot depend on it (audit #186
  A9); keep them to the lines they need.
- `crates/secs-engine-testkit/src/lib.rs` (unpublished, may depend on
  `secs-oracle`): `pub struct Oracle;` implementing `FixtureStore` with
  `secs_oracle`, plus thin wrappers that keep today's public API without the
  store parameter (`pub fn replay_case(name) -> ReplayCase { secs_engine::testkit::replay_case(&Oracle, name) }`
  and so on for every function `secs-export`, `secs-service` and the engine's
  integration tests and examples use; `pub use` the types). `Cargo.toml` of the
  facade adds `secs-oracle.workspace = true`.
- The engine's own unit tests (`crates/secs-engine/src/tests/*.rs`,
  `src/testkit.rs` tests) get one adapter `crate::tests::support::Oracle`
  (same impl, about 15 lines, `secs-oracle` from dev-dependencies) and pass
  `&Oracle` where they call testkit; `examples/engine_perf.rs` and
  `tests/surface.rs` use the facade (they already link the lib once).

Gates (all must hold before the commit):

```
cargo test -p secs-policy --test publishing --locked
cargo tree -p secs-engine -e normal --features test-support | grep -c secs-oracle   # 0
cargo test -p secs-engine -p secs-engine-testkit -p secs-export --locked           # same counts as before
cargo test -p secs-service --locked
cargo build -p secs-engine --locked
```

Commit: `refactor(audit): X-9 엔진 testkit이 fixture 저장소를 인터페이스로 받고 secs-oracle 의존을 뺍니다 (#186 A9)`.

## 2 · B-31 · TD-243 · the cfg spellings

Files: `crates/secs-sys/src/**`, `apps/secs2r-host/src/{files.rs,main.rs}`,
`apps/secs2r-host-shell/src/main.rs`. Unify the two spellings of the Windows
cfg (`#[cfg(target_os = "windows")]` and `#[cfg(windows)]`) to `#[cfg(windows)]`
everywhere in those files; the P195 report lists the lines. Do not touch the
RFC 3339 formatting: the host shell writes `YYYY-MM-DDTHH:MM:SSZ` and the host
writes `...+00:00`, and unifying them changes visible bytes (reclassified; say
so in the report). Gate: `cargo test -p secs-sys -p secs2r-host -p secs-policy --locked`
with the same counts, `cargo clippy` for the three packages native and for
`--target x86_64-pc-windows-msvc`. Commit:
`refactor(audit): B-31 Windows cfg 표기를 하나로 맞춥니다 (#186 TD-243)`.

## 3 · X-32 · A32 · delete the hand-written Rust lexer in the policy gates

Follow P195's X-32 section (`docs/packets/P195-daybreak-audit-misc-b.md`,
"### X-32") verbatim now that the gate is green: delete the lexer and its
tests from `crates/secs-policy/tests/*.rs`, keep the gates that used it on
`cargo test -- --list` output (already the stronger check), and fix the
`features.toml` entries that named the deleted tests. Gate:
`cargo test -p secs-policy --locked` green, `python3 .github/scripts/check_feature_tests.py`
listing only the two known Windows blind spots. Commit as P195 says.

## 4 · B-36 · TD-259 · pair correlation and pair-phase recompute windows and stages

P194 section 2 could not do this item because its file list lacked
`crates/secs-engine/src/rc.rs`. Follow P194's "### B-36" section verbatim
(`docs/packets/P194-daybreak-audit-filters-engine-b.md`) with `rc.rs`,
`polarity.rs`, `pair_phase.rs`, `design.rs` and `mimo/*.rs` allowed: compute
the RC roll-off once in `rc.rs::prepare_correction` and carry it in
`FirPreparation`, memoise the polarity window spectra and the pair-phase
top response as the section says. Category B: no new tests; engine goldens
bit-identical; counts unchanged. The `polarity::check` cancel token stays
out (Category A, noted). Commit with P194's title.

## 5 · X-5 residue · A5 · the three items left public

P194 section 2 kept `Design.chain`, `secs_engine::mimo` and the two
resampling constants public because `crates/secs-service/src/calibration.rs`
and `crates/secs-gpu` read them. With those files allowed now, move each read
behind a `Design` method or an existing export (`signal_chain()`, the
`mimo_responses` accessor, a `secs_dsp::rate` constant), make the three
items `pub(crate)`, and extend `public_surface_is_the_adr_0046_list` to the
narrowed list. Gates: `cargo test -p secs-engine -p secs-gpu -p secs-service --locked`,
`cargo clippy` for the three. Commit:
`refactor(audit): X-5 서비스와 GPU가 Design 내부 대신 메서드를 읽고 남은 공개 항목을 닫습니다 (#186 A5)`.

Allowed files for items 4 and 5, beyond item 1's: `crates/secs-engine/src/**`,
`crates/secs-gpu/src/**`, `crates/secs-service/src/calibration.rs`, their
tests, `features.toml`.

## Report

Per item: the gate outputs' last lines, the commit hash, files, what was
deleted; for item 1 the `cargo tree` count and the list of testkit functions
whose signature gained the store parameter; anything you could not do with
the reason. Finish with `git log --oneline fix/audit-186..audit/186-testkit-fix`.
