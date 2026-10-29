# P99 (Daybreak): make the internal crates publishable on crates.io

Worktree: `E:\Sound-Equalization-and-Correction-System-2R` (branch
`claude/public-api`). Work only there. Do not commit, do not push, do not
spawn sub-agents, and **never run `cargo publish` without `--dry-run`**.
Claude reviews the diff and commits.

Read first: `CLAUDE.md`, `docs/adr/0016-public-rust-api-and-local-api.md`
(the decision this packet implements), `docs/adr/0002-unsafe-isolation-and-policy-gates.md`,
the workspace `Cargo.toml`, every `crates/*/Cargo.toml`,
`crates/secs-service/src/lib.rs`, `crates/secs-mdat/src/lib.rs`,
`crates/secs-engine/src/lib.rs` and `crates/secs-engine/src/test_support.rs`,
`crates/secs-policy/tests/`.

## Why

ADR 0016 publishes the calculation engine on crates.io behind one facade
crate (written later, in P101). crates.io only accepts crates whose every
normal, build and optional dependency is itself on crates.io with a version
requirement, and it builds the packaged files alone. So the 15 internal
crates below must be publishable first, as a set with one shared version that
is separate from the app's version.

Published set (exactly these 15; the facade joins later):
`secs-types`, `secs-dsp`, `secs-compute`, `secs-engine`, `secs-jos`,
`secs-mdat`, `secs-project`, `secs-dirac`, `secs-export`, `secs-graph`,
`secs-camilla`, `secs-filters`, `secs-rew`, `secs-audio`, `secs-service`.

Never published (keep `publish.workspace = true`, which is `false`, and the
workspace version): `secs-sys`, `secs-gpu`, `secs-cuda-sys`, `secs-oracle`,
`secs-policy`, `apps/secs2r`, `apps/secs2r-cli`.

## Rules (fixed)

1. **Manifests of the published set.**
   - `version = "0.1.0"` written in the crate (not `version.workspace`).
     The workspace `[workspace.package] version` stays the app version and
     the release workflow keeps bumping only that.
   - `publish` allowed (drop `publish.workspace = true`). `edition`,
     `rust-version`, `license` (`GPL-3.0-only`) and `repository` stay
     inherited from the workspace. Add `homepage` (the repository URL),
     `readme = "README.md"`, `keywords` (at most 5, ASCII, at most 20 chars
     each, chosen per crate) and `categories` (valid crates.io slugs, for
     example `multimedia::audio`, `science`, `encoding`, `parser-implementations`).
     Keep the existing `description`; make it say what the crate does in one
     sentence if it does not.
   - `include = [...]` listing only `Cargo.toml`, `README.md`, `LICENSE`,
     `src/**`, a `build.rs` if one exists, and the data files the crate needs
     to build (rule 3). No `tests/`, `examples/`, `benches/`, fixtures or
     goldens.
   - Every dependency between two published crates is a normal dependency
     with an exact requirement `=0.1.0`. Put those in `[workspace.dependencies]`
     as `{ path = "crates/<name>", version = "=0.1.0" }` and refer to them
     with `<name>.workspace = true` (plus `features = [...]` when needed)
     from every crate, including the unpublished ones and the apps. No crate
     keeps a bare `{ path = "../<name>" }` for a published crate.
     Dev-dependencies on unpublished crates (`secs-oracle`, path only) are
     fine: cargo strips them when publishing.
2. **README.md and LICENSE in each published crate.** `README.md` is short:
   what the crate holds (one paragraph), that it is an internal crate of
   SECS 2R whose public items may change incompatibly in any release, that
   the supported API is the crate `sound-equalization-and-correction-system-rust-api`,
   the repository link and `GPL-3.0-only`. Write it in English. `LICENSE` is a
   byte-identical copy of the repository root `LICENSE` (GPL-3.0 requires the
   licence text to travel with the source).
3. **No reads outside the crate directory in code that is compiled when the
   crate is built as a dependency.** Test-only code (`#[cfg(test)]` modules,
   `tests/`, examples) may keep its paths. Known cases:
   - `crates/secs-service/src/lib.rs` includes `../../../UPSTREAM.toml`. Put
     a byte-identical copy at `crates/secs-service/UPSTREAM.toml` and include
     that.
   - `crates/secs-mdat/src/lib.rs` includes the upstream tree's
     `src/secs/rew_mdat/template_1.mdat`. Put a byte-identical copy at
     `crates/secs-mdat/data/template_1.mdat` and include that. The upstream
     tree itself is never edited.
   - Search all 15 crates for any other `include_str!`, `include_bytes!`,
     `#[path]`, `env!("CARGO_MANIFEST_DIR")` or build-script read that leaves
     the crate in non-test code, and fix it the same way.
