# P23a-2 (Daybreak): fixes from the P23a review

Continue in `/home/user/wt/p23a` (branch `m4/p23a`; your P23a work is in the
tree, uncommitted). Same rules as P23a: do not commit, no sub-agents,
`export CARGO_INCREMENTAL=0 CARGO_TARGET_DIR=/home/user/wt/target-p23a`.
Do not create or edit `crates/secs-service/src/measure/**` or
`crates/secs-service/src/mdat_import.rs` (P23b owns them).

Review found seven things to change. Do them in this order; each one is
small. Keep everything else as it is.

## 1. Restore and write the IPC documentation

`crates/secs-types/src/ipc.rs` lost every doc comment it had (34 lines:
the module doc, `Request`, `METHODS`, `method()`, `Response`,
`BootstrapInfo` and its fields, `UpstreamBase`, `Platform` and its fields,
`IpcError` and its fields, `ErrorCode` and its variants, `Envelope`).
Restore each one word for word from `git show HEAD:crates/secs-types/src/ipc.rs`
(fix only a sentence the new code makes false, such as "long jobs report
progress through a separate event channel" → the event sink passed with the
call). Then give every new public type, variant and field a one-line `///`
doc in the same plain English style: what it is, its unit (ms, Hz, dB,
percent) and where it comes from upstream when that helps the screen
(`step_calibration.py` verdict card, `Metrics` field names). ts-rs copies
these into the TypeScript bindings, which the screen authors read; regenerate
them with `cargo test -p secs-types`.

## 2. Notes and warnings as codes, not Debug strings

`CalibrationSummary.notes`/`warnings` and `MimoSummary.notes` are `Vec<String>`
filled with `{:?}`. ADR 0003 and `docs/M4.md` §8: the service sends codes
and values, the screen writes the sentence. Add to `ipc.rs`:

```rust
pub struct Notice { pub code: String, pub params: BTreeMap<String, String> }
```

and use `Vec<Notice>` for those fields. Map each `secs_engine::Note` variant
(and each MIMO note) to `code` = the variant name in snake_case and `params`
= its fields formatted with `{}` (floats as Rust prints them, lists as JSON).
One mapping function with a test that covers every variant.

## 3. Tests in `crates/secs-service/tests/`

Move the integration tests out of `src/lib.rs` into
`crates/secs-service/tests/service.rs` (or a few files named by area) and
drive them through the public API (`Service::new(Deps)`, `Service::call`,
an `EventSink` collector). Unit tests of private helpers may stay in their
modules. Keep every test name you reported, so `features.toml` still points
at them (`secs-service::<name>` resolves in any test target of the crate).

## 4. SPL offset in mdat exports

Upstream passes `cal.project_spl_offset(project)` to both mdat exports
(`step_calibration.py:2455`, `step8_mdat.py:218`): the median of every
`spl_offset` value in the project's measurement entries, `None` when there is
none. The service passes `None`. Port `project_spl_offset` into
`crates/secs-project/src/signature.rs` (export it from `lib.rs`; median as
NumPy computes it: mean of the two middle values for an even count), pass
it to both mdat exports, and test it with one value, an even count, an odd
count and none.

## 5. Exports equal the direct exporters, byte for byte

The P23a packet asked for this and the tests only check structure.

- CamillaDSP: export through the service, then call
  `secs_camilla::export` with the same chain, labels and MIMO paths into a
  second folder; every file must be byte-identical.
- mdat: the UUIDs are random. Add a seam to `secs-export` (the only change
  allowed there): `export_mdat_with_mimo_using` and
  `export_before_after_mdat_with_mimo_using` that take
  `uuid: &mut dyn FnMut() -> Result<u128, ExportError>`, with the existing
  functions calling them with `random_uuid`. The service takes the UUID
  source from `Deps` (default random); the test injects a counter for both
  the service and the direct call and compares bytes.
- XO: the service's recommendation equals `secs_engine` `recommend_xo` run
  directly on the same inputs (the whole result, not "non-empty").
- MIMO: the service's paths equal `mimo::compute` run directly on the same
  inputs (`MimoPaths` equality).

## 6. Exports after reopening (S1)

Upstream caches the whole run so that a reopened project shows its results
and exports without recomputing (`run_cache_path` docstring: the 10-09 user
report). Keep the cache data-only, but make it hold what the CamillaDSP export
needs: the chain is already there; add the `RunLabels` inputs (mode,
`ultra_ms`, latency, `rc_hi`, `mimo_max_supports`) and the MIMO paths. After
reopening, `export { kind: camilla_dsp }` works from the cache without a
calibration run. The two mdat exports need the live design (its spectra are
too large for the cache): after reopening, the export job recomputes the
design first (a `Progress { stage: "design" }` event, cancellable) and then
writes. Write this into S1 in `docs/DEVIATIONS.md`, and test both paths
(Camilla after reopen equals the pre-close bytes without a calibration job
running; staged mdat after reopen equals the pre-close bytes with the
counter UUIDs).

## 7. Small things

- `apps/secs2r-cli`: keep it compiling, nothing more (P25 writes the CLI).
- Any `unwrap()` on a lock or an `Option` in non-test service code that a
  request can reach: return an `IpcError` (`internal`) instead, or show why
  it cannot fail with a comment.

## Verification (foreground; finish before you report)

```sh
cd /home/user/wt/p23a
export CARGO_INCREMENTAL=0 CARGO_TARGET_DIR=/home/user/wt/target-p23a
cargo fmt --all -- --check
cargo clippy -p secs-service -p secs-types -p secs-export -p secs-project -p secs2r-cli --all-targets --locked -- -D warnings
cargo test -p secs-types -p secs-service -p secs-export -p secs-project --locked 2>&1 | grep -E "test result|FAILED|panicked"
cargo test -p secs-policy --locked 2>&1 | grep -E "test result|FAILED"
(cd apps/secs2r/ui && npm ci --no-audit --no-fund && npm run typecheck && npm test)
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps -p secs-service -p secs-types -p secs-export -p secs-project
python3 .github/scripts/upstream.py check-tree
```

## Report

Each of the seven items: what changed and the test that shows it, with the
byte comparisons' results. Anything you could not do.
