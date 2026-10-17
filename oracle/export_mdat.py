"""mdat family: bounded JOS parsing, REW adaptation, and deterministic building."""
from __future__ import annotations

import dataclasses
from pathlib import Path
from typing import Any

import numpy as np

import common
from secs import rew_mdat
from secs.rew_mdat import builder
from secs.rew_mdat.limits import ParseLimits
from secs.rew_mdat.serialization.modified_utf8 import decode_modified_utf8
from secs.rew_mdat.serialization.parser import JavaSerializationParser
from secs.rew_mdat.serialization.writer import dumps, encode_modified_utf8


fam = common.Family("mdat")
TEMPLATE = Path(rew_mdat.__file__).with_name("template_1.mdat")
REAL_IRS = common.ROOT / "src" / "secs" / "rewapi" / "sample_dolby_714.npz"
NOW_MS = 1792195538802


def adapted(value: Any, key: str, digest: bool = False) -> Any:
    """Recursively freeze public mdat dataclasses: arrays as sidecars, or as
    digests when ``digest`` (built files, whose bytes are a blob already)."""
    if isinstance(value, np.ndarray):
        return fam.f64_digest(value) if digest else fam.f64(key, value)
    if dataclasses.is_dataclass(value):
        out = {}
        for field in dataclasses.fields(value):
            if field.name in {"raw_top_level", "unknown_top_level"}:
                continue
            item = getattr(value, field.name)
            if field.name == "unknown_fields":
                out[field.name] = list(item)
            else:
                out[field.name] = adapted(item, f"{key}_{field.name}", digest)
        return out
    if isinstance(value, dict):
        return {str(name): adapted(item, f"{key}_{name}", digest) for name, item in value.items()}
    if isinstance(value, (list, tuple)):
        return [adapted(item, f"{key}_{index}", digest) for index, item in enumerate(value)]
    return common.plain(value)


def round_trip(data: bytes) -> bool:
    parsed = JavaSerializationParser(data).parse()
    return dumps(parsed) == data


def save_document(case: str, data: bytes, *, inputs: dict[str, Any], split: bool = False) -> None:
    model = rew_mdat.loads(data)
    frozen = adapted(model, f"{case}_adapted", digest=True)
    if split:
        refs = []
        for index, measurement in enumerate(frozen["measurements"]):
            measurement_case = f"measurement_{case}_{index:02d}"
            refs.append(measurement_case)
            fam.save(measurement_case, inputs={"index": index}, outputs={"adapted": measurement})
        frozen["measurements"] = {"cases": refs}
    fam.save(case, inputs=inputs, outputs={
        "bytes": fam.blob(f"{case}_bytes", data),
        "adapted": frozen,
        "unknown_top_level_count": len(model.unknown_top_level),
        "round_trip": round_trip(data),
    })


# The checked-in official upstream template exercises every primitive field kind,
# array adaptation, meanings, and the large ordered set of unknown REW fields.
template_bytes = TEMPLATE.read_bytes()
template_blob = fam.blob("template_input", template_bytes)
template_model = rew_mdat.loads(template_bytes)
fam.save("template", inputs={"bytes": template_blob}, outputs={
    "adapted": adapted(template_model, "template_adapted"),
    "unknown_top_level_count": len(template_model.unknown_top_level),
    "round_trip": round_trip(template_bytes),
})


class FixedUuid:
    def __init__(self, value: int) -> None:
        self.int = value


def build_case(case: str, items: list[builder.MeasurementInput], uuids: list[int],
               spl_offset_db: float | None, *, split: bool = False) -> None:
    iterator = iter(uuids)
    original_uuid = builder.uuid.uuid4
    original_time = builder.time.time
    builder.uuid.uuid4 = lambda: FixedUuid(next(iterator))
    builder.time.time = lambda: NOW_MS / 1000.0
    try:
        data = builder.build_mdat_bytes(items, spl_offset_db=spl_offset_db)
    finally:
        builder.uuid.uuid4 = original_uuid
        builder.time.time = original_time
    inputs = {
        "measurements": [{
            "title": item.title,
            "notes": item.notes,
            "sample_rate_hz": item.sample_rate_hz,
            "samples": fam.f64(f"{case}_input_{index}_samples", item.samples),
            "start_time_s": item.start_time_s,
            "spl_offset_db": item.spl_offset_db,
            "source_info": item.source_info,
            "computed": item.computed,
        } for index, item in enumerate(items)],
        "spl_offset_db": spl_offset_db,
        "now_ms": NOW_MS,
        # Decimal strings avoid loss in JSON consumers whose integer is an f64.
        "uuid": [str(value) for value in uuids],
    }
    save_document(case, data, inputs=inputs, split=split)