4. **No published feature depends on an unpublished crate.** `secs-engine`'s
   `test-support` feature pulls the optional `secs-oracle`, which blocks
   publishing. Restructure so that the published `secs-engine` manifest has
   no dependency on `secs-oracle` other than a dev-dependency, while every
   current user of `test-support` (`secs-export`, `secs-gpu`, `secs-service`
   dev-dependencies, the `engine_perf` example, tests) keeps working with the
   same goldens and tolerances. Prefer moving the oracle-reading helpers out
   of `secs-engine` (into `secs-oracle`, or a new unpublished crate such as
   `secs-engine-testkit` with `#![forbid(unsafe_code)]` and
   `[lints] workspace = true`) and exposing only what they need from
   `secs-engine` behind a `#[doc(hidden)]` module that compiles without
   `secs-oracle`. Do not weaken or delete any test. Check the other
   published crates for the same problem.
5. **App version vs library version.** `secs_service::VERSION` is
   `env!("CARGO_PKG_VERSION")`, which becomes `0.1.0`. The bootstrap screen
   and anything else that shows or writes "the SECS 2R version" must keep
   showing the app version:
   - add `pub app_version: std::borrow::Cow<'static, str>` to
     `secs_service::Deps`, documented as the product version shown to users
     and written into exported files; `Deps::default()` uses the library
     version;
   - `apps/secs2r/src/main.rs` and `apps/secs2r-cli/src/main.rs` pass their
     own `env!("CARGO_PKG_VERSION")`;
   - keep `pub const VERSION` but document it as the library version; switch
     every user-visible use to `deps.app_version` (bootstrap `version` and
     `prerelease`, and any other place you find), and fix the tests that
     compared against `VERSION`.
6. **Policy gates** in `crates/secs-policy/tests/` (new test file
   `publishing.rs`), reading manifests from disk like the existing gates:
   - the crates whose manifest allows publishing are exactly the published
     set above (the gate holds the list; P101 will add the facade);
   - they all have the same literal version, and every dependency from one
     published crate to another is `=<that version>`;
   - each has `README.md`, `LICENSE` byte-identical to the root `LICENSE`,
     `readme`, `keywords`, `categories`, `description`, and an `include`
     without `tests`, `examples` or `benches`;
   - `crates/secs-service/UPSTREAM.toml` equals the root `UPSTREAM.toml` and
     `crates/secs-mdat/data/template_1.mdat` equals
     `src/secs/rew_mdat/template_1.mdat`, byte for byte;
   - no published crate has a non-dev dependency (optional included) on an
     unpublished workspace crate.

## Files

You may edit: the workspace `Cargo.toml`, `Cargo.lock` (as cargo changes
it), every `crates/*/Cargo.toml`, new `README.md`, `LICENSE` and data copies
inside the 15 published crates, `crates/secs-service/src/**`,
`crates/secs-service/tests/**`, `crates/secs-mdat/src/lib.rs`,
`crates/secs-engine/**`, `crates/secs-oracle/**`, a new
`crates/secs-engine-testkit/**` if you choose it, other crates' tests that
use `test-support`, `crates/secs-policy/**`, `apps/secs2r/src/main.rs`,
`apps/secs2r-cli/src/main.rs`.

Do not touch: the upstream tree (`src/`, `tests/`, `tools/`, `배포/`, root
`LICENSE`, `UPSTREAM.toml` and the rest listed in `CLAUDE.md`), `.github/`,
`docs/`, `features.toml`, `apps/secs2r/ui/`, `CHANGELOG.md`.

## Verification (foreground, all must pass; do not end the turn before they finish)

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo publish --workspace --dry-run --locked --allow-dirty
python .github/scripts/check_feature_tests.py
python .github/scripts/upstream.py check-tree
```

and, for each of the 15 published crates,
`RUSTDOCFLAGS="-D warnings" cargo doc --no-deps -p <crate>` (in PowerShell:
`$env:RUSTDOCFLAGS='-D warnings'; cargo doc --no-deps -p <crate>`).
`cargo publish --workspace --dry-run` packages and builds every publishable
member against the packaged files only; if your cargo cannot do that for
crates not yet on crates.io, say so and run `cargo package --workspace
--locked --allow-dirty` instead. Also run `cargo package --list -p <crate>`
for each and report the packaged size of the largest crate (crates.io limit
10 MB).

If an API you need is uncertain (cargo flags, manifest keys, crates.io
category slugs), check the official documentation before using it.

## Report

Changed and added files grouped by rule; how you resolved rule 4 and why;
every other out-of-crate read you found; the tail of each verification
command's output; the packaged size table; anything left undone.
