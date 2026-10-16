"""Markdown for the screen gallery review issue that ui-gallery.yml opens.

The issue shows the screen tour (scripts/screen-tour.mjs: every step and
state of the real app on the mock service, four variants per scene) and the
specimen gallery (scripts/gallery-shots.mjs) from PNGs published on the
ui-gallery branch.

  python3 gallery_issue.py --screens DIR --specimens DIR --raw-base URL \
      --tree-url URL --context TEXT --sha SHA --ref REF --run-url URL \
      --keep N --out DIR

Writes part-1.md, part-2.md, ... to --out, each under GitHub's 65,536
character limit for an issue body or a comment. Part 1 opens the issue (or
is the first comment when the issue already exists); the rest follow as
comments. Standard library only.
"""
from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path
from urllib.parse import quote

# GitHub rejects bodies over 65,536 characters; leave room for the headers.
LIMIT = 60_000

# Issue sections in the order a user meets them. Scenes whose step is not
# listed follow in the order the manifest gives.
STEP_TITLES = {
    "start": "시작과 프로젝트",
    "system": "1 · 시스템",
    "connect": "2 · 연결·점검",
    "measure": "3 · 측정·가져오기",
    "calibrate": "4 · 보정",
    "export": "5 · 내보내기",
    "frame": "틀: 도움말·작업·메뉴",
    "tutorial": "튜토리얼",
}
VARIANT_TITLES = {
    "light.ko": "한국어 · 밝게",
    "dark.ko": "한국어 · 어둡게",
    "light.en": "English · 밝게",
    "dark.en": "English · 어둡게",
}
# The specimen gallery shows one variant inline; the rest stay on the branch.
SPECIMEN_VARIANT = "light.ko"


class GalleryError(Exception):
    pass


def load_manifest(screens: Path) -> dict:
    path = screens / "manifest.json"
    try:
        manifest = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, ValueError) as error:
        raise GalleryError(f"{path}: {error}") from error
    if not manifest.get("scenes"):
        raise GalleryError(f"{path}: no scenes")
    for scene in manifest["scenes"]:
        for variant in manifest["variants"]:
            png = screens / f"{scene['id']}.{variant}.png"
            if not png.is_file():
                raise GalleryError(f"missing screenshot {png}")
    return manifest


def scene_block(scene: dict, variants: list[str], base: str) -> str:
    title = scene["title"]["ko"]
    lines = [f"<details><summary><b>{title}</b> · <code>{scene['id']}</code></summary>", ""]
    note = scene.get("note", {}).get("ko")
    if note:
        lines += [note, ""]
    for variant in variants:
        name = f"{scene['id']}.{variant}"
        lines += [f"**{VARIANT_TITLES.get(variant, variant)}**", "", f"![{name}]({base}/screens/{quote(name)}.png)", ""]
    lines += ["</details>", ""]
    return "\n".join(lines)


def screen_blocks(manifest: dict, base: str) -> list[tuple[str, str]]:
    """(section heading, scene block) in issue order."""
    steps = list(STEP_TITLES)
    for scene in manifest["scenes"]:
        if scene["step"] not in steps:
            steps.append(scene["step"])
    blocks = []
    for step in steps:
        heading = f"### {STEP_TITLES.get(step, step)}"
        for scene in manifest["scenes"]:
            if scene["step"] == step:
                blocks.append((heading, scene_block(scene, manifest["variants"], base)))
    return blocks


def specimen_ids(specimens: Path, variant: str = SPECIMEN_VARIANT) -> list[str]:
    """Specimen ids that have a shot in the variant. Ids may contain dots, so
    the variant suffix is matched whole (light.ko never matches
    light.ko.tutorial)."""
    suffix = f".{variant}.png"
    return sorted(p.name[: -len(suffix)] for p in specimens.glob(f"*{suffix}") if p.is_file())


