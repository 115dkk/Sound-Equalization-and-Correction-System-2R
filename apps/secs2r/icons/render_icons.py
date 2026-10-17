"""Makes the app icons from the upstream logo (docs/DEVIATIONS.md U12).

The bundle icons are upstream's confirmed logo, `logo_b8.svg`. The Windows
icon and the 32, 128 and 256 px PNGs are upstream's own renders, copied byte
for byte (`app.ico`, `app_256.png`); the larger sizes the macOS and Linux
bundles need are rendered here with upstream's renderer
(`tools/render_logo.py`, Qt SVG), which draws 32 px and smaller from the
simplified `logo_b8_small.svg` as upstream does.

`window/` holds the other choice in the app settings, upstream's sixth draft
`logo_b6.svg`. The shell sets it as the window icon when the user picks it;
the bundle icons stay `logo_b8`.

Run again when upstream changes its logo (`upstream.py` names this file):

    pip install PyQt6
    QT_QPA_PLATFORM=offscreen python apps/secs2r/icons/render_icons.py
"""

from __future__ import annotations

import shutil
import struct
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
ASSETS = ROOT / "src/secs/wizard/assets"
sys.path.insert(0, str(ROOT / "tools"))

from PyQt6.QtGui import QGuiApplication, QImage  # noqa: E402
from render_logo import png_bytes, render  # noqa: E402

PNG = b"\x89PNG\r\n\x1a\n"


def draw(name: str, size: int) -> QImage:
    """Upstream's rule: 32 px and smaller come from the simplified drawing."""
    small = ASSETS / f"{name}_small.svg"
    return render(small if size <= 32 and small.is_file() else ASSETS / f"{name}.svg", size)


def ico_entries(path: Path) -> dict[int, bytes]:
    data = path.read_bytes()
    _, kind, count = struct.unpack("<HHH", data[:6])
    if kind != 1:
        raise ValueError(f"{path} is not an icon file")
    entries = {}
    for index in range(count):
        width, _, _, _, _, _, size, offset = struct.unpack("<BBBBHHII", data[6 + 16 * index : 22 + 16 * index])
        payload = data[offset : offset + size]
        if not payload.startswith(PNG):
            raise ValueError(f"{path}: the {width or 256} px entry is not a PNG")
        entries[width or 256] = payload
    return entries


def rgba(image: QImage) -> bytes:
    image = image.convertToFormat(QImage.Format.Format_RGBA8888)
    return bytes(image.constBits().asstring(image.sizeInBytes()))


def packbits(channel: bytes) -> bytes:
    """The icns run-length code: 0..127 copies n+1 bytes, 128..255 repeats one byte n-125 times."""
    out = bytearray()
    i = 0
    while i < len(channel):
        run = 1
        while i + run < len(channel) and run < 130 and channel[i + run] == channel[i]:
            run += 1
        if run >= 3:
            out += bytes((run + 125, channel[i]))
            i += run
            continue
        start = i
        while i < len(channel) and i - start < 128:
            if i + 2 < len(channel) and channel[i] == channel[i + 1] == channel[i + 2]:
                break
            i += 1
        out.append(i - start - 1)
        out += channel[start:i]
    return bytes(out)


def legacy(image: QImage) -> tuple[bytes, bytes]:
    """`is32`/`il32` (run-length RGB planes) and `s8mk`/`l8mk` (alpha) for one size."""
    pixels = rgba(image)
    rgb = b"".join(packbits(pixels[channel::4]) for channel in range(3))
    return rgb, pixels[3::4]


def icns(chunks: list[tuple[bytes, bytes]]) -> bytes:
    body = b"".join(kind + struct.pack(">I", 8 + len(data)) + data for kind, data in chunks)
    return b"icns" + struct.pack(">I", 8 + len(body)) + body


def main() -> None:
    app = QGuiApplication.instance() or QGuiApplication([])
    upstream = ico_entries(ASSETS / "app.ico")
    shutil.copyfile(ASSETS / "app.ico", HERE / "icon.ico")
    shutil.copyfile(ASSETS / "app_256.png", HERE / "128x128@2x.png")
    (HERE / "32x32.png").write_bytes(upstream[32])
    (HERE / "128x128.png").write_bytes(upstream[128])

    big = {size: png_bytes(draw("logo_b8", size)) for size in (512, 1024)}
    (HERE / "icon.png").write_bytes(big[512])
    small_rgb, small_mask = legacy(draw("logo_b8", 16))
    large_rgb, large_mask = legacy(draw("logo_b8", 32))
    retina_32 = png_bytes(draw("logo_b8", 32))
    retina_64 = png_bytes(draw("logo_b8", 64))
    (HERE / "icon.icns").write_bytes(
        icns(
            [
                (b"is32", small_rgb),
                (b"s8mk", small_mask),
                (b"il32", large_rgb),
                (b"l8mk", large_mask),
                (b"ic11", retina_32),
                (b"ic12", retina_64),
                (b"ic07", upstream[128]),
                (b"ic13", upstream[256]),
                (b"ic08", upstream[256]),
                (b"ic14", big[512]),
                (b"ic09", big[512]),
                (b"ic10", big[1024]),
            ]
        )
    )

    window = HERE / "window"
    window.mkdir(exist_ok=True)
    for size in (32, 48, 64, 256):
        (window / f"bubble-{size}.png").write_bytes(png_bytes(draw("logo_b6", size)))
    del app
    print("icons written to", HERE)


if __name__ == "__main__":
    main()
