"""Unit tests for gallery_issue.py (python -m unittest, no dependencies)."""
import argparse
import json
import tempfile
import unittest
from pathlib import Path

import gallery_issue as g

VARIANTS = ["light.ko", "dark.ko", "light.en", "dark.en"]


def scene(id_, step, note=None):
    s = {"id": id_, "step": step, "title": {"ko": f"{id_} 제목", "en": f"{id_} title"}}
    if note:
        s["note"] = {"ko": note, "en": note}
    return s


class GalleryIssueTest(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        root = Path(self.tmp.name)
        self.screens = root / "screens"
        self.specimens = root / "specimens"
        self.screens.mkdir()
        self.specimens.mkdir()
        # Manifest order is not issue order: issue order follows the steps.
        self.scenes = [scene("export.done", "export"), scene("start.new", "start", "대화상자가 열려 있습니다."),
                       scene("system.sample", "system"), scene("odd.one", "elsewhere")]
        self.write_manifest(self.scenes)
        for s in self.scenes:
            for v in VARIANTS:
                (self.screens / f"{s['id']}.{v}.png").write_bytes(b"")
        for name in ["kit.button.light.ko.png", "kit.button.dark.ko.png", "kit.button.light.ko.tutorial.png",
                     "plot.dense.light.ko.png", "frame.rail.light.en.png"]:
            (self.specimens / name).write_bytes(b"")

    def tearDown(self):
        self.tmp.cleanup()

    def write_manifest(self, scenes):
        manifest = {"viewport": {"width": 1280, "height": 800}, "variants": VARIANTS, "scenes": scenes}
        (self.screens / "manifest.json").write_text(json.dumps(manifest, ensure_ascii=False), encoding="utf-8")

    def args(self, **over):
        values = dict(screens=self.screens, specimens=self.specimens, raw_base="https://raw.example/runs/7/",
                      tree_url="https://tree.example/runs/7", context="PR #33", sha="0123456789abcdef",
                      ref="feature", run_url="https://run.example", keep=8, out=Path(self.tmp.name) / "out")
        values.update(over)
        return argparse.Namespace(**values)

    def test_scenes_follow_step_order_and_show_every_variant(self):
        body = g.build_parts(self.args())[0]
        order = [body.index(f"<code>{i}</code>") for i in ["start.new", "system.sample", "export.done", "odd.one"]]
        self.assertEqual(order, sorted(order))
        self.assertLess(body.index("### 시작과 프로젝트"), body.index("### 1 · 시스템"))
        self.assertIn("### elsewhere", body)
        for s in self.scenes:
            for v in VARIANTS:
                self.assertIn(f"https://raw.example/runs/7/screens/{s['id']}.{v}.png", body)
        self.assertIn("대화상자가 열려 있습니다.", body)
        self.assertIn("장면 4개", body)
        self.assertIn("1280×800", body)
        self.assertIn("`0123456789ab`", body)

    def test_specimens_take_one_variant_and_never_the_tutorial_suffix(self):
        self.assertEqual(g.specimen_ids(self.specimens), ["kit.button", "plot.dense"])
        parts = g.build_parts(self.args())
        self.assertEqual(len(parts), 2)
        self.assertIn("견본 2개", parts[1])
        self.assertIn("https://raw.example/runs/7/specimens/kit.button.light.ko.png", parts[1])
        self.assertIn("#### kit", parts[1])
        self.assertNotIn("tutorial", parts[1])

    def test_no_specimens_means_one_part(self):
        for p in self.specimens.iterdir():
            p.unlink()
        self.assertEqual(len(g.build_parts(self.args())), 1)

    def test_missing_screenshot_fails(self):
        (self.screens / "system.sample.dark.en.png").unlink()
        with self.assertRaises(g.GalleryError):
            g.build_parts(self.args())

    def test_empty_manifest_fails(self):
        self.write_manifest([])
        with self.assertRaises(g.GalleryError):
            g.build_parts(self.args())

    def test_paginate_splits_under_the_limit_and_repeats_the_heading(self):
        blocks = [("### A", "a" * 50 + "\n"), ("### A", "b" * 50 + "\n"), ("### B", "c" * 50 + "\n")]
        parts = g.paginate("HEAD\n", blocks, "MORE\n", limit=100)
        self.assertEqual(len(parts), 3)
        self.assertTrue(all(len(p) <= 100 for p in parts))
        self.assertTrue(parts[1].startswith("MORE\n"))
        self.assertIn("### A (이어서)", parts[1])
        self.assertIn("### B\n", parts[2])
        self.assertNotIn("이어서", parts[2])

    def test_paginate_refuses_a_block_over_the_limit(self):
        with self.assertRaises(g.GalleryError):
            g.paginate("H\n", [("### A", "x" * 200)], "M\n", limit=100)

    def test_real_sized_tour_fits_in_one_part(self):
        scenes = [scene(f"step{i // 8}.scene-{i}", "system") for i in range(60)]
        self.write_manifest(scenes)
        for s in scenes:
            for v in VARIANTS:
                (self.screens / f"{s['id']}.{v}.png").write_bytes(b"")
        parts = g.build_parts(self.args())
        self.assertTrue(all(len(p) <= g.LIMIT for p in parts))

    def test_main_writes_parts(self):
        out = Path(self.tmp.name) / "out"
        argv = ["--screens", str(self.screens), "--specimens", str(self.specimens), "--raw-base", "https://r",
                "--tree-url", "https://t", "--context", "manual", "--sha", "abc", "--ref", "main",
                "--run-url", "https://u", "--keep", "8", "--out", str(out)]
        self.assertEqual(g.main(argv), 0)
        self.assertEqual(sorted(p.name for p in out.iterdir()), ["part-1.md", "part-2.md"])


if __name__ == "__main__":
    unittest.main()
