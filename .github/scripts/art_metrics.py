"""Private ART comparison numerics (ADR 0026); no logging or command-line I/O."""
from __future__ import annotations

import json
import math
import struct
from collections.abc import Iterable
from pathlib import Path, PureWindowsPath
from typing import NamedTuple

import numpy as np

FS = 48000
TOLERANCE = {
    "arrival": 3.0, "gd_excess": 3.0, "pre": 6.0,
    "post20": 20.0, "edt": 0.03, "phase": 25.0,
}
GROUPS = {
    "arrival": ("arrival_63", "arrival_80", "arrival_100"),
    "gd_excess": ("gd_excess_30_60", "gd_excess_60_100", "gd_excess_100_150"),
    "pre": ("pre_63", "pre_125"),
    "decay": ("post20_80_120", "edt_63_125"),
}
CROSS_ROOM_GROUPS = ("arrival", "gd_excess", "pre")
CROSS_ROOM_FACTOR = 1.5
GRID = np.geomspace(20, 8000, 521)
NAN = float("nan")


class Ir(NamedTuple):
    """Mono float64 samples and their first sample's measurement-clock time."""

    samples: np.ndarray
    t0: float


def _decode_wav(data: bytes) -> tuple[np.ndarray, int]:
    """Decode little-endian RIFF, respecting chunk sizes and odd-byte padding."""
    if len(data) < 12 or data[:4] != b"RIFF" or data[8:12] != b"WAVE":
        raise ValueError("wav")
    end = struct.unpack_from("<I", data, 4)[0] + 8
    if end != len(data):
        raise ValueError("wav")
    fmt = payload = None
    offset = 12
    while offset < end:
        if offset + 8 > end:
            raise ValueError("wav")
        name = data[offset:offset + 4]
        size = struct.unpack_from("<I", data, offset + 4)[0]
        offset += 8
        next_offset = offset + size + (size & 1)
        if next_offset > end:
            raise ValueError("wav")
        if name == b"fmt ":
            if fmt is not None:
                raise ValueError("wav")
            fmt = data[offset:offset + size]
        elif name == b"data":
            if payload is not None:
                raise ValueError("wav")
            payload = data[offset:offset + size]
        offset = next_offset
    if fmt is None or payload is None or len(fmt) < 16:
        raise ValueError("wav")
    tag, channels, rate, byte_rate, align, bits = struct.unpack_from("<HHIIHH", fmt)
    if tag == 0xFFFE:
        if len(fmt) < 40:
            raise ValueError("wav")
        extra, valid_bits, mask = struct.unpack_from("<HHI", fmt, 16)
        if extra < 22 or len(fmt) < 18 + extra:
            raise ValueError("wav")
        guid = fmt[24:40]
        if guid[4:] != bytes.fromhex("00001000800000aa00389b71"):
            raise ValueError("wav")
        tag = struct.unpack_from("<I", guid)[0]
        if not 0 < valid_bits <= bits or (tag == 3 and valid_bits != bits):
            raise ValueError("wav")
    if channels != 1 or rate <= 0 or align != bits // 8 or align == 0:
        raise ValueError("wav")
    if byte_rate != rate * align or len(payload) % align:
        raise ValueError("wav")
    if tag == 3 and bits in (32, 64):
        samples = np.frombuffer(payload, dtype=f"<f{align}").astype(np.float64)
    elif tag == 1 and bits in (16, 32):
        samples = np.frombuffer(payload, dtype=f"<i{align}").astype(np.float64)
        samples /= 2 ** (bits - 1)
    elif tag == 1 and bits == 24:
        octets = np.frombuffer(payload, dtype=np.uint8).reshape(-1, 3).astype(np.int32)
        integers = octets[:, 0] | (octets[:, 1] << 8) | (octets[:, 2] << 16)
        integers = (integers ^ 0x800000) - 0x800000
        samples = integers.astype(np.float64) / 2 ** 23
    else:
        raise ValueError("wav")
    if not np.all(np.isfinite(samples)):
        raise ValueError("wav")
    return samples, rate


