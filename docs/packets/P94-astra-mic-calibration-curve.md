# P94 (ASTRA): UMIK-1 calibration files and the response correction in `secs-audio`

Worktree: `E:\Sound-Equalization-and-Correction-System-2R\.claude\worktrees\dirac-liveproject-support-2e310d`
(branch `claude/mic-calibration`). Work only there. Do not commit, do not
push, do not spawn sub-agents. Claude reviews the diff and commits. Nobody
else edits this tree while you work.

Read first: `docs/adr/0015-mic-calibration-in-calculation.md` (the decision),
`CLAUDE.md`, `crates/secs-audio/src/calibration.rs` (the parser you extend),
`crates/secs-audio/tests/level_calibration.rs` (its tests), and
`crates/secs-dsp/src/fft.rs` (`rfft`, `irfft`). REW's file rules, which the
parser follows: <https://www.roomeqwizard.com/help/help_en-GB/html/calfiles.html>
(fetch it; "Only lines which begin with a number are loaded").

## Goal

1. `MicCalibration::parse` reads real miniDSP UMIK-1/UMIK-2 files, including
   the 90-degree file whose second line is a quoted description, and the
   header variant with `AGain`. It still rejects broken data rows.
2. A `ResponseCorrection` that removes the calibration curves attached to a
   measurement from its impulse response, as REW removes them from its
   frequency response. P95 (Daybreak) builds it from the `meterCal`/`scCal`
   arrays of a measurement's source sidecar and applies it before the engine.

## The UMIK-1 file

A UMIK-1 90-degree file is laid out like this (CRLF line ends, tab-separated,
about 600 log-spaced data rows from 10 Hz to 20 kHz, no phase column; serial and
values below are invented):

```
"Sens Factor =-.7312dB, SERNO: 7000001"
"Auto-generated 90-degree calibration file for use with nanoAVR DL"
10.062	-3.8810
10.188	-3.7102
...
20011.300	-6.1500
```

Newer files have `"Sens Factor =4.125dB, AGain =18dB, SERNO: 7000002"` as the
first line, and the 0-degree file has no description line. Today `parse` fails
on the description line (it falls through to the numeric rows and has more
than three columns). Do not commit a vendor file: write synthetic fixtures with
exactly this layout (serial `7000001`, invented gains).

## Parser rules (change `parse`, keep its signature)

Per line, after trimming whitespace and a leading BOM:

- Blank lines and lines starting with `#`, `;`, `//`, `*` are skipped (as now).
- A line wrapped in double quotes is metadata. Strip the quotes. If the text
  starts with `Sens Factor`, read the header (below). Otherwise keep it as a
  note (`notes()`), at most 8 notes, each truncated to 200 chars. Never an
  error.
- An unquoted line starting with `Sens Factor` or `Sensitivity` is read as the
  header, as now. Malformed header values stay errors.
- An unquoted line whose first character is not the start of a number (digit,
  `+`, `-`, `.`) is a heading such as `Freq(Hz) SPL(dB) Phase(deg)`: skip it,
  as REW does. Not a note.
- A line starting with a number is a data row and is parsed exactly as now
  (2 or 3 columns separated by whitespace or commas, a trailing `#`/`;`
  comment allowed, finite values, frequency > 0 and strictly increasing,
  |gain| <= 200). Anything else on such a line is an error with its line
  number. Phase, when present, must be finite.

The `Sens Factor` header: comma-separated fields after the key. `= <value>dB`
is required (as now; `-.7312` must parse). Optional `AGain = <value>dB` and
`SERNO: <text>` (trimmed, at most 64 chars, may be any text). Unknown fields
are ignored. A second `Sens Factor` header is an error (as now).

All existing tests in `level_calibration.rs` must keep passing unchanged,
except the malformed-input list may only gain cases. In particular
`"Sens Factor = broken\n20 0\n1000 0"` and `"20 0 nope\n1000 0"` stay errors.

