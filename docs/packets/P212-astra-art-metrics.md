# P212 (ASTRA): ART gate numerics, `art_metrics.py`

Worktree `E:\wt\art-gate`, branch `claude/art-gate`. No Rust build is needed
and none may be started. Daybreak works in the same worktree on
`.github/scripts/art_gate.py` and `test_art_gate.py` (P213); do not open or
edit those. Your files, and the only ones you may create or edit:

- `.github/scripts/art_metrics.py`
- `.github/scripts/test_art_metrics.py`

Do not commit, do not push. Run every command in the foreground and end your
turn only after the report. Python 3.13, `numpy` only (2.4.6 is installed; no
scipy, no other third-party import). Read `docs/adr/0026-art-reference-gate.md`
first: the '보완' section at its end is the contract this packet implements.

**Privacy rule (ADR 0026).** The real measurements are private and are not in
this worktree. Work only with synthetic signals you generate in the tests.
Do not look for measurement files elsewhere on this machine. The module must
never print a metric value, and its exception messages must not carry sample
or metric values (names of inputs, outputs and files are fine).

## What the module does

The gate (P213) runs the SECS 2R CLI on a private project, gets a
`convolution` export, and calls this module to (1) predict the corrected
response at the evaluation microphone, (2) compute the same metrics on the
predicted response and on a Dirac ART measurement, (3) judge them similar or
not. Everything here is pure computation on arrays and files handed in.

## Data shapes

```python
class Ir(NamedTuple):
    samples: np.ndarray   # float64, mono
    t0: float             # time in seconds of samples[0] on the measurement's clock

FS = 48000                # every file must be 48 kHz; anything else: ValueError("sample rate")
```