def read_wav(path: Path) -> tuple[np.ndarray, int]:
    """Read mono PCM/IEEE float WAV; malformed files expose only their basename.

    Extensible PCM uses left-aligned valid bits, scaled by container width.
    Duplicate fmt/data chunks, trailing bytes outside RIFF and nonfinite
    samples are rejected rather than guessed at. The loaders enforce FS.
    """
    path = Path(path)
    try:
        return _decode_wav(path.read_bytes())
    except (OSError, ValueError, struct.error, OverflowError):
        raise ValueError(path.name) from None


def _inside(folder: Path, name: str, plain: bool = False) -> Path:
    """Resolve relative paths beneath folder, including symlink containment."""
    if not isinstance(name, str) or not name or "\x00" in name:
        raise ValueError("wav path")
    relative = Path(name)
    windows = PureWindowsPath(name)
    if relative.is_absolute() or windows.drive or windows.root:
        raise ValueError("wav path")
    if plain and ("/" in name or "\\" in name or name in (".", "..")):
        raise ValueError("wav path")
    try:
        root = folder.resolve()
        resolved = (root / relative).resolve()
        if not resolved.is_relative_to(root) or resolved == root:
            raise ValueError("wav path")
        return resolved
    except (OSError, RuntimeError, ValueError):
        raise ValueError("wav path") from None


def _number(value, name: str) -> float:
    """Parse finite metadata without including its value in an exception."""
    try:
        number = float(value)
        if math.isfinite(number):
            return number
    except (TypeError, ValueError, OverflowError):
        pass
    raise ValueError(name)


def load_ir(case_dir: Path, entry: dict) -> Ir:
    """Load a contained 48 kHz measurement and apply its optional linear gain."""
    samples, rate = read_wav(_inside(case_dir, entry["wav"]))
    if rate != FS:
        raise ValueError("sample rate")
    t0 = _number(entry["t0"], "t0")
    gain_db = _number(entry.get("gain_db", 0.0), "gain_db")
    try:
        gain = 10.0 ** (gain_db / 20.0)
    except OverflowError:
        raise ValueError("gain_db") from None
    with np.errstate(over="ignore", invalid="ignore"):
        samples = samples * gain
    if not np.all(np.isfinite(samples)):
        raise ValueError("gain_db")
    return Ir(samples, t0)


def load_export(folder: Path) -> dict:
    """Load routing and its contained mono FIRs; do not reapply gain or delay."""
    try:
        routing = json.loads((folder / "routing.json").read_text(encoding="utf-8"))
    except (OSError, ValueError):
        raise ValueError("routing.json") from None
    if not isinstance(routing, dict) or routing.get("schema") != 1:
        raise ValueError("schema")
    if routing.get("sample_rate") != FS:
        raise ValueError("sample rate")
    inputs, outputs = routing["inputs"], routing["outputs"]
    if any(not isinstance(names, list) or any(not isinstance(n, str) for n in names)
           or len(set(names)) != len(names) for names in (inputs, outputs)):
        raise ValueError("routing.json")
    fir = {}
    for path in routing["paths"]:
        source, output = path["input"], path["output"]
        if source not in inputs or output not in outputs or (source, output) in fir:
            raise ValueError("routing.json")
        samples, rate = read_wav(_inside(folder, path["wav"], plain=True))
        if rate != FS:
            raise ValueError("sample rate")
        if not samples.size:
            raise ValueError("fir")
        fir[source, output] = samples
    return {"inputs": inputs, "outputs": outputs, "fir": fir}


def _fft_convolve(a: np.ndarray, b: np.ndarray) -> np.ndarray:
    """Linear convolution, zero-padded to the next power-of-two FFT size."""
    if not len(a) or not len(b):
        return np.empty(0, dtype=np.float64)
    length = len(a) + len(b) - 1
    size = 1 << (length - 1).bit_length()
    return np.fft.irfft(np.fft.rfft(a, size) * np.fft.rfft(b, size), size)[:length]


