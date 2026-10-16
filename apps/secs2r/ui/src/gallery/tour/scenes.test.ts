import { describe, expect, it } from "vitest";
import { STEPS } from "../../app/model";
import { TOUR_SCENES } from "./scenes";

describe("screen tour scenes", () => {
  it("have unique ids in the file-name form", () => {
    const ids = TOUR_SCENES.map((scene) => scene.id);
    expect(new Set(ids).size).toBe(ids.length);
    for (const id of ids) expect(id).toMatch(/^[a-z0-9]+([.-][a-z0-9]+)*$/);
  });

  it("have a Korean and an English title", () => {
    for (const scene of TOUR_SCENES) {
      expect(scene.title.ko.trim(), scene.id).not.toBe("");
      expect(scene.title.en.trim(), scene.id).not.toBe("");
      if (scene.note) {
        expect(scene.note.ko.trim(), scene.id).not.toBe("");
        expect(scene.note.en.trim(), scene.id).not.toBe("");
      }
    }
  });

  it("cover every step", () => {
    for (const step of STEPS) expect(TOUR_SCENES.some((scene) => scene.step === step), step).toBe(true);
  });

  it("find elements by stable ids, not by words that change with the language", () => {
    for (const scene of TOUR_SCENES) {
      for (const op of scene.ops ?? []) {
        if (op.kind !== "click" && op.kind !== "hover") continue;
        // No quoted text with a space in it, no Hangul, no Playwright text engines.
        for (const quoted of op.selector.match(/"[^"]*"|'[^']*'/g) ?? []) expect(quoted, scene.id).not.toMatch(/\s/);
        expect(op.selector, scene.id).not.toMatch(/[ㄱ-ㆎ가-힣]/);
        expect(op.selector, scene.id).not.toMatch(/:has-text|:text|text=/);
      }
    }
  });
});
