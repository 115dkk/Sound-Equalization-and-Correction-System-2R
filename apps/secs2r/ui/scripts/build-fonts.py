#!/usr/bin/env python3
"""Build the bundled web fonts in apps/secs2r/ui/src/fonts from upstream files.

The app ships its own fonts (docs/DESIGN.md §4): Pretendard for every skin's
labels, Sarasa Mono K for the Matrix skin's readouts, Orbit for the Matrix
skin's bands. This script turns the upstream files into the woff2 files the
stylesheet (src/fonts/fonts.css) references and writes src/fonts/manifest.json,
which src/fonts/fonts.test.ts checks against the files and the catalogs.

    python3 scripts/build-fonts.py \
        --pretendard /path/to/PretendardVariable.woff2 \
        --sarasa-dir /path/with/SarasaMonoK-{Regular,SemiBold,Bold}.ttf \
        --orbit /path/to/Orbit-Regular.ttf

Needs fontTools with woff2 support (`pip install "fonttools[woff]"`, which
brings brotli). Sources and licences are listed in src/fonts/README.md.

Pretendard is copied unchanged: its licence reserves the font name, and the
official variable woff2 already is a web build. Sarasa Mono K and Orbit are
subset to UNICODES below and written as woff2; neither reserves a name.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import shutil
from pathlib import Path

from fontTools import subset
from fontTools.ttLib import TTFont

# Code points the UI can show: Latin with its punctuation and symbols, the
# arrows, operators and shapes the catalogs and formatters use, compatibility
# jamo, every Hangul syllable (user-entered names are not limited to KS X 1001).
UNICODES = (
    "U+0020-007E,U+00A0-00FF,U+0100-017F,U+02C6-02DC,"
    "U+2000-206F,U+2070-209F,U+20A0-20CF,U+2100-214F,U+2190-21FF,U+2200-22FF,"
    "U+2300-23FF,U+25A0-25FF,U+2600-26FF,U+2700-27BF,U+3000-303F,U+3130-318F,"
    "U+AC00-D7A3,U+FEFF,U+FFFD"
)

# OpenType features kept in the subsets. Ligatures (calt, liga, dlig) are left
# out: a readout must show the characters it holds, never a ligature of them.
FEATURES = ["ccmp", "locl", "mark", "mkmk", "kern", "lnum", "tnum", "pnum", "zero"]


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def coverage(path: Path) -> tuple[int, str]:
    """Code point count and the ranges a font maps, as "20-7E A0 A2-FF ..."."""
    points = sorted(TTFont(path, lazy=True).getBestCmap())
    ranges: list[list[int]] = []
    for point in points:
        if ranges and point == ranges[-1][1] + 1:
            ranges[-1][1] = point
        else:
            ranges.append([point, point])
    text = " ".join(f"{a:X}" if a == b else f"{a:X}-{b:X}" for a, b in ranges)
    return len(points), text


def subset_font(source: Path, out: Path) -> None:
    options = subset.Options()
    options.flavor = "woff2"
    options.layout_features = FEATURES
    options.name_IDs = ["*"]
    options.name_legacy = True
    options.notdef_outline = True
    font = subset.load_font(str(source), options)
    subsetter = subset.Subsetter(options)
    subsetter.populate(unicodes=subset.parse_unicodes(UNICODES))
    subsetter.subset(font)
    subset.save_font(font, str(out), options)
    font.close()


def entry(path: Path, family: str, weight, upstream: dict) -> dict:
    glyphs, ranges = coverage(path)
    return {
        "file": path.name,
        "family": family,
        "weight": weight,
        "style": "normal",
        "upstream": upstream,
        "bytes": path.stat().st_size,
        "sha256": sha256(path),
        "codepoints": glyphs,
        "coverage": ranges,
    }


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--pretendard", type=Path, required=True, help="PretendardVariable.woff2 from the pretendard npm package")
    parser.add_argument("--sarasa-dir", type=Path, required=True, help="folder holding SarasaMonoK-{Regular,SemiBold,Bold}.ttf")
    parser.add_argument("--orbit", type=Path, required=True, help="Orbit-Regular.ttf from the Orbit repository")
    parser.add_argument("--out", type=Path, default=Path(__file__).resolve().parent.parent / "src" / "fonts")
    args = parser.parse_args()
    out: Path = args.out
    out.mkdir(parents=True, exist_ok=True)

    fonts = []

    pretendard = out / "PretendardVariable.woff2"
    shutil.copyfile(args.pretendard, pretendard)
    names = TTFont(pretendard, lazy=True)["name"]
    fonts.append(
        entry(
            pretendard,
            "Pretendard",
            [45, 920],
            {
                "name": "Pretendard Variable",
                "version": names.getDebugName(5),
                "source": "npm pretendard@1.3.9, dist/web/variable/woff2/PretendardVariable.woff2 (unchanged)",
                "license": "OFL-1.1, see LICENSE-Pretendard.txt",
            },
        )
    )

    for style, weight in (("Regular", [400, 500]), ("SemiBold", 600), ("Bold", [700, 900])):
        source = args.sarasa_dir / f"SarasaMonoK-{style}.ttf"
        target = out / f"SarasaMonoK-{style}.woff2"
        subset_font(source, target)
        fonts.append(
            entry(
                target,
                "Sarasa Mono K",
                weight,
                {
                    "name": f"Sarasa Mono K {style}",
                    "version": TTFont(source, lazy=True)["name"].getDebugName(5),
                    "source": "Sarasa Gothic 1.0.42 (Arch Linux ttf-sarasa-gothic 1.0.42-2, face 'Sarasa Mono K' of Sarasa-*.ttc), subset to UNICODES",
                    "license": "OFL-1.1, see OFL-Sarasa.txt",
                },
            )
        )

    orbit = out / "Orbit-Regular.woff2"
    subset_font(args.orbit, orbit)
    fonts.append(
        entry(
            orbit,
            "Orbit",
            400,
            {
                "name": "Orbit Regular",
                "version": TTFont(args.orbit, lazy=True)["name"].getDebugName(5),
                "source": "github.com/JAMO-TYPEFACE/Orbit, Fonts/ttf/Orbit-Regular.ttf, subset to UNICODES",
                "license": "OFL-1.1, see OFL-Orbit.txt",
            },
        )
    )

    manifest = {"unicodes": UNICODES, "features": FEATURES, "fonts": fonts}
    (out / "manifest.json").write_text(json.dumps(manifest, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    for font in fonts:
        print(f"{font['file']:28} {font['bytes'] / 1024:7.0f} KB  {font['codepoints']:6} code points  {font['family']} {font['weight']}")


if __name__ == "__main__":
    main()