`case.json` (written by P213's `pack`, read here through `score_case`):

```json
{
  "schema": 1,
  "project": "project/case.secs",
  "inputs": ["L", "R", "Ls", "Rs"],
  "eval": {"raw": {"L": {"wav": "eval/L.wav", "t0": -0.25, "gain_db": 0.0}, "SW1": {"wav": "eval/SW1.wav", "t0": -0.25}}},
  "art": {"L": {"wav": "art/L.wav", "t0": -1.0}, "R": {"wav": "art/R.wav", "t0": -1.0}},
  "phase_pairs": [["L", "R"]]
}
```

`eval.raw` is keyed by export **output** name and holds the uncorrected
response of that one output at the evaluation microphone. `gain_db` is
optional (default 0) and is applied when loading. `art` is absent in
input-only cases; it is keyed by **input** name. `phase_pairs` is optional
(default empty).

The convolution export folder (written by `secs2r-cli export --kind
convolution`) holds `routing.json` and one mono WAV per path:

```json
{"schema": 1, "sample_rate": 48000, "length": 65536, "format": "f32", "headroom_db": 11.5,
 "inputs": ["L", "R", "Ls", "Rs", "LFE"], "outputs": ["L", "R", "Ls", "Rs", "SW1", "SW2"],
 "paths": [{"input": "L", "output": "L", "wav": "L_to_L.wav", "csv": "L_to_L.csv", "peak_gain_db": 6.6, "delay_ms": 3.0}]}
```

Each WAV is IEEE float32 (format tag 3), mono, 48 kHz, the causal FIR from
that input to that output with every gain and delay already in it. A path
that is absent contributes nothing.

## Public functions (exact names and signatures)

```python
def read_wav(path: Path) -> tuple[np.ndarray, int]
```

Mono WAV to float64 samples and sample rate. Support format tag 3 (float32
and float64) and tag 1 PCM 16/24/32 bit, plus `WAVE_FORMAT_EXTENSIBLE`
wrapping either. Walk the RIFF chunks (do not assume a 44-byte header). More
than one channel, a truncated file or an unknown format: `ValueError` naming
the file name only.

```python
def load_ir(case_dir: Path, entry: dict) -> Ir
```

`entry` is `{"wav": relative path, "t0": float, "gain_db": optional float}`.
The resolved path must stay inside `case_dir` (reject otherwise). Applies the
gain. Rate must be `FS`.

```python
def load_export(folder: Path) -> dict
```

Reads `routing.json`, checks `schema == 1` and `sample_rate == FS`, returns
`{"inputs": [...], "outputs": [...], "fir": {(input, output): np.ndarray}}`.
WAV names come from the file; they must be plain file names inside `folder`.

```python
def predict(export: dict, raw: dict[str, Ir], inputs: list[str]) -> dict[str, Ir]
```

For each requested input: `y = sum over outputs of conv(fir[input, output],
raw[output])`. Put all raw IRs on one clock first: `t_ref = min(t0)`, each
raw is delayed by `round((t0 - t_ref) * FS)` samples of leading zeros, the
result's `t0` is `t_ref`. Every output listed in `export["outputs"]` that has
at least one path from a requested input must be present in `raw`, else
`ValueError` naming the output. An input that is not in `export["inputs"]`:
`ValueError` naming it. Use FFT convolution; the result length is the longest
shifted raw plus the FIR length minus one.

```python
def channel_metrics(ir: Ir) -> dict[str, float]
```

Keys, always all present, `float("nan")` when a value cannot be computed:
`arrival_63`, `arrival_80`, `arrival_100` (ms), `gd_excess_30_60`,
`gd_excess_60_100`, `gd_excess_100_150` (ms), `pre_63`, `pre_125` (dB),
`post20_80_120` (ms), `edt_63_125` (s). Definitions below.

```python
def pair_phase(a: Ir, b: Ir) -> float
```

Degrees, definition below.

```python
def score_case(case_dir: Path, export_dir: Path, case: dict) -> dict
```

Returns
`{"predicted": {input: metrics}, "predicted_phase": {"A/B": deg}, "art": {input: metrics} or None, "art_phase": {"A/B": deg} or None}`.
`predicted` covers `case["inputs"]`. `art` covers the inputs present in
`case["art"]` (None when the key is absent). The phase dicts cover
`case["phase_pairs"]` (pairs whose two inputs are available on that side).

```python
TOLERANCE = {"arrival": 3.0, "gd_excess": 3.0, "pre": 6.0, "post20": 20.0, "edt": 0.03, "phase": 25.0}
GROUPS = {
    "arrival": ("arrival_63", "arrival_80", "arrival_100"),
    "gd_excess": ("gd_excess_30_60", "gd_excess_60_100", "gd_excess_100_150"),
    "pre": ("pre_63", "pre_125"),
    "decay": ("post20_80_120", "edt_63_125"),
}
CROSS_ROOM_GROUPS = ("arrival", "gd_excess", "pre")
CROSS_ROOM_FACTOR = 1.5

def tolerance_of(metric: str) -> float          # by prefix: arrival_, gd_excess_, pre_, post20_, edt_; "phase:A/B" -> phase
def judge(predicted, reference, groups: Iterable[str]) -> dict[str, str]
def judge_phase(predicted_phase, reference_phase) -> dict[str, str]     # keys "phase:A/B"
def pool(references: list[dict]) -> dict[str, float]
def judge_pooled(predicted, pooled: dict[str, float], groups: Iterable[str]) -> dict[str, str]
def grade(verdicts: dict[str, str]) -> str
```

- Verdict strings are exactly `"similar"`, `"different"`, `"unknown"`.
- `judge`: for each metric of each named group, take the inputs present in
  both `predicted` and `reference` whose two values are finite;
  `median(|predicted - reference|) <= tolerance` is `"similar"`, larger is
  `"different"`, no usable input is `"unknown"`. An unknown group name:
  `ValueError`.
- `judge_phase`: per pair present in both with finite values, the same rule
  with the phase tolerance.
- `pool`: each element is one ART case's `{input: metrics}`; the result is,
  per metric, the median of all finite values over all inputs of all cases
  (metric absent from the result when there is none).
- `judge_pooled`: per metric of the named groups, restricted to
  `CROSS_ROOM_GROUPS` (a group outside it: `ValueError`),
  `median over inputs of |predicted - pooled[metric]| <= tolerance * CROSS_ROOM_FACTOR`;
  `"unknown"` when the pooled value or every predicted value is missing.
- `grade`: among verdicts that are not `"unknown"`, the share of
  `"similar"`: at least 0.75 `"close"`, at least 0.40 `"fair"`, else
  `"far"`; nothing decided: `"none"`.

## Metric definitions

These are definitions, not code to copy from anywhere: write the numerics
yourself and keep each step a small named function with a docstring that
states the definition.

**Segments.** `i0 = argmax(|samples|)`. Segment A is the samples from
`i0 - 0.30 s` to `i0 + 1.00 s`; segment B from `i0 - 0.30 s` to
`i0 + 1.65 s`. Where the IR has no data the segment is zero. Fade the
available data in over its first 50 ms and out over its last 100 ms
(raised cosine) so that the cut makes no click. The time axis of a segment
is `t = (n - i0_in_segment) / FS`.

**Band envelope `E(fc, B)`** of a segment: FFT, multiply by
`W(f) = cos^2(pi * x / (B + 1))` for `|x| < (B + 1) / 2`, zero elsewhere,
with `x = log2(f / fc)` and `B` the bandwidth in octaves; zero the negative
frequencies, double the positive ones, inverse FFT, magnitude.

**Time reference `t_hf`.** The time of the largest value of
`E(sqrt(3000 * 6000), 1)` on segment A.

**arrival_F** (F = 63, 80, 100): the time of the largest value of
`E(F, 1/3)` on segment A within `[t_hf - 0.15 s, t_hf + 0.30 s]`, minus
`t_hf`, in ms.

**gd_excess_lo_hi.** On segment A: `H = FFT(x)`, `T = FFT(t * x)`, total
group delay `Re(T / H)` in seconds. On the grid `G = geomspace(20, 8000, 521)`
take, per grid point, the median of that quantity over the FFT bins within
plus or minus 1/12 octave (the nearest bin when none falls inside).
Minimum-phase group delay of the same magnitude: smooth `|H|^2` over plus or
minus 1/12 octave on a log grid from 10 Hz to 24 kHz, interpolate its log
magnitude over log frequency onto a 2^17-point FFT grid, fold the real
cepstrum to get the minimum-phase phase, and take `-d(phase)/d(omega)`.
Excess is total minus minimum-phase on `G`. The metric is the median of the
excess over `lo <= f < hi` minus its median over 3000-6000 Hz, in ms.

**pre_F** (F = 63, 125).
`L(env, t) = 10 * log10(mean(env^2 over [t_pk - 0.1 s, t_pk - 1/F]) / env(t_pk)^2)`
where `t_pk` is the time of the largest value of `env` within
`[t_hf - 0.15 s, t_hf + 0.30 s]`. `pre_F = L` of `E(F, 1)` on segment A
minus the same `L` of the zero-phase response with the same magnitude
(`fftshift(ifft(|FFT(segment A)|))`, its own time axis centred on its
middle sample, its own `t_hf` and `t_pk`).

**post20_80_120.** For 8 centres `geomspace(80, 120, 8)`: on segment A,
`env = E(fc, 1/6)`, `t_pk` as above; the last time within 1.0 s after
`t_pk` at which `env > 0.1 * env(t_pk)`, minus `t_pk`, in ms. The metric is
the median over the centres (NaN ignored).

**edt_63_125.** Needs the IR to hold real data up to `i0 + 1.65 s`; if it
is shorter the metric is NaN. For `fc` in (63, 80, 100, 125) on segment B:
band-filter with `W` for `B = 1/3` (real signal, not the envelope),
`p = y^2`, noise is the mean of `p` over the last 100 ms of the segment,
`env` is `p` smoothed with a 20 ms moving average, `start` is the index of
the largest `env` between 0.2 s and 0.5 s into the segment, `stop` is the
first index after `start` where `env < 3 * noise` (the segment end when
none). Backward-integrate `p - noise` from `stop` down to `start`,
normalise to its value at `start`; EDT is 6 times the time at which that
curve first falls to -10 dB. NaN when it never does or when
`10 * log10(env[start] / noise) <= 15`. The metric is the median over the
four bands (NaN ignored; NaN when all are).

**pair_phase(a, b).** Cut both IRs on the same absolute time span
`[t_ref - 0.30 s, t_ref + 1.00 s]` where `t_ref` is the absolute time of
the peak of `a` (`a.t0 + i0_a / FS`), each located through its own `t0`
rounded to samples, zeros where there is no data, the same fades.
`C = FFT(a) * conj(FFT(b))`. For each point of `G` between 30 and 150 Hz
take the complex mean of `C` over plus or minus 1/12 octave, then the mean
over those points of `|angle|` in degrees.

## Tests (`test_art_metrics.py`)

Follow the neighbours (`test_integrity.py`): `import art_metrics` works
because CI runs `python3 -m unittest discover -s .github/scripts`. The lint
job has no numpy, so start the test module with a guard that skips the whole
module when numpy is missing (`raise unittest.SkipTest` at import time), and
make `art_metrics.py` importable without side effects when numpy is
present. Synthetic data only, fixed seeds, each test under about 2 s:

1. `read_wav` round trips float32, PCM16 and PCM24 it writes itself, walks
   an extra chunk before `data`, and rejects stereo and a truncated file.
2. `load_ir` rejects a path that leaves the case folder and applies the gain.
3. `load_export` and `predict`: an identity FIR returns the raw IR; two
   outputs with different FIR delays and gains sum as computed by hand; a
   raw IR with a later `t0` lands that many samples later; a missing raw
   output and an unknown input raise.
4. Arrival: a click at 0 plus a Gaussian-windowed 80 Hz burst centred 10 ms
   later gives `arrival_80` within 1.5 ms of 10.
5. Excess group delay: a pure delayed impulse gives about 0 in the three
   bands (within 0.5 ms); the impulse through a second-order all-pass at
   80 Hz (Q about 1) gives a clearly positive `gd_excess_60_100` within 25 %
   of the median of that all-pass's analytic group delay over 60-100 Hz; a
   minimum-phase second-order high-pass gives about 0 (within 1 ms).
6. Pre-ringing: a symmetric (linear-phase) band-limited FIR gives `pre_63`
   and `pre_125` within 1.5 dB of 0; its minimum-phase version is below
   -6 dB; the time-reversed minimum-phase version is above +6 dB.
7. Decay: a 100 Hz mode with a known T60 (exponentially decaying sine plus a
   click, with a low noise floor, 2.5 s long) gives `edt_63_125` within 15 %
   of T60 and `post20_80_120` within 20 % of T60/3; the same signal cut to
   0.6 s gives NaN for `edt_63_125`.
8. `pair_phase`: identical IRs about 0 degrees, one inverted about 180, one
   delayed by 2 ms within 3 degrees of the mean of `360 * f * 0.002` over
   the grid points between 30 and 150 Hz.
9. `score_case` on a tiny case folder you write to a temp dir (identity
   export, `art` equal to the raw of one input): shapes as specified, `art`
   None when the key is absent.
10. `judge`, `judge_phase`, `pool`, `judge_pooled`, `grade`: the rules
    above, including NaN handling, the 1.5 factor, the group restriction
    and the three grade thresholds at their edges.

## Verification (foreground)

```sh
cd /e/wt/art-gate
python -m unittest discover -s .github/scripts -p "test_art_metrics.py" -v
python -m unittest discover -s .github/scripts -p "test_*.py" 2>&1 | tail -5
python -m py_compile .github/scripts/art_metrics.py
git status --short
```

The second command must not get worse than before your change (other
modules' tests are not yours; report, do not fix, anything unrelated).

## Report

Files changed with line counts; the full output of the first command; the
last lines of the second; anything in the definitions that you had to
decide yourself (state the decision and where it is in the code); known
limits.