def predict(export: dict, raw: dict[str, Ir], inputs: list[str]) -> dict[str, Ir]:
    """Sum routed convolutions on the earliest raw clock, rounding ties to even.

    All raw IRs define the common clock and longest shifted raw length. Use
    the longest requested FIR if path lengths differ. Unrouted inputs yield
    zeros of that common length; an empty raw set has clock zero and length
    zero. No headroom, path gain or path delay is applied a second time.
    """
    for source in inputs:
        if source not in export["inputs"]:
            raise ValueError(source)
    paths = [(source, output, fir) for (source, output), fir in export["fir"].items()
             if source in inputs]
    for _, output, _ in paths:
        if output not in raw:
            raise ValueError(output)
    t_ref = min((ir.t0 for ir in raw.values()), default=0.0)
    shifts = {name: round((ir.t0 - t_ref) * FS) for name, ir in raw.items()}
    raw_length = max((shifts[name] + len(ir.samples) for name, ir in raw.items()), default=0)
    fir_length = max((len(fir) for _, _, fir in paths), default=1)
    length = raw_length + fir_length - 1 if raw_length else 0
    result = {source: Ir(np.zeros(length), t_ref) for source in inputs}
    for source, output, fir in paths:
        convolved = _fft_convolve(fir, raw[output].samples)
        shift = shifts[output]
        result[source].samples[shift:shift + len(convolved)] += convolved
    return result


def _normalised(samples: np.ndarray) -> np.ndarray | None:
    """Normalise finite nonempty mono data for scale-invariant metrics."""
    samples = np.asarray(samples, dtype=np.float64)
    if samples.ndim != 1 or not samples.size or not np.all(np.isfinite(samples)):
        return None
    peak = float(np.max(np.abs(samples)))
    return samples / peak if peak > 0 else None


def _cut(samples: np.ndarray, start: int, length: int) -> np.ndarray:
    """Take a half-open segment, zero-pad, and fade its available data edges.

    Raised-cosine fades include both endpoints (50 ms in, 100 ms out).
    If available data is shorter than a fade, shorten that fade; overlapping
    fades multiply. Fades belong to the data intersection, not padded zeros.
    """
    result = np.zeros(length)
    lo, hi = max(0, start), min(len(samples), start + length)
    if hi <= lo:
        return result
    available = samples[lo:hi].copy()
    for count, at_end in ((round(0.05 * FS), False), (round(0.1 * FS), True)):
        count = min(count, len(available))
        fade = (1 - np.cos(np.linspace(0, np.pi, count))) / 2
        if at_end:
            available[-count:] *= fade[::-1]
        else:
            available[:count] *= fade
    result[lo - start:hi - start] = available
    return result


def _segment(samples: np.ndarray, peak: int, after: float) -> tuple[np.ndarray, np.ndarray]:
    """Segment [-0.30, after) seconds relative to the absolute-sample peak."""
    before = round(0.30 * FS)
    length = before + round(after * FS)
    return _cut(samples, peak - before, length), (np.arange(length) - before) / FS


def _weight(frequencies: np.ndarray, centre: float, bandwidth: float) -> np.ndarray:
    """Cosine-squared log-frequency window, support (B + 1) octaves wide."""
    weight = np.zeros_like(frequencies)
    positive = frequencies > 0
    x = np.log2(frequencies[positive] / centre)
    weight[positive] = np.where(
        np.abs(x) < (bandwidth + 1) / 2,
        np.cos(np.pi * x / (bandwidth + 1)) ** 2, 0,
    )
    return weight