Add a structured error so the screen can say which line is wrong. `parse`
keeps returning `AudioError` (map the new error into it with the same
message style); add:

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CalibrationIssue {
    TooLarge,          // text over 4 MiB
    Header,            // malformed Sens Factor / Sensitivity, or a duplicate
    Row,               // a data row that is not 2-3 finite numbers
    Order,             // frequency <= 0 or not increasing
    Range,             // |gain| > 200
    TooFewPoints,      // fewer than two data rows
    BothConventions,   // Sens Factor and Sensitivity in one file
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[error("calibration {issue:?} at line {line:?}")]
pub struct CalibrationError { pub issue: CalibrationIssue, pub line: Option<usize> } // 1-based

impl MicCalibration {
    pub fn parse_checked(text: &str) -> Result<Self, CalibrationError>;
    pub fn parse(text: &str) -> Result<Self, AudioError>; // = parse_checked mapped
    pub fn serial(&self) -> Option<&str>;
    pub fn analog_gain_db(&self) -> Option<f64>;
    pub fn notes(&self) -> &[String];
    pub fn has_phase(&self) -> bool; // any point has a phase value
    /// A curve already in arrays (REW's embedded `freqArray`/`gainArray`/
    /// `phaseArray`). Same point rules as `parse`; no header values.
    pub fn from_points(points: Vec<CalibrationPoint>) -> Result<Self, CalibrationError>;
}
```

`from_points` errors use `line: Some(index + 1)` for the offending point.
Re-export the new types from `lib.rs` next to `MicCalibration`.

## `ResponseCorrection` (new, in `calibration.rs`)

```rust
/// The most a correction raises any frequency, as REW limits cal boost.
pub const MAX_CORRECTION_BOOST_DB: f64 = 20.0;

/// IEC 61672-1 C weighting in dB (0 at 1 kHz within 0.01 dB; -inf at 0 Hz).
pub fn c_weighting_db(frequency: f64) -> f64;

/// What REW removes from a measurement's frequency response: the attached
/// calibration curves (mic and soundcard) and, for a C-weighted SPL meter,
/// C weighting.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ResponseCorrection { curves: Vec<MicCalibration>, c_weighted: bool }

impl ResponseCorrection {
    pub fn new(curves: Vec<MicCalibration>, c_weighted: bool) -> Self;
    pub fn is_empty(&self) -> bool; // no curves and not C-weighted
    /// Summed response the correction removes at `frequency`: gain in dB
    /// (curves' `gain_db` plus C weighting when set) and phase in degrees
    /// (curves with phase only; others contribute 0).
    pub fn response(&self, frequency: f64) -> (f64, f64);
    /// The IR with that response removed. Same length as `ir`.
    pub fn apply(&self, ir: &[f32], sample_rate: u32) -> Result<Vec<f32>, AudioError>;
}
```

`apply`, fixed:

- Empty correction: return `ir.to_vec()` (bit-exact).
- Errors (never panics): empty `ir`, `sample_rate == 0`, a nonfinite sample.
- `n = 2 * ir.len()`; zero-pad to `n` in f64; `X = fft::rfft`.
- For each bin `k` in `0..=n/2`, `f = k * sample_rate / n`.
  `(gain, phase) = response(f)`. Correction magnitude in dB is
  `(-gain).min(MAX_CORRECTION_BOOST_DB)` (cuts are unlimited; a `-inf` C
  weighting at 0 Hz becomes the cap). Multiply the bin by
  `10^(dB/20) * exp(-j * phase.to_radians())`. At `k = 0` and at the
  Nyquist bin of even `n`, use phase 0.
- `fft::irfft(&X, n)`, take the first `ir.len()` samples, cast to f32.
- Phase is interpolated like gain (log frequency, endpoint values held);
  put that in a small private helper next to `gain_db`, do not change
  `gain_db`'s behaviour.
- Sequential, no parallelism, f64 throughout (ADR 0007).

## Tests (new file `crates/secs-audio/tests/mic_calibration.rs`, plus fixtures)

Fixtures under `crates/secs-audio/tests/data/` (synthetic, header comment is
impossible in a quoted-first-line file, so name them clearly):
`umik1_90deg_synthetic.txt` (CRLF, tabs, the two quoted lines above with
serial 7000001, at least 20 log-spaced rows 10 Hz–20 kHz, a leading `-.`
value somewhere) and `umik1_again_synthetic.txt` (LF, the `AGain` header, no
description).

Test names (P95 and `features.toml` will cite them):

- `umik1_files_parse_with_serial_gain_and_notes`: both fixtures; sens factor,
  analog gain, serial, notes (the 90° description, none for the other),
  point count, first/last frequency, `has_phase() == false`, `gain_db` at a
  tabulated and an interpolated frequency.
- `rew_headings_are_skipped_and_broken_rows_still_fail`: a heading line
  `Freq(Hz)\tSPL(dB)\tPhase(deg)` and an unquoted note-like line are skipped;
  each `CalibrationIssue` variant is produced with the right 1-based line
  (where a line applies) by a small input.
- `from_points_validates_like_the_parser`.
- `empty_correction_is_bit_exact`.
- `flat_curve_scales_the_ir`: a +3 dB flat curve scales a random IR by
  `10^(-3/20)` within 1e-6 relative; a -40 dB curve is capped at +20 dB.
- `correction_matches_the_curve_on_bins`: a delayed unit impulse (delay
  1000 samples, 48 kHz, 65536 samples) through a non-flat curve (e.g. the
  90° fixture): the output's spectrum magnitude at 31.5 Hz, 1 kHz, 10 kHz and
  16 kHz equals `-gain_db` within 0.05 dB, and the output peak stays at
  sample 1000 (zero phase keeps the arrival).
- `phase_column_is_removed`: a curve with constant +90° phase turns a
  delayed impulse's bin phase by -90° (check one mid bin).
- `c_weighting_matches_iec_table`: 31.5 Hz -3.0, 63 Hz -0.8, 1 kHz 0.0,
  8 kHz -3.0, 10 kHz -4.4 (tolerance 0.1 dB), and a C-weighted correction
  without curves boosts 31.5 Hz by about 3 dB.
- `bad_inputs_error_without_panics`: empty IR, rate 0, NaN sample.

## Allowed files

`crates/secs-audio/src/calibration.rs`, `crates/secs-audio/src/lib.rs`,
`crates/secs-audio/tests/mic_calibration.rs` (new),
`crates/secs-audio/tests/data/umik1_*_synthetic.txt` (new),
`crates/secs-audio/tests/level_calibration.rs` (only to add malformed cases).
`crates/secs-audio/Cargo.toml` only if you truly need `thiserror` derive
features already present (it is a dependency). Nothing else; Claude edits
`features.toml`, docs and the changelog.

## Verify (foreground; do not end your turn before they finish)

```sh
cargo fmt --all -- --check
cargo clippy -p secs-audio --all-targets --locked -- -D warnings
cargo test -p secs-audio --locked
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps -p secs-audio --locked
```

Also run this once against the real file on this machine and report the
printed line (do not copy the file into the repo):

```sh
cargo test -p secs-audio --locked --test mic_calibration -- --ignored --nocapture real_umik1_file
```

where `real_umik1_file` is an `#[ignore]` test that reads the file named by
the `SECS_REAL_MIC_CAL` environment variable (Claude gives the path in the
task, not in the repository; skip with a message when it is unset or missing)
and prints serial, sens factor, notes, point count and `gain_db(10000.0)`.

## Report

Changed files, the output of each command above (the tail is enough), the
real-file line, and anything you could not do.