def decay(rate: int, count: int, frequency: float, phase: float = 0.0) -> np.ndarray:
    time = np.arange(count, dtype=np.float64) / rate
    signal = np.exp(-time * 19.0) * np.sin(2.0 * np.pi * frequency * time + phase)
    signal[0] += 0.75
    return signal


uuid_base = int("123456789abcdef00123456789abcdef", 16)
build_case("build_1_44100", [
    builder.MeasurementInput("합성 L\0끝 🚀", "메모\0둘째 줄 𐐷", 44100,
                             decay(44100, 257, 73.0), -0.0125, None),
], [uuid_base], 81.25)

items3 = [
    builder.MeasurementInput("Front L", "global SPL", 96000,
                             decay(96000, 513, 61.0), -0.003, None),
    builder.MeasurementInput("Center", "item SPL", 96000,
                             decay(96000, 777, 97.0, 0.2), 0.0, 76.5),
    builder.MeasurementInput("Sub", "template SPL", 96000,
                             decay(96000, 385, 29.0, -0.3), 0.125, None),
]
build_case("build_3_96000", items3, [uuid_base + index + 1 for index in range(3)], 83.0)

# Explicitly exercise an absent global and absent per-item SPL override.
build_case("build_1_48000_absent_spl", [
    builder.MeasurementInput("No offset", "템플릿 값을 보존합니다.", 48000,
                             decay(48000, 129, 113.0), 0.25, None),
], [uuid_base + 4], None)

# Three of the fourteen real Dolby-room IRs (a main, a top, a subwoofer): the
# real-data path without committing all fourteen inputs (each is 384 kB).
REAL_ROLES = ("L", "Tfl", "SW1")
with np.load(REAL_IRS) as sample:
    roles = [name[3:] for name in sample.files if name.startswith("ir_")]
    starts = np.asarray(sample["t0"], dtype=np.float64)
    real_items = [
        builder.MeasurementInput(role, f"실제 돌비룸 IR {index + 1}/14", 48000,
                                 np.asarray(sample[f"ir_{role}"], dtype=np.float64),
                                 float(starts[index]), 79.0 + index * 0.125)
        for index, role in enumerate(roles) if role in REAL_ROLES
    ]
assert [item.title for item in real_items] == list(REAL_ROLES)
build_case("build_3_real_48000", real_items,
           [uuid_base + 5 + index for index in range(len(real_items))], None, split=True)


# Measurement-source environment: scrubbed template, boxed descriptor creation,
# calibration arrays, computed markers, and import/rebuild round-trip.
from secs.rew_mdat import source_info

