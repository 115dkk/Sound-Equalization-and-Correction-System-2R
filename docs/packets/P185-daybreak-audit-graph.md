# P185 (Daybreak): audit #186, Category A, the export graph and the writers

Tree: `E:\wt\audit-graph` (git worktree of the repository, branch `audit/186-graph`).
Set `CARGO_TARGET_DIR=C:\cargo-target\audit-graph` for every cargo command
(the main checkout's target directory is not yours). Work only in that tree.

Allowed files: `crates/secs-graph/**`, `crates/secs-filters/**`,
`crates/secs-camilla/**`, `crates/secs-export/**`, `crates/secs-apply/**`,
`crates/secs-project/src/irs.rs` and its tests, `crates/secs-mdat/src/source_info.rs`
and its tests, `features.toml`. Forbidden: `crates/secs-service/**`,
`crates/secs-engine/**`, `crates/secs-types/**`, the UI, `oracle/**`, the
upstream Python tree. If an item cannot be finished inside the allowed files,
stop that item, report why, and continue with the next one.

Do not push. Do not spawn sub-agents. Commit on `audit/186-graph` only.

Read first: `CLAUDE.md` (gates, commit prefixes, Korean commit text),
`CONTEXT.md` (terms), `docs/adr/0004-upstream-parity.md` (goldens are the
contract), `crates/secs-graph/src/lib.rs` (GraphError, ExportGraph),
`crates/secs-graph/src/validate.rs`, `crates/secs-graph/src/plan.rs`,
`crates/secs-graph/src/files.rs`, `crates/secs-graph/tests/graph.rs`
(`every_rule_rejects_its_input_and_names_the_field`),
`crates/secs-filters/src/lib.rs` and `render.rs`,
`crates/secs-camilla/src/pyfmt.rs`, `crates/secs-apply/src/camilla.rs`,
`crates/secs-project/src/irs.rs`, `crates/secs-mdat/src/source_info.rs`.
The audit's full text for each finding (TD-xxx) is in issue #186; the
"Finding" lines below paraphrase it. The audit was written against main
`97a27818`, so line numbers may have drifted by a few lines.

## Why

The monthly audit (issue #186) found places where a hand-built or revived
export plan, an out-of-range filter value, or a stray newline is caught only
after files are written, or not at all (a panic, a silently shared WAV, a
bundle installed through a symlink). Every item here is a behaviour change at
a public boundary of a published crate, so each one is done test-first at
the seam named below: write the failing test, run it and confirm it fails
for the reason the finding describes, write the minimum code, run it again,
tidy only the lines this cycle wrote, run the item's gate, commit.

## Rules for every item

- One finding, one commit. Subject `fix(audit): A-n <한국어 설명> (#186 TD-xxx)`;
  body in Korean saying what the Red test proves and what changed; last line
  `Co-Authored-By: Claude Mythos 5.1 <noreply@anthropic.com>`. Nothing else
  goes in the commit (no drive-by edits, no formatting of untouched files).
- The Red test's expected values come from literals and the specification
  (the finding, the ADR, the crate docs), never from running the code under
  test. Assert the effect (the error variant and its field name, the file
  that is or is not written), not that a call succeeded.
- Gate per item, all in the foreground, all passing before the commit:
  `cargo fmt --all -- --check`,
  `cargo clippy -p <touched crates> --all-targets --locked -- -D warnings`,
  `cargo test -p <touched crates> --locked`.
  The golden tests of `secs-camilla`, `secs-filters` and `secs-export` must
  stay bit-identical: a golden that changes means the item changed shipped
  output and is reported as blocked, not "updated" (A-45 is the one stated
  exception).
- Every fifth commit, and at the end: `cargo test --workspace --locked`.
- Register each new test in `features.toml` next to the entry it belongs to
  (`export.camilladsp`, `export.eqapo`, `export.filter_text`,
  `export.convolution`, `format.irs_store`, `format.mdat_source_info`,
  `apply.camilla_ws`; see the ids in the file). `python3 .github/scripts/check_feature_tests.py`
  must accept the names (on Windows it also lists two known missing tests
  unrelated to you; ignore those two only).
- Tidy touches only lines this cycle wrote. Structural clean-up elsewhere is
  another audit item's business.

## Items, in order

### A-19 (TD-182): range rules in the graph validation

Finding: graph validation checks PEQ, HP, LP, XO and `low_hz` only for
finiteness, so negative or above-Nyquist frequencies and `q <= 0` pass and
CamillaDSP rejects the YAML only after every file is written, against the
camilla module's contract (lib.rs:12, every check before the first file).

Seam: `ExportGraph::build` (through `validate::graph` and `validate_mimo`).
Red: new cases in `crates/secs-graph/tests/graph.rs::every_rule_rejects_its_input_and_names_the_field`
in the same shape as the existing ones: a PEQ at 0 Hz, a PEQ at 24 000 Hz, a
PEQ with `q = 0`, an all-pass with `q = -1`, a sub `hp_hz = -20`, a sub
`lp_hz = 30 000`, a main `xo_hz = 0`, a MIMO path `low_hz = 48 000`, each
rejected with `GraphError::Invalid` whose message names the field exactly as
the finite checks already do (`main.L.top_peq[0].frequency`,
`sub.SW1.allpass[0].q`, `mimo.paths[0].low_hz`, ...). Expected rule, fixed
here: every frequency `0 < f < 24_000` (strictly inside, in hertz at the
48 kHz design rate), every PEQ and all-pass `q > 0`. Gains keep the finite
check only.
Green: two helpers next to `finite` in `validate.rs` (`frequency(name, hz)`,
`positive_q(name, q)`) called from `peqs`, `allpasses`, the sub `hp_hz`/`lp_hz`,
the main `xo_hz`, the MIMO `low_hz` and the LFE `lp_hz`.
Tidy: naming inside those helpers only. Goldens: the oracle chains are all in
range, so no golden changes; if one does change, stop and report.

### A-18 (TD-180): writers return an error on a malformed plan

Finding: the public writers (`windows_channels`, `eqapo_filters`,
`filter_text`, `render_paths`, `plan_responses`) accept a `FilterPlan` the
crate docs say may be hand-built or revived, but a structurally wrong plan
(an input without `main_ops`, a FIR name missing from `firs()`, a support
missing from `outputs`) panics in `.expect(...)`.

Seam: `FilterPlan::check_structure(&self) -> Result<(), GraphError>` in
`crates/secs-graph/src/plan.rs`, called where `validate_plan_delays` is
called in `secs-filters` (and by `plan_responses` / `headroom_db` in
secs-graph), so every public writer answers `Err` instead of panicking.
Red (secs-graph): a unit test in `plan.rs` that takes a plan from
`ExportGraph::build(...)` on a small chain, then breaks it three ways in
three sub-cases (removes an `(input, ops)` entry from `main_ops`; renames
one `Op::Fir { name }` so no FIR of that name exists among `firs()`; adds a
MIMO feed whose support is not in `outputs`) and asserts
`Err(GraphError::Invalid(message))` with the message naming the input, the
FIR name or the support. Red (secs-filters): a test in
`crates/secs-filters/tests/` that calls `eqapo_filters`, `filter_text` and
`render_paths` with the plan missing a `main_ops` entry and asserts
`Err(FiltersError::Invalid(_))` (after A-44 it becomes
`FiltersError::Graph(GraphError::Invalid(_))`; write it against `Invalid` now
and let A-44 update the match in its own commit) instead of a panic. Use
`std::panic::catch_unwind` only to prove the Red fails for the right reason
if you need to; the final test must not need it.
Green: implement `check_structure` (inputs ⊆ keys of `input_ops` and
`main_ops`; every `send_ops`/`subs` role ⊆ `outputs`; every MIMO feed's
support ⊆ `outputs`; every `Op::Fir`/`Op::FirLowRate` name reachable from
`firs()`), call it from `validate_plan_delays`'s callers in `secs-filters`
(`windows_channels` is infallible by signature: give it a fallible twin or
make the writers that call it validate first; do not change its signature
if an external caller exists in `secs-export` or the CLI), and turn the
listed `.expect` sites (`lib.rs` 263, 281, 287, 336, 343, 370, 1036, 1050,
1163, 1170; `render.rs` 372) into `?` on an error that names the missing
item. Do not change any golden output.

### A-51 (TD-266): FIR file-name collisions are refused

Finding: roles may contain `_`/`-`, FIR file names are joined with `_`
(`{role}_fir`, `mimo_{input}_{support}`) and `firs()` deduplicates by name
keeping the last taps, so roles `a_b`+`c` and `a`+`b_c` silently share one
WAV. The app cannot reach it (fixed role names) but the public
`ExportGraph::build` can.

Seam: `ExportGraph::build` (through `plan::make`).
Red: a case in `every_rule_rejects_its_input_and_names_the_field` (or a
sibling test in `tests/graph.rs`): mains `a_b` and `a` with MIMO paths
`a_b → c` and `a → b_c` (both with a FIR) is rejected with
`GraphError::Invalid` naming the colliding name (`mimo_a_b_c`), before any
plan exists. A second case: two mains whose `{role}_fir` names cannot collide
(`L`, `R`) still build.
Green: in `plan::make`, collect every FIR name as it is pushed; on the second
occurrence return `Err` (so `make` becomes fallible, or `build` checks the
roles up front with the same rule; choose the one that keeps `make` pure and
say which in the commit body). Roles are validated in `validate::identifier`;
rejecting `_` in roles there would change which chains export and is not the
chosen fix.

### A-54 (TD-270): a symlinked parent under the export root is refused

Finding: the install refuses a real folder in a file's place, but
`create_dir_all(parent)` and the rename follow a symlinked sub-folder under
the user's export root (`coeffs/` pointing elsewhere), so the bundle lands
beyond the link.

Seam: `write_bundle_observed` / `install` in `crates/secs-graph/src/files.rs`.
Red: a test next to `failed_second_install_restores_every_old_file` that
creates `root/coeffs` as a symlink to a folder outside `root` (on Windows
`std::os::windows::fs::symlink_dir`; when the OS refuses the link the test
must fail loudly with that error, not skip; CI's Windows runner allows it and
so does this machine with Developer Mode or elevation. If it fails here for
lack of privilege, use a directory junction via `mklink /J` through
`std::process::Command` and say so in the commit body) and asserts that the
install returns `Err` with `ErrorKind::InvalidInput` naming the link, that
nothing landed beyond the link and that the staging folder is gone.
Green: before `create_dir_all(parent)` and the rename, walk each destination's
ancestors below `root` with `symlink_metadata`, like `crates/secs-project/src/irs.rs:286-296`
does, and refuse `is_symlink()` (and the reparse-point attribute on Windows).
A pre-existing real folder stays allowed.

### A-44 (TD-225): no panic past the RIFF limit, graph errors keep their kind

Finding: `wav_bytes` panics (`u32::try_from(data.len()*4).expect("FIR WAV fits u32")`)
when a hand-made plan exceeds 1 GiB, while `secs-project`'s WAV writer
returns the same overflow as an error; `From<GraphError> for FiltersError`
flattens `NonAscii`/`NonFinite`/`Invalid` into `Invalid(String)`, so the
service cannot classify like it does for CamillaDSP; `stage_items` and
`before_after_items` are `pub` and panic with
`.expect("validated engine export inputs")` while their `_mimo` twins return
`Result`.

Seams: `secs_graph::files::wav_bytes`, `FiltersError`, `secs_export::{stage_items, before_after_items}`.
Red 1: factor the limit into `pub fn wav_data_len(samples: usize) -> Result<u32, GraphError>`
and test it at `(u32::MAX - 36) / 4 + 1` samples →
`Err(GraphError::Invalid("FIR WAV exceeds RIFF limit"))`; `wav_bytes`
calls it first, before any allocation (no test allocates a gigabyte; `unsafe`
is forbidden in these crates).
Red 2: `FiltersError::from(GraphError::NonFinite("x".into()))` matches
`FiltersError::Graph(GraphError::NonFinite(_))` (new variant
`#[error(transparent)] Graph(#[from] GraphError)`), and the three old
mappings' texts stay reachable through `Display`. `secs-service` is forbidden
to you, so keep `Invalid(String)` for every other producer; only the
`From<GraphError>` changes.
Red 3: `stage_items`/`before_after_items` return `Result<Vec<MeasurementInput>, ExportError>`;
the test builds the inputs that make `*_with_mimo` fail today and asserts
`Err`. Callers: the tests and `examples/export_perf.rs`. If `secs-service`
calls either function, keep the old names as thin wrappers that return the
`Vec` and add `try_stage_items` / `try_before_after_items` instead, and say
so in the report so the main session can switch the service in its own
commit.

### A-43 (TD-224): newlines in comment inputs are rejected

Finding: header-comment inputs are only filtered for ASCII. `Options.note`
keeps `\n`/`\r`, `sub.mode` is checked for ASCII and then emitted unquoted on
a `#` line, and `FilterRun.note` is not seen by `validate_run`; a newline ends
the comment and can inject a YAML or APO line. Today only engine- and
service-made strings reach these inputs.

Seams: `secs_camilla::validate_options` (note), `secs_graph::validate::graph`
(sub mode), `secs_filters::validate_run` (run.note).
Red: three cases, one per seam: a note `"a\nb"` → `CamillaError::Invalid`
naming `options.note`; a sub mode `"fixed\r"` → `GraphError::Invalid` naming
`sub.<role>.mode`; a `FilterRun.note` with `\n` → `FiltersError::Invalid`
naming `note`. Each asserts that no file exists afterwards (use a temp
folder and list it).
Green: the same `contains(['\n', '\r'])` rule `validate_options` already
applies to `mode` (camilla lib.rs 455), applied to the three inputs.

### A-45 (TD-226): convolution guide numbers through pyfmt

Finding: the convolution guide uses Rust `{:.1}`/`{:+.1}`/`{:.2}` while every
other human-facing number in the writers goes through `pyfmt::format_fixed`
(delays are 3 digits in the APO, text and Camilla comments, 2 here).

Seam: the guide text `secs_filters` writes (`convolution_guide`, read through
the public convolution export).
Red: a test in `crates/secs-filters/tests/` that exports a convolution bundle
for a path with `delay_ms = 1.2345` and `peak_gain_db = 2.25` and asserts the
guide line reads `... | +2.2 dB | 1.234 ms` (Python's `format(2.25, '+.1f')`
is `+2.2` by half-to-even and `format(1.2345, '.3f')` is `1.234`; verify both
with `python3 -c` before writing them into the test and quote the command in
the test's comment). Also the headroom line: `format_fixed(headroom_db, 1, false)`.
Green: replace the four `{:.1}`/`{:+.1}`/`{:.2}` sites in `convolution_guide`
with `format_fixed(value, places, plus)`. The CSV lines (`{:.6}`, `{:.3}`) are
machine-readable and stay as they are; say so in the commit body. The
convolution goldens compare files: if a golden guide text changes, that is
the intended change of this item and the golden is updated in the same
commit with the diff quoted in the body (this is the one item where a golden
may change; nothing else may).

### A-52 (TD-267): pyfmt refuses to format NaN and infinity as digits

Finding: `format_fixed`/`format_g` have no non-finite guard; NaN/inf go
through `parts()` and ryu's `format_finite` and come out as a long digit
string instead of `nan` (`repr_f64` alone guards). Every current caller
validates first.

Seam: `secs_camilla::pyfmt::{format_fixed, format_g}`.
Red: unit tests in `pyfmt.rs`: `format_fixed(f64::NAN, 2, false) == "nan"`,
`format_fixed(f64::INFINITY, 1, true) == "+inf"`, `format_fixed(f64::NEG_INFINITY, 3, false) == "-inf"`,
`format_g(f64::NAN, false) == "nan"` (Python: `format(float('nan'), '.2f')`
is `nan`, `format(float('inf'), '+.1f')` is `+inf`; quote the python3 check).
Green: the same early returns `repr_f64` has (lines 81-93), at the top of both
functions.

### A-46 (TD-228): the WAV reader and the mdat integer box reject what they cannot represent

Finding: `read_mono_float_wav` returns `sample_rate == 0` unchecked;
`read_wav_float32` ignores `channels`/`block_align` and reads a stereo
float32 file as interleaved mono (crates.io API, today used by tests and
fuzz only); `Value::Int(value as i32)` truncates a `Boxed::Integer(i64)`
that comes in through the public `SourceInfo` API.

Seams: `secs_project::irs::{read_mono_float_wav, read_wav_float32}` and
`secs_mdat` box building (the public `build` or `Boxes` input validation).
Red: in `irs.rs` tests, a 48-sample float32 WAV with `sample_rate = 0` →
`Err(ProjectError::Wav(_))` from both readers; a stereo float32 WAV (channels
2, block_align 8) → `read_wav_float32` returns `Err` naming the channel count
instead of 96 interleaved samples. In `secs-mdat`, `Boxed::Integer(i64::MAX)`
through the public API → `Err(MdatError::Input(_))`, and `Boxed::Integer(7)`
still becomes `Value::Int(7)`.
Green: `wav_chunks` rejects `sample_rate == 0`; `read_wav_float32` requires
`channels == 1 && block_align == 4`; `i32::try_from` → `MdatError::Input`
at the input-validation point of the public API (not in `make`, which is
infallible by design; validate before it is called).

### A-33 (TD-201): one deadline for a whole status call

Finding: `apply_status` sends up to ten WebSocket commands (2 s each) and a
named-pipe call in sequence on top of an IPC request, so a stuck CamillaDSP
holds one status request for about 20 s; the screen asks for it after every
edit and every 3 s.

Seam: `secs_apply::camilla::Camilla::status` (a transport whose requests
share one deadline).
Red: a unit test next to `status_reads_v4_replies` with a `FakeTransport`
that answers the first command and then blocks (sleeps) longer than the
per-command timeout on every later command; assert that `status` returns
`Err(ApplyError::...TimedOut...)` within `timeout + a small margin` measured
with `Instant` (use a 200 ms timeout in the test so the whole test runs under
a second), where today it takes about ten times the timeout.
Green: `Camilla::status` computes `deadline = Instant::now() + timeout` once
and every command inside it uses that deadline (`WsTransport` already has
`DeadlineStream::set_deadline`; thread a `request_until(deadline)` through
the `Transport` trait with a default that calls `request`). The
`apply_status` caller in the service is forbidden to you and needs no change.

## Report (in your final message, nothing else)

For each item, in order: the commit hash on `audit/186-graph`, the Red test
names and what they assert, the Green summary in one line, the gate output
summary (fmt, clippy, the crate test counts, the workspace run when it ran),
and whether a golden changed (which, why). Then a list of anything blocked or
reclassified (which item, why, what you did instead), and the `git log --oneline main..audit/186-graph`.