def specimen_blocks(ids: list[str], base: str) -> list[tuple[str, str]]:
    blocks = []
    for specimen in ids:
        group = specimen.split(".", 1)[0]
        name = f"{specimen}.{SPECIMEN_VARIANT}"
        block = "\n".join([
            f"<details><summary><code>{specimen}</code></summary>", "",
            f"![{name}]({base}/specimens/{quote(name)}.png)", "", "</details>", "",
        ])
        blocks.append((f"#### {group}", block))
    return blocks


def paginate(header: str, blocks: list[tuple[str, str]], more: str, limit: int = LIMIT) -> list[str]:
    """Pack blocks into parts under limit, repeating the section heading when
    a section continues in a new part."""
    parts: list[str] = []
    current = header
    section = None
    for heading, block in blocks:
        piece = (f"{heading}\n\n" if heading != section else "") + block
        if len(current) + len(piece) + 1 > limit and current not in (header, more):
            parts.append(current)
            current = more
            piece = f"{heading} (이어서)\n\n{block}" if heading == section else piece
        if len(current) + len(piece) + 1 > limit:
            raise GalleryError(f"one block is longer than {limit} characters")
        current += "\n" + piece
        section = heading
    parts.append(current)
    return parts


def build_parts(args: argparse.Namespace) -> list[str]:
    manifest = load_manifest(args.screens)
    base = args.raw_base.rstrip("/")
    tree = args.tree_url.rstrip("/")
    viewport = manifest.get("viewport", {})
    size = f"{viewport.get('width', '?')}×{viewport.get('height', '?')}"
    count = len(manifest["scenes"])
    header = "\n".join([
        f"화면 갤러리 — {args.context}, 커밋 `{args.sha[:12]}` (`{args.ref}`), [실행 기록]({args.run_url}).",
        "",
        f"실제 앱을 mock 서비스 위에 띄워 장면 {count}개를 창 크기({size}) 그대로 찍었습니다. "
        "장면을 펼치면 한국어·영어와 밝게·어둡게 네 장이 있습니다. 부품 견본은 이어지는 댓글에 있습니다.",
        "",
        f"원본 PNG는 [ui-gallery 브랜치]({tree})에 있고, 그 브랜치는 최근 {args.keep}번의 실행만 남깁니다. "
        "그보다 오래된 실행의 이미지는 이 이슈에서도 보이지 않게 됩니다.",
        "",
    ])
    parts = paginate(header, screen_blocks(manifest, base), "화면 갤러리 (이어서)\n")

    ids = specimen_ids(args.specimens)
    if ids:
        variant = VARIANT_TITLES[SPECIMEN_VARIANT]
        spec_header = "\n".join([
            f"### 부품 견본 ({variant})",
            "",
            f"`gallery.html`의 견본 {len(ids)}개입니다. 어둡게·영어·튜토리얼 강조색은 "
            f"[실행 기록]({args.run_url})의 `ui-gallery` 아티팩트에 있습니다(14일 보관).",
            "",
        ])
        parts += paginate(spec_header, specimen_blocks(ids, base), f"### 부품 견본 ({variant}, 이어서)\n")
    return parts


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n", 1)[0])
    parser.add_argument("--screens", type=Path, required=True)
    parser.add_argument("--specimens", type=Path, required=True)
    parser.add_argument("--raw-base", required=True)
    parser.add_argument("--tree-url", required=True)
    parser.add_argument("--context", required=True)
    parser.add_argument("--sha", required=True)
    parser.add_argument("--ref", required=True)
    parser.add_argument("--run-url", required=True)
    parser.add_argument("--keep", type=int, required=True)
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args(argv)
    try:
        parts = build_parts(args)
    except GalleryError as error:
        print(f"::error::{error}", file=sys.stderr)
        return 1
    args.out.mkdir(parents=True, exist_ok=True)
    for index, part in enumerate(parts, start=1):
        (args.out / f"part-{index}.md").write_text(part, encoding="utf-8")
    print(f"{len(parts)} part(s) in {args.out}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