info_full = {
    "strings": {"outputName": "BlackHole 16ch, L", "sourceFileName": "Umik-1  Gain: 18dB",
                "osName": "mac os x"},
    "boxed": {"micSensitivity": {"type": "java.lang.Double", "value": -1.337},
              "usesEmbeddedCalData": {"type": "java.lang.Boolean", "value": True}},
    "cal": {"meterCal": {"inverseCSelected": False, "sourceName": "7135539_90deg.txt",
                           "sourcePath": "/Users/someone/7135539_90deg.txt",
                           "freqArray": [10.0, 100.0, 1000.0],
                           "gainArray": [-1.5, 0.25, 0.0], "phaseArray": [0.0, 0.0, 0.0],
                           "serialNum": {"type": "java.lang.Integer", "value": 7135539},
                           "sensitivitydB": {"type": "java.lang.Double", "value": -1.337}}},
    "notes": "  Acoustic reference played from L  ",
    "format": "256k Log Swept Sine, 1 sweep at -14.0 dBFS",
    "date": NOW_MS,
}
info_sc = {
    "strings": {"timingRefName": "Center"},
    "boxed": {},
    "cal": {"scCal": {"inverseCSelected": True, "sourceName": "meter.cal",
                        "sourcePath": "/cal/meter.cal", "freqArray": None,
                        "gainArray": None, "phaseArray": None,
                        "revision": {"type": "java.lang.Integer", "value": 7}}},
    "notes": "", "format": "", "date": 0,
}
source_ir = np.zeros(257, dtype=np.float64)
source_ir[12] = 1.0
source_items = [
    builder.MeasurementInput("before", "SECS", 48000, source_ir, source_info=info_full),
    builder.MeasurementInput("after", "SECS", 48000, source_ir, source_info=info_full, computed=True),
    builder.MeasurementInput("partial", "SECS", 48000, source_ir, source_info=info_sc),
]
build_case("source_info_build", source_items,
           [uuid_base + 20 + index for index in range(len(source_items))], None)
source_iterator = iter(uuid_base + 20 + index for index in range(len(source_items)))
original_uuid, original_time = builder.uuid.uuid4, builder.time.time
builder.uuid.uuid4 = lambda: FixedUuid(next(source_iterator))
builder.time.time = lambda: NOW_MS / 1000.0
try:
    source_fixture_data = builder.build_mdat_bytes(source_items)
finally:
    builder.uuid.uuid4, builder.time.time = original_uuid, original_time
loaded_model = rew_mdat.loads(source_fixture_data)
extracted_sources = [source_info.extract({**m.unknown_fields,
                                          "measNotes": source_info.JavaString(m.notes)})
                     for m in loaded_model.measurements]
fam.save("source_info_extract", inputs={"bytes": fam.blob("source_info_extract_bytes", source_fixture_data)},
         outputs={"source_info": extracted_sources})
rebuilt_items = [builder.MeasurementInput(m.name, m.notes, m.sample_rate_hz,
                                         np.asarray(m.impulse_response.samples, dtype=np.float32) /
                                         builder.REW_MDAT_IR_SCALE,
                                         float(m.impulse_response.start_time_s), source_info=info)
                 for m, info in zip(loaded_model.measurements, extracted_sources)]
iterator = iter(uuid_base + 30 + index for index in range(len(rebuilt_items)))
original_uuid, original_time = builder.uuid.uuid4, builder.time.time
builder.uuid.uuid4 = lambda: FixedUuid(next(iterator))
builder.time.time = lambda: NOW_MS / 1000.0
try:
    rebuilt_data = builder.build_mdat_bytes(rebuilt_items)
finally:
    builder.uuid.uuid4, builder.time.time = original_uuid, original_time
rebuilt_model = rew_mdat.loads(rebuilt_data)
fam.save("source_info_round_trip", inputs={"source_info": extracted_sources, "now_ms": NOW_MS,
                                           "uuid": [str(uuid_base + 30 + i) for i in range(len(rebuilt_items))]},
         outputs={"bytes": fam.blob("source_info_round_trip_bytes", rebuilt_data),
                  "source_info": [source_info.extract({**m.unknown_fields,
                                                        "measNotes": source_info.JavaString(m.notes)})
                                  for m in rebuilt_model.measurements]})

# ``from_rew_measurement`` text-mode and Python-float edges. Patch open/time so
# every case is hermetic and carries the same measurement timestamp.
import builtins
import io
import time as time_module

def source_from_case(case, path, raw, output="", input_="", channel=""):
    original_open, original_time = builtins.open, time_module.time
    def fake_open(name, *args, **kwargs):
        if name != path or raw is None:
            raise OSError(name)
        return io.TextIOWrapper(io.BytesIO(raw), encoding="utf-8", errors="replace", newline=None)
    builtins.open = fake_open
    time_module.time = lambda: NOW_MS / 1000.0
    try:
        try:
            result = source_info.from_rew_measurement(path, output, input_, channel)
        except Exception as exc:
            outputs = {"kind": type(exc).__name__, "message": str(exc)}
        else:
            outputs = {"source_info": result}
    finally:
        builtins.open, time_module.time = original_open, original_time
    fam.save("source_info_from_" + case,
             inputs={"path": path, "text": None if raw is None else fam.blob("source_info_from_" + case + "_text", raw),
                     "output_device": output, "input_device": input_, "output_channel": channel,
                     "now_ms": NOW_MS}, outputs=outputs)

