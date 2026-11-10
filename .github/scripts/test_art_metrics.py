"""Synthetic-only tests for the private ART gate numerics."""
from __future__ import annotations

import importlib.util
import io
import json
import math
import struct
import tempfile
import unittest
from contextlib import redirect_stderr, redirect_stdout
from pathlib import Path

if importlib.util.find_spec("numpy") is None:
    raise unittest.SkipTest("ART numerics need numpy")

import numpy as np

import art_metrics as art


def wav(path: Path, samples, tag=3, bits=32, channels=1, rate=art.FS,
        extra=False, extensible=False) -> None:
    """Write independent RIFF fixtures, including signed packed PCM24."""
    samples = np.asarray(samples)
    if tag == 3:
        data = samples.astype(f"<f{bits // 8}").tobytes()
    else:
        integers = np.rint(samples * 2 ** (bits - 1)).astype(np.int64)
        if bits == 24:
            data = b"".join((int(value) & 0xFFFFFF).to_bytes(3, "little") for value in integers)
        else:
            data = integers.astype(f"<i{bits // 8}").tobytes()
    align = channels * bits // 8
    fmt = struct.pack("<HHIIHH", 0xFFFE if extensible else tag, channels,
                      rate, rate * align, align, bits)
    if extensible:
        fmt += struct.pack("<HHII", 22, bits, 4, tag) + bytes.fromhex("00001000800000aa00389b71")

    def chunk(name, payload):
        return name + struct.pack("<I", len(payload)) + payload + b"\0" * (len(payload) % 2)

    body = b"WAVE" + chunk(b"fmt ", fmt)
    if extra:
        body += chunk(b"JUNK", b"odd") + chunk(b"LIST", b"metadata")
    body += chunk(b"data", data)
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(b"RIFF" + struct.pack("<I", len(body)) + body)


def routing(folder: Path, inputs, outputs, paths) -> None:
    """Write an export fixture whose path WAVs already carry gain and delay."""
    folder.mkdir(parents=True, exist_ok=True)
    entries = []
    for index, (source, output, samples) in enumerate(paths):
        name = f"fir_{index}.wav"
        wav(folder / name, samples)
        entries.append({"input": source, "output": output, "wav": name,
                        "peak_gain_db": 17, "delay_ms": 9})
    (folder / "routing.json").write_text(json.dumps({
        "schema": 1, "sample_rate": art.FS, "headroom_db": 11.5,
        "inputs": inputs, "outputs": outputs, "paths": entries,
    }), encoding="utf-8")


def impulse(seconds=2.5, time=0.4) -> np.ndarray:
    samples = np.zeros(round(seconds * art.FS))
    samples[round(time * art.FS)] = 1
    return samples


def biquad(kind: str, centre=80.0, q=1.0):
    """RBJ second-order coefficients, independent of the metrics implementation."""
    omega = 2 * np.pi * centre / art.FS
    cosine, alpha = np.cos(omega), np.sin(omega) / (2 * q)
    denominator = np.array([1 + alpha, -2 * cosine, 1 - alpha])
    numerator = (denominator[::-1].copy() if kind == "allpass" else
                 np.array([(1 + cosine) / 2, -(1 + cosine), (1 + cosine) / 2]))
    return numerator / denominator[0], denominator / denominator[0]


def biquad_impulse(numerator, denominator, length=48000):
    """Solve a second-order recurrence, without scipy or FFT phase construction."""
    result = np.zeros(length)
    for n in range(length):
        result[n] = ((numerator[n] if n < 3 else 0)
                     - (denominator[1] * result[n - 1] if n else 0)
                     - (denominator[2] * result[n - 2] if n > 1 else 0))
    return result


def analytic_group_delay(numerator, denominator, frequencies):
    """Exact rational-filter derivative, -Im(d(log H)/d omega)/FS."""
    powers = np.arange(3)
    z = np.exp(-1j * 2 * np.pi * frequencies[:, None] / art.FS * powers)
    b, a = z @ numerator, z @ denominator
    db, da = z @ (-1j * powers * numerator), z @ (-1j * powers * denominator)
    return -np.imag(db / b - da / a) / art.FS