def _envelope(spectrum: np.ndarray, length: int, centre: float, bandwidth: float) -> np.ndarray:
    """Magnitude of the analytic band signal: double only positive FFT bins."""
    analytic = np.zeros(length, dtype=np.complex128)
    weight = _weight(np.fft.rfftfreq(length, 1 / FS), centre, bandwidth)
    analytic[:len(spectrum)] = 2 * spectrum * weight
    if length % 2 == 0:
        analytic[length // 2] = 0  # Nyquist is not a positive analytic bin.
    return np.abs(np.fft.ifft(analytic))


def _peak_time(env: np.ndarray, times: np.ndarray, reference: float | None = None) -> float:
    """Earliest envelope maximum, optionally within HF -150/+300 ms."""
    mask = np.ones(len(times), dtype=bool) if reference is None else (
        (times >= reference - 0.15) & (times <= reference + 0.30)
    )
    indices = np.flatnonzero(mask)
    if not len(indices):
        return NAN
    index = indices[np.argmax(env[indices])]
    return float(times[index]) if env[index] > 0 else NAN


def _finite_median(values) -> float:
    """Median of finite values, or NaN without emitting empty-slice warnings."""
    values = np.asarray(list(values), dtype=np.float64)
    finite = np.sort(values[np.isfinite(values)])
    if not finite.size:
        return NAN
    middle = len(finite) // 2
    if len(finite) % 2:
        return float(finite[middle])
    return float(finite[middle - 1] / 2 + finite[middle] / 2)


def _bin_ranges(frequencies: np.ndarray, grid: np.ndarray):
    """FFT-bin slices within +/-1/12 octave; use nearest bin for empty slices."""
    for centre in grid:
        lo = np.searchsorted(frequencies, centre / 2 ** (1 / 12), side="left")
        hi = np.searchsorted(frequencies, centre * 2 ** (1 / 12), side="right")
        if lo == hi:
            right = min(int(np.searchsorted(frequencies, centre)), len(frequencies) - 1)
            left = max(0, right - 1)
            lo = left if centre - frequencies[left] <= frequencies[right] - centre else right
            hi = lo + 1
        yield int(lo), int(hi)


def _minimum_group_delay(spectrum: np.ndarray, frequencies: np.ndarray) -> np.ndarray:
    """GD of the smoothed magnitude's folded real-cepstrum minimum phase.

    The unspecified smoothing grid uses 96 intervals/octave (rounded up to
    span 10 Hz..Nyquist). Smooth power by the arithmetic mean of FFT bins,
    then interpolate log magnitude against log frequency. Below 10 Hz,
    extrapolate the 10..20 Hz log-frequency slope; DC uses the first positive
    FFT frequency. This preserves a high-pass roll-off instead of flattening
    it. Floor power at 1e-24 of its maximum only for taking logs.
    Differentiate the unwrapped log-spectrum phase
    on the 2**17 FFT grid, then linearly interpolate GD onto GRID.
    """
    count = math.ceil(math.log2((FS / 2) / 10) * 96) + 1
    smooth_grid = np.geomspace(10, FS / 2, count)
    power = np.abs(spectrum) ** 2
    smoothed = np.array([np.mean(power[lo:hi])
                         for lo, hi in _bin_ranges(frequencies, smooth_grid)])
    log_magnitude = 0.5 * np.log(np.maximum(smoothed, np.max(power) * 1e-24))
    size = 1 << 17
    target = np.fft.rfftfreq(size, 1 / FS)
    log_grid = np.log(smooth_grid)
    log_target = np.log(np.maximum(target, target[1]))
    interpolated = np.interp(log_target, log_grid, log_magnitude)
    slope = (np.interp(math.log(20), log_grid, log_magnitude) - log_magnitude[0]) / math.log(2)
    below = target < smooth_grid[0]
    interpolated[below] = log_magnitude[0] + slope * (log_target[below] - log_grid[0])
    cepstrum = np.fft.irfft(interpolated, size)
    folded = np.zeros(size)
    folded[0], folded[size // 2] = cepstrum[0], cepstrum[size // 2]
    folded[1:size // 2] = 2 * cepstrum[1:size // 2]
    phase = np.fft.rfft(folded).imag
    delay = -np.gradient(phase, 2 * np.pi * target)
    return np.interp(GRID, target, delay)


def _excess_group_delay(samples: np.ndarray, times: np.ndarray, spectrum: np.ndarray) -> dict:
    """Median total-minus-minimum GD per LF band, minus the 3..6 kHz median.

    FFT bins below 1e-12 of the largest magnitude are undefined for T/H.
    Their total GD is omitted, not replaced by a zero-delay observation.
    """
    frequencies = np.fft.rfftfreq(len(samples), 1 / FS)
    numerator = np.fft.rfft(times * samples)
    delay = np.full(len(spectrum), NAN)
    usable = np.abs(spectrum) > np.max(np.abs(spectrum)) * 1e-12
    delay[usable] = (numerator[usable] / spectrum[usable]).real
    total = np.array([_finite_median(delay[lo:hi]) for lo, hi in _bin_ranges(frequencies, GRID)])
    excess = total - _minimum_group_delay(spectrum, frequencies)
    baseline = _finite_median(excess[(GRID >= 3000) & (GRID <= 6000)])
    return {f"gd_excess_{lo}_{hi}": 1000 * (
        _finite_median(excess[(GRID >= lo) & (GRID < hi)]) - baseline
    ) for lo, hi in ((30, 60), (60, 100), (100, 150))}


def _pre_level(env: np.ndarray, times: np.ndarray, reference: float, centre: float) -> float:
    """Mean envelope power from peak-100 ms to peak-one-period, in peak dB."""
    peak = _peak_time(env, times, reference)
    mask = (times >= peak - 0.1) & (times <= peak - 1 / centre)
    if not np.any(mask):
        return NAN
    peak_value = env[np.argmin(np.abs(times - peak))]
    power = float(np.mean((env[mask] / peak_value) ** 2))
    return 10 * math.log10(power) if power > 0 else NAN


def _post20(spectrum: np.ndarray, times: np.ndarray, reference: float) -> float:
    """Median last >-20 dB envelope time within one second of its band peak.

    Search only the available segment, without extrapolating a crossing.
    """
    values = []
    for centre in np.geomspace(80, 120, 8):
        env = _envelope(spectrum, len(times), centre, 1 / 6)
        peak = _peak_time(env, times, reference)
        if not math.isfinite(peak):
            continue
        height = env[np.argmin(np.abs(times - peak))]
        indices = np.flatnonzero((times >= peak) & (times <= peak + 1) & (env > 0.1 * height))
        if len(indices):
            values.append(1000 * (times[indices[-1]] - peak))
    return _finite_median(values)


def _moving_average(power: np.ndarray, width: int) -> np.ndarray:
    """Centred, zero-padded moving mean, matching convolve(..., 'same')."""
    padded = np.pad(power, (width // 2, (width - 1) // 2))
    cumulative = np.concatenate(([0.0], np.cumsum(padded)))
    return (cumulative[width:] - cumulative[:-width]) / width


def _band_edt(power: np.ndarray) -> float:
    """Noise-corrected backward energy integral; six times first -10 dB time.

    Include stop in the integral, do not clip negative noise-corrected
    power, and use the first sampled crossing (no interpolation). Zero
    noise means infinite SNR when the peak is positive.
    """
    noise = float(np.mean(power[-round(0.1 * FS):]))
    env = _moving_average(power, round(0.02 * FS))
    lo, hi = round(0.2 * FS), round(0.5 * FS)
    start = lo + int(np.argmax(env[lo:hi + 1]))
    if env[start] <= 0 or (noise > 0 and env[start] <= noise * 10 ** 1.5):
        return NAN
    below = np.flatnonzero(env[start + 1:] < 3 * noise)
    stop = start + 1 + int(below[0]) if len(below) else len(power) - 1
    curve = np.cumsum((power[start:stop + 1] - noise)[::-1])[::-1]
    if curve[0] <= 0:
        return NAN
    crossings = np.flatnonzero(curve <= 0.1 * curve[0])
    return 6 * int(crossings[0]) / FS if len(crossings) else NAN


def _edt(samples: np.ndarray, peak: int) -> float:
    """Median 1/3-octave Schroeder EDT, requiring data through peak+1.65 s."""
    if len(samples) <= peak + round(1.65 * FS):
        return NAN
    segment, _ = _segment(samples, peak, 1.65)
    spectrum = np.fft.rfft(segment)
    frequencies = np.fft.rfftfreq(len(segment), 1 / FS)
    values = []
    for centre in (63, 80, 100, 125):
        filtered = np.fft.irfft(spectrum * _weight(frequencies, centre, 1 / 3), len(segment))
        values.append(_band_edt(filtered ** 2))
    return _finite_median(values)


def channel_metrics(ir: Ir) -> dict[str, float]:
    """Compute all channel metrics; absent/invalid/silent data yields NaNs."""
    result = {metric: NAN for metrics in GROUPS.values() for metric in metrics}
    samples = _normalised(ir.samples)
    if samples is None:
        return result
    peak = int(np.argmax(np.abs(samples)))
    segment, times = _segment(samples, peak, 1.0)
    if not np.any(segment):
        return result
    spectrum = np.fft.rfft(segment)
    reference = _peak_time(_envelope(spectrum, len(segment), math.sqrt(3000 * 6000), 1), times)
    for centre in (63, 80, 100):
        env = _envelope(spectrum, len(segment), centre, 1 / 3)
        result[f"arrival_{centre}"] = 1000 * (_peak_time(env, times, reference) - reference)
    result.update(_excess_group_delay(segment, times, spectrum))
    zero = np.fft.fftshift(np.fft.irfft(np.abs(spectrum), len(segment)))
    zero_times = (np.arange(len(segment)) - len(segment) // 2) / FS
    zero_spectrum = np.fft.rfft(zero)
    zero_reference = _peak_time(
        _envelope(zero_spectrum, len(segment), math.sqrt(3000 * 6000), 1), zero_times,
    )
    for centre in (63, 125):
        actual = _pre_level(_envelope(spectrum, len(segment), centre, 1), times, reference, centre)
        baseline = _pre_level(_envelope(zero_spectrum, len(segment), centre, 1),
                              zero_times, zero_reference, centre)
        result[f"pre_{centre}"] = actual - baseline
    result["post20_80_120"] = _post20(spectrum, times, reference)
    result["edt_63_125"] = _edt(samples, peak)
    return result


def pair_phase(a: Ir, b: Ir) -> float:
    """Mean absolute cross-spectrum phase, log-grid 30..150 Hz on a shared clock.

    The common segment is half-open and each starting index uses ties-to-even
    rounding. Empty FFT neighbourhoods use the nearest bin, as for GD. A zero
    cross-spectrum contributes no phase observation (NaN if all are zero).
    """
    first, second = _normalised(a.samples), _normalised(b.samples)
    if first is None or second is None:
        return NAN
    reference = a.t0 + int(np.argmax(np.abs(first))) / FS
    length = round(1.3 * FS)
    first = _cut(first, round((reference - 0.3 - a.t0) * FS), length)
    second = _cut(second, round((reference - 0.3 - b.t0) * FS), length)
    cross = np.fft.rfft(first) * np.conj(np.fft.rfft(second))
    frequencies = np.fft.rfftfreq(length, 1 / FS)
    values = []
    for lo, hi in _bin_ranges(frequencies, GRID[(GRID >= 30) & (GRID <= 150)]):
        mean = np.mean(cross[lo:hi])
        if abs(mean) > 0:
            values.append(abs(float(np.angle(mean, deg=True))))
    return float(np.mean(values)) if values else NAN


def score_case(case_dir: Path, export_dir: Path, case: dict) -> dict:
    """Load, predict and score each side, retaining only available phase pairs."""
    raw = {name: load_ir(case_dir, entry) for name, entry in case["eval"]["raw"].items()}
    predicted = predict(load_export(export_dir), raw, case["inputs"])
    art = ({name: load_ir(case_dir, entry) for name, entry in case["art"].items()}
           if "art" in case else None)
    pairs = case.get("phase_pairs", [])

    def phases(channels):
        return {f"{a}/{b}": pair_phase(channels[a], channels[b])
                for a, b in pairs if a in channels and b in channels}

    return {
        "predicted": {name: channel_metrics(ir) for name, ir in predicted.items()},
        "predicted_phase": phases(predicted),
        "art": {name: channel_metrics(ir) for name, ir in art.items()} if art is not None else None,
        "art_phase": phases(art) if art is not None else None,
    }


def tolerance_of(metric: str) -> float:
    """Resolve a metric prefix to its comparison tolerance."""
    if metric.startswith("phase:"):
        return TOLERANCE["phase"]
    for prefix in ("arrival", "gd_excess", "pre", "post20", "edt"):
        if metric.startswith(prefix + "_"):
            return TOLERANCE[prefix]
    raise ValueError("metric")


def _group_metrics(groups: Iterable[str], allowed=GROUPS) -> list[str]:
    """Validate the whole selection, then flatten its metric names."""
    groups = list(groups)
    if any(group not in allowed for group in groups):
        raise ValueError("group")
    return [metric for group in groups for metric in GROUPS[group]]


def _verdict(differences, tolerance: float) -> str:
    """Judge an inclusive bound, saturating overflow of finite-pair differences.

    Callers already discard nonfinite observations. An overflowed absolute
    difference therefore means different, not an unknown measurement.
    """
    value = _finite_median(min(value, float(np.finfo(np.float64).max)) for value in differences)
    return "unknown" if not math.isfinite(value) else "similar" if value <= tolerance else "different"


def judge(predicted, reference, groups: Iterable[str]) -> dict[str, str]:
    """Compare channel-matched metric values, ignoring nonfinite pairs."""
    verdicts = {}
    for metric in _group_metrics(groups):
        differences = []
        for name in predicted.keys() & reference.keys():
            a, b = predicted[name].get(metric, NAN), reference[name].get(metric, NAN)
            if math.isfinite(a) and math.isfinite(b):
                differences.append(abs(float(a) - float(b)))
        verdicts[metric] = _verdict(differences, tolerance_of(metric))
    return verdicts


def judge_phase(predicted_phase, reference_phase) -> dict[str, str]:
    """Compare shared phase pairs; nonfinite shared pairs are unknown.

    Pairs absent on either side are omitted, following 'present in both'.
    """
    return {f"phase:{pair}": _verdict(
        [abs(float(value) - float(reference_phase[pair]))]
        if math.isfinite(value) and math.isfinite(reference_phase[pair]) else [], TOLERANCE["phase"],
    ) for pair, value in predicted_phase.items() if pair in reference_phase}


def pool(references: list[dict]) -> dict[str, float]:
    """Pool finite observations across all ART inputs and cases, equally weighted."""
    values = {}
    for reference in references:
        for metrics in reference.values():
            for metric, value in metrics.items():
                if math.isfinite(value):
                    values.setdefault(metric, []).append(value)
    return {metric: _finite_median(observations) for metric, observations in values.items()}


def judge_pooled(predicted, pooled: dict[str, float], groups: Iterable[str]) -> dict[str, str]:
    """Compare cross-room-safe groups to a pooled ART value, at 1.5x tolerance."""
    verdicts = {}
    for metric in _group_metrics(groups, CROSS_ROOM_GROUPS):
        reference = pooled.get(metric, NAN)
        differences = [abs(float(metrics[metric]) - float(reference)) for metrics in predicted.values()
                       if math.isfinite(reference) and math.isfinite(metrics.get(metric, NAN))]
        verdicts[metric] = _verdict(differences, tolerance_of(metric) * CROSS_ROOM_FACTOR)
    return verdicts


def grade(verdicts: dict[str, str]) -> str:
    """Grade the decided share: close >=75%, fair >=40%, otherwise far."""
    decided = [value for value in verdicts.values() if value != "unknown"]
    if not decided:
        return "none"
    fraction = decided.count("similar") / len(decided)
    return "close" if fraction >= 0.75 else "fair" if fraction >= 0.40 else "far"