source_from_case("full", "/cal/mic.txt",
                 b'"Sens Factor =-1.337dB"\r\n10.0,-1.5;0\r100\t0.25\n1000 0.0 3.0\n',
                 "Out dev", "In dev", "out 1")
source_from_case("unicode_edges", "/cal/edge.txt",
                 b'\xef\xbb\xbfheader\n1_0 2\n0 4 5\n-1 4 5\n20 inf nan\n30 infinity 1\ninvalid\xff row\n')
source_from_case("missing", "/gone/mic.txt", None, "Out", "", "")
source_from_case("empty", "/cal/empty.txt", b"")
source_from_case("bad_sensitivity", "/cal/bad.txt", b"Sens Factor = 1.2.3\n10 0\n")
source_from_case("none", "", None)


# Modified UTF-8 differs from UTF-8 for NUL and supplementary characters.
for index, text in enumerate(["A\0B", "\x7f\x80߿ࠀ", "한국어", "astral 🚀 𐐷"]):
    encoded = encode_modified_utf8(text)
    fam.save(f"utf8_{index}", inputs={"text": text}, outputs={
        "bytes": fam.blob(f"utf8_{index}_bytes", encoded),
        "decode": decode_modified_utf8(encoded),
    })

for name, data in {
    "literal_null": b"\x00",
    "invalid_lead": b"\xf0",
    "invalid_continuation": b"\xc2A",
    "truncated_two": b"\xc2",
    "truncated_three": b"\xe1\x80",
    "overlong": b"\xc1\x81",
}.items():
    try:
        decode_modified_utf8(data)
    except Exception as exc:
        fam.save(f"utf8_error_{name}", inputs={"bytes": fam.blob(f"utf8_error_{name}_bytes", data)},
                 outputs={"kind": type(exc).__name__, "message": str(exc),
                          "offset": getattr(exc, "offset", None)})
    else:
        raise AssertionError(f"{name} unexpectedly decoded")


def save_error(case: str, data: bytes, overrides: dict[str, int],
               descriptor: dict[str, Any] | None = None) -> None:
    limits = ParseLimits(**{**dataclasses.asdict(ParseLimits()), **overrides})
    try:
        JavaSerializationParser(data, limits=limits).parse()
    except Exception as exc:
        fam.save(case, inputs={"bytes": descriptor or fam.blob(f"{case}_bytes", data), "limits": overrides}, outputs={
            "kind": type(exc).__name__, "message": str(exc), "offset": getattr(exc, "offset", None),
        })
    else:
        raise AssertionError(f"{case} unexpectedly parsed")


for case, data in {
    "error_magic": b"\x00\x00\x00\x05",
    "error_version": b"\xac\xed\x00\x04",
    "error_truncated_header": b"\xac\xed\x00",
    "error_truncated_string": b"\xac\xed\x00\x05\x74\x00\x03A",
    "error_dangling_handle": b"\xac\xed\x00\x05\x71\x00\x7e\x00\x00",
}.items():
    save_error(case, data, {})

for offset in (10, 1000, len(template_bytes) - 1):
    save_error(f"error_truncated_template_{offset}", template_bytes[:offset], {})

for name, overrides in {
    "file_bytes": {"max_file_bytes": len(template_bytes) - 1},
    "array_bytes": {"max_array_bytes": 1},
    "array_elements": {"max_array_elements": 1},
    "string_bytes": {"max_string_bytes": 0},
    "depth": {"max_depth": 1},
    "handles": {"max_handles": 0},
    "fields_per_class": {"max_fields_per_class": 0},
    "top_level_values": {"max_top_level_values": 0},
}.items():
    save_error(f"error_limit_{name}", template_bytes, overrides, template_blob)

fam.finish()