class WavTests(unittest.TestCase):
    def test_float_and_pcm_round_trips_with_extra_chunks(self):
        samples = np.array([-1, -0.75, -0.125, 0, 0.25, 0.5, 0.875])
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "mono.wav"
            for tag, bits in ((3, 32), (3, 64), (1, 16), (1, 24), (1, 32)):
                for extensible in (False, True):
                    with self.subTest(tag=tag, bits=bits, extensible=extensible):
                        wav(path, samples, tag, bits, extra=True, extensible=extensible)
                        actual, rate = art.read_wav(path)
                        self.assertEqual(actual.dtype, np.float64)
                        self.assertEqual(rate, art.FS)
                        np.testing.assert_array_equal(actual, samples)

    def test_rejects_stereo_truncated_unknown_and_nonfinite(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "invalid.wav"
            wav(path, [0, 0], channels=2)
            with self.assertRaisesRegex(ValueError, r"^invalid\.wav$"):
                art.read_wav(path)
            wav(path, [0.25])
            complete = path.read_bytes()
            for cut in (1, 10, len(complete) - 1):
                path.write_bytes(complete[:cut])
                with self.assertRaisesRegex(ValueError, r"^invalid\.wav$"):
                    art.read_wav(path)
            unknown = bytearray(complete)
            struct.pack_into("<H", unknown, 20, 7)
            path.write_bytes(unknown)
            with self.assertRaisesRegex(ValueError, r"^invalid\.wav$"):
                art.read_wav(path)
            wav(path, [np.nan])
            with self.assertRaisesRegex(ValueError, r"^invalid\.wav$"):
                art.read_wav(path)

    def test_extensible_rejects_unknown_guid(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "guid.wav"
            wav(path, [0.5], extensible=True)
            data = bytearray(path.read_bytes())
            data[59] ^= 1
            path.write_bytes(data)
            with self.assertRaisesRegex(ValueError, r"^guid\.wav$"):
                art.read_wav(path)

    def test_load_ir_containment_gain_and_rate(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            wav(root / "eval" / "L.wav", [0.25, -0.5])
            actual = art.load_ir(root, {"wav": "eval/L.wav", "t0": -0.25,
                                        "gain_db": 20 * math.log10(2)})
            np.testing.assert_allclose(actual.samples, [0.5, -1])
            self.assertEqual(actual.t0, -0.25)
            for name in ("../outside.wav", "eval/../../outside.wav", str(root / "eval/L.wav"),
                         "C:\\outside.wav", "C:outside.wav"):
                with self.subTest(name=name), self.assertRaises(ValueError):
                    art.load_ir(root, {"wav": name, "t0": 0})
            wav(root / "rate.wav", [0], rate=44100)
            self.assertEqual(art.read_wav(root / "rate.wav")[1], 44100)
            with self.assertRaisesRegex(ValueError, "^sample rate$"):
                art.load_ir(root, {"wav": "rate.wav", "t0": 0})
            with self.assertRaisesRegex(ValueError, "^gain_db$"):
                art.load_ir(root, {"wav": "eval/L.wav", "t0": 0, "gain_db": "private-value"})

    def test_load_ir_rejects_symlink_escape(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            case = root / "case"
            case.mkdir()
            wav(root / "outside.wav", [0])
            try:
                (case / "link.wav").symlink_to(root / "outside.wav")
            except OSError:
                self.skipTest("symlinks unavailable")
            with self.assertRaises(ValueError):
                art.load_ir(case, {"wav": "link.wav", "t0": 0})


class PredictionTests(unittest.TestCase):
    def test_identity_export(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            routing(root, ["L"], ["L"], [("L", "L", [1])])
            raw = art.Ir(np.array([0, 1, -0.25, 0.5]), -0.25)
            predicted = art.predict(art.load_export(root), {"L": raw}, ["L"])["L"]
            np.testing.assert_allclose(predicted.samples, raw.samples, atol=1e-15)
            self.assertEqual(predicted.t0, raw.t0)

    def test_delayed_gained_paths_sum_on_common_clock(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            routing(root, ["L", "R"], ["L", "SW", "unused"],
                    [("L", "L", [0, 2, 0]), ("L", "SW", [0, 0, -0.5])])
            raw = {"L": art.Ir(np.array([1, 2]), -0.25),
                   "SW": art.Ir(np.array([4, 8]), -0.25 + 2 / art.FS)}
            result = art.predict(art.load_export(root), raw, ["L", "R"])
            np.testing.assert_allclose(result["L"].samples, [0, 2, 4, 0, -2, -4], atol=1e-14)
            np.testing.assert_array_equal(result["R"].samples, np.zeros(6))
            self.assertEqual(result["L"].t0, -0.25)
            self.assertEqual(result["R"].t0, -0.25)

    def test_missing_output_and_unknown_input(self):
        export = {"inputs": ["L"], "outputs": ["SW"], "fir": {("L", "SW"): np.ones(1)}}
        with self.assertRaisesRegex(ValueError, "^SW$"):
            art.predict(export, {}, ["L"])
        with self.assertRaisesRegex(ValueError, "^R$"):
            art.predict(export, {}, ["R"])

    def test_export_rejects_schema_rate_and_nonplain_wav(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            routing(root, ["L"], ["L"], [("L", "L", [1])])
            path = root / "routing.json"
            original = json.loads(path.read_text())
            for key, value, message in (("schema", 2, "schema"), ("sample_rate", 44100, "sample rate")):
                changed = dict(original, **{key: value})
                path.write_text(json.dumps(changed))
                with self.assertRaisesRegex(ValueError, f"^{message}$"):
                    art.load_export(root)
            for name in ("../outside.wav", "nested/fir.wav", "nested\\fir.wav", "C:fir.wav"):
                changed = dict(original, paths=[{"input": "L", "output": "L", "wav": name}])
                path.write_text(json.dumps(changed))
                with self.assertRaises(ValueError):
                    art.load_export(root)

    def test_fft_convolution_matches_direct_sum(self):
        rng = np.random.default_rng(212)
        raw = {"one": art.Ir(rng.normal(size=53), 0),
               "two": art.Ir(rng.normal(size=41), 3 / art.FS)}
        fir = {("L", "one"): rng.normal(size=7), ("L", "two"): rng.normal(size=7)}
        export = {"inputs": ["L"], "outputs": ["one", "two"], "fir": fir}
        expected = np.convolve(raw["one"].samples, fir["L", "one"])
        other = np.convolve(raw["two"].samples, fir["L", "two"])
        expected[3:3 + len(other)] += other
        np.testing.assert_allclose(art.predict(export, raw, ["L"])["L"].samples, expected, atol=1e-13)


class MetricTests(unittest.TestCase):
    def test_arrival_of_delayed_gaussian_burst(self):
        samples = impulse()
        time = np.arange(len(samples)) / art.FS - 0.41
        samples += 0.2 * np.exp(-0.5 * (time / 0.025) ** 2) * np.cos(2 * np.pi * 80 * time)
        metrics = art.channel_metrics(art.Ir(samples, -0.4))
        self.assertAlmostEqual(metrics["arrival_80"], 10, delta=1.5)

    def test_delayed_impulse_has_no_excess_group_delay(self):
        metrics = art.channel_metrics(art.Ir(impulse(time=0.453), -0.4))
        for key in art.GROUPS["gd_excess"]:
            self.assertAlmostEqual(metrics[key], 0, delta=0.5)

    def test_allpass_excess_matches_analytic_group_delay(self):
        numerator, denominator = biquad("allpass")
        samples = np.pad(biquad_impulse(numerator, denominator), (round(0.4 * art.FS), 48000))
        metrics = art.channel_metrics(art.Ir(samples, 0))
        grid = np.geomspace(20, 8000, 521)
        expected = 1000 * np.median(analytic_group_delay(
            numerator, denominator, grid[(grid >= 60) & (grid < 100)],
        ))
        self.assertGreater(metrics["gd_excess_60_100"], 1)
        self.assertAlmostEqual(metrics["gd_excess_60_100"], expected, delta=expected * 0.25)

    def test_minimum_phase_highpass_has_no_excess_group_delay(self):
        numerator, denominator = biquad("highpass", q=1 / math.sqrt(2))
        samples = np.pad(biquad_impulse(numerator, denominator), (round(0.4 * art.FS), 48000))
        metrics = art.channel_metrics(art.Ir(samples, 0))
        for key in art.GROUPS["gd_excess"]:
            self.assertAlmostEqual(metrics[key], 0, delta=1)

    def test_pre_ringing_zero_minimum_and_reversed_minimum_phase(self):
        # A narrow resonator supplies a band-limited, symmetric linear-phase
        # FIR. Its minimum-phase counterpart comes from a stable all-pole
        # transfer function, independently of the metric's cepstral algorithm.
        length = 131072
        frequencies = np.fft.rfftfreq(length, 1 / art.FS)
        z = np.exp(-2j * np.pi * frequencies / art.FS)
        radius = np.exp(-2 * np.pi * 2 / art.FS)
        response = (1 - radius) ** 2 / (
            1 - 2 * radius * np.cos(2 * np.pi * 90 / art.FS) * z + radius ** 2 * z ** 2
        )
        causal = np.fft.irfft(response, length)[:48000]
        symmetric = np.fft.fftshift(np.fft.irfft(np.abs(response), length))
        versions = {
            "zero": symmetric,
            "minimum": np.pad(causal, (19200, 48000)),
            "reversed": np.pad(causal[::-1], (19200, 48000)),
        }
        for name, samples in versions.items():
            metrics = art.channel_metrics(art.Ir(samples, 0))
            for key in ("pre_63", "pre_125"):
                with self.subTest(phase=name, metric=key):
                    if name == "zero":
                        self.assertAlmostEqual(metrics[key], 0, delta=1.5)
                    elif name == "minimum":
                        self.assertLess(metrics[key], -6)
                    else:
                        self.assertGreater(metrics[key], 6)

    def test_decay_known_t60_and_short_ir(self):
        t60 = 0.9
        samples = impulse()
        time = np.arange(len(samples)) / art.FS - 0.4
        positive = time >= 0
        samples[positive] += (0.4 * np.exp(-math.log(1000) * time[positive] / t60)
                              * np.sin(2 * np.pi * 100 * time[positive]))
        samples += np.random.default_rng(212).normal(0, 1e-7, len(samples))
        metrics = art.channel_metrics(art.Ir(samples, -0.4))
        self.assertAlmostEqual(metrics["edt_63_125"], t60, delta=t60 * 0.15)
        self.assertAlmostEqual(metrics["post20_80_120"], 1000 * t60 / 3, delta=1000 * t60 / 3 * 0.20)
        short = art.channel_metrics(art.Ir(samples[:round(0.6 * art.FS)], -0.4))
        self.assertTrue(math.isnan(short["edt_63_125"]))

    def test_pair_phase_identical_inverted_and_delayed(self):
        samples = impulse()
        original = art.Ir(samples, -0.4)
        self.assertAlmostEqual(art.pair_phase(original, original), 0, delta=1e-10)
        self.assertAlmostEqual(art.pair_phase(original, art.Ir(-samples, -0.4)), 180, delta=1e-10)
        grid = np.geomspace(20, 8000, 521)
        expected = np.mean(360 * grid[(grid >= 30) & (grid <= 150)] * 0.002)
        for other in (art.Ir(impulse(time=0.402), -0.4), art.Ir(samples, -0.398)):
            self.assertAlmostEqual(art.pair_phase(original, other), expected, delta=3)
        aligned = art.Ir(np.pad(samples, (96, 0)), -0.402)
        self.assertAlmostEqual(art.pair_phase(original, aligned), 0, delta=1e-8)

    def test_silent_empty_and_invalid_data_are_unknown(self):
        keys = {key for group in art.GROUPS.values() for key in group}
        for samples in (np.zeros(48000), np.array([]), np.array([np.nan]), np.array([np.inf])):
            metrics = art.channel_metrics(art.Ir(samples, 0))
            self.assertEqual(set(metrics), keys)
            self.assertTrue(all(math.isnan(value) for value in metrics.values()))
            self.assertTrue(math.isnan(art.pair_phase(art.Ir(samples, 0), art.Ir(impulse(), 0))))

    def test_metrics_are_gain_invariant_and_silent_on_streams(self):
        samples = impulse()
        out, err = io.StringIO(), io.StringIO()
        with redirect_stdout(out), redirect_stderr(err):
            actual = art.channel_metrics(art.Ir(samples, 0))
            quiet = art.channel_metrics(art.Ir(samples * 1e-100, 0))
        self.assertEqual(out.getvalue(), "")
        self.assertEqual(err.getvalue(), "")
        for key in actual:
            np.testing.assert_allclose(actual[key], quiet[key], equal_nan=True)

    def test_segment_fades_available_data_and_zero_pads(self):
        samples = np.ones(art.FS)
        cut = art._cut(samples, -4800, 62400)
        np.testing.assert_array_equal(cut[:4800], 0)
        np.testing.assert_array_equal(cut[52800:], 0)
        self.assertEqual(cut[4800], 0)
        self.assertEqual(cut[52799], 0)
        self.assertEqual(cut[10000], 1)

    def test_moving_average_matches_direct_convolution(self):
        power = np.random.default_rng(212).random(100)
        for width in (4, 5):
            np.testing.assert_allclose(art._moving_average(power, width),
                                       np.convolve(power, np.ones(width) / width, "same"), atol=1e-14)


class ScoreCaseTests(unittest.TestCase):
    def test_case_shapes_partial_art_and_input_only(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            wav(root / "eval/L.wav", impulse(seconds=0.7))
            wav(root / "eval/R.wav", impulse(seconds=0.7))
            routing(root / "export", ["L", "R"], ["L", "R"],
                    [("L", "L", [1]), ("R", "R", [1])])
            case = {"schema": 1, "inputs": ["L", "R"], "eval": {"raw": {
                "L": {"wav": "eval/L.wav", "t0": -0.4},
                "R": {"wav": "eval/R.wav", "t0": -0.4},
            }}, "art": {"L": {"wav": "eval/L.wav", "t0": -0.4}},
                "phase_pairs": [["L", "R"], ["L", "absent"]]}
            scored = art.score_case(root, root / "export", case)
            self.assertEqual(set(scored), {"predicted", "predicted_phase", "art", "art_phase"})
            self.assertEqual(set(scored["predicted"]), {"L", "R"})
            self.assertEqual(set(scored["art"]), {"L"})
            self.assertEqual(set(scored["predicted_phase"]), {"L/R"})
            self.assertEqual(scored["art_phase"], {})
            for metric in scored["art"]["L"]:
                np.testing.assert_allclose(scored["art"]["L"][metric], scored["predicted"]["L"][metric],
                                           equal_nan=True, atol=1e-10)
            case["art"]["R"] = {"wav": "eval/R.wav", "t0": -0.4}
            paired = art.score_case(root, root / "export", case)
            self.assertEqual(set(paired["art_phase"]), {"L/R"})
            self.assertAlmostEqual(paired["art_phase"]["L/R"], 0, delta=1e-10)
            del case["art"]
            del case["phase_pairs"]
            scored = art.score_case(root, root / "export", case)
            self.assertEqual(scored["predicted_phase"], {})
            self.assertIsNone(scored["art"])
            self.assertIsNone(scored["art_phase"])


class JudgementTests(unittest.TestCase):
    def test_tolerance_prefixes(self):
        for group, expected in (("arrival", 3), ("gd_excess", 3), ("pre", 6),
                                ("post20", 20), ("edt", 0.03)):
            self.assertEqual(art.tolerance_of(group + "_suffix"), expected)
        self.assertEqual(art.tolerance_of("phase:L/R"), 25)
        with self.assertRaises(ValueError):
            art.tolerance_of("other")

    def test_judge_median_finite_channel_intersection_and_boundaries(self):
        predicted = {
            "L": {"arrival_63": 3, "arrival_80": 4, "arrival_100": np.nan},
            "R": {"arrival_63": 100, "arrival_80": 100},
            "C": {"arrival_63": 2, "arrival_80": 5},
            "extra": {"arrival_63": -1000},
        }
        reference = {name: {"arrival_63": 0, "arrival_80": 0, "arrival_100": np.inf}
                     for name in ("L", "R", "C", "reference_only")}
        self.assertEqual(art.judge(predicted, reference, ["arrival"]), {
            "arrival_63": "similar", "arrival_80": "different", "arrival_100": "unknown",
        })
        self.assertTrue(all(v == "unknown" for v in art.judge({}, {}, art.GROUPS).values()))
        with self.assertRaises(ValueError):
            art.judge({}, {}, ["invalid"])

    def test_phase_shared_pairs_and_nan(self):
        self.assertEqual(art.judge_phase(
            {"L/R": 25, "Ls/Rs": 26, "A/B": np.nan, "C/D": 1},
            {"L/R": 0, "Ls/Rs": 0, "A/B": 0, "E/F": 0},
        ), {"phase:L/R": "similar", "phase:Ls/Rs": "different", "phase:A/B": "unknown"})

    def test_pool_uses_all_inputs_and_omits_no_finite_values(self):
        references = [
            {"L": {"arrival_63": 1, "pre_63": np.nan}, "R": {"arrival_63": 3}},
            {"L": {"arrival_63": 9, "pre_63": np.inf, "edt_63_125": 0.3}},
        ]
        self.assertEqual(art.pool(references), {"arrival_63": 3, "edt_63_125": 0.3})
        self.assertEqual(art.pool([]), {})

    def test_pooled_factor_and_group_restriction(self):
        predicted = {"L": {"arrival_63": 4.5, "arrival_80": 4.51, "pre_63": 9,
                            "pre_125": np.inf}}
        pooled = {"arrival_63": 0, "arrival_80": 0, "pre_63": 0, "pre_125": 0}
        actual = art.judge_pooled(predicted, pooled, ["arrival", "pre"])
        self.assertEqual(actual, {"arrival_63": "similar", "arrival_80": "different",
                                  "arrival_100": "unknown", "pre_63": "similar", "pre_125": "unknown"})
        with self.assertRaises(ValueError):
            art.judge_pooled(predicted, pooled, ["decay"])
        with self.assertRaises(ValueError):
            art.judge_pooled(predicted, pooled, ["phase"])
        self.assertTrue(all(v == "unknown" for v in art.judge_pooled({}, {}, ["pre"]).values()))

    def test_finite_extremes_do_not_become_unknown_on_overflow(self):
        predicted = {"L": {"arrival_63": np.float64(1e308)}, "R": {"arrival_63": 1e308}}
        reference = {"L": {"arrival_63": -1e308}, "R": {"arrival_63": -1e308}}
        self.assertEqual(art.judge(predicted, reference, ["arrival"])["arrival_63"], "different")
        self.assertEqual(art.pool([predicted])["arrival_63"], 1e308)
        self.assertEqual(art.judge_pooled(predicted, {"arrival_63": -1e308}, ["arrival"])
                         ["arrival_63"], "different")
        self.assertEqual(art.judge_phase({"L/R": np.float64(1e308)}, {"L/R": -1e308}),
                         {"phase:L/R": "different"})

    def test_grade_thresholds_and_unknown_exclusion(self):
        for similar, total, expected in ((3, 4, "close"), (2, 3, "fair"), (2, 5, "fair"),
                                         (1, 3, "far"), (0, 1, "far"), (1, 1, "close")):
            values = {str(i): "similar" if i < similar else "different" for i in range(total)}
            values["unknown"] = "unknown"
            self.assertEqual(art.grade(values), expected)
        self.assertEqual(art.grade({}), "none")
        self.assertEqual(art.grade({"only": "unknown"}), "none")


if __name__ == "__main__":
    unittest.main()
