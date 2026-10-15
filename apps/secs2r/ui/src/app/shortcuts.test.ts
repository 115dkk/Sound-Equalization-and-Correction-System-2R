import { describe, expect, it } from "vitest";
import { SHORTCUTS, matchShortcut, shortcutText } from "./shortcuts";

const key = (key: string, mods: Partial<{ ctrlKey: boolean; metaKey: boolean; altKey: boolean; shiftKey: boolean }> = {}, code?: string) => ({
  key,
  ...(code ? { code } : {}),
  ctrlKey: false,
  metaKey: false,
  altKey: false,
  shiftKey: false,
  ...mods,
});

describe("shortcut registry", () => {
  it("covers the upstream shortcuts once each", () => {
    expect(SHORTCUTS.map((s) => s.id)).toEqual(["step.prev", "step.next", "step.1", "step.2", "step.3", "step.4", "step.5", "project.save", "project.save_as", "project.open", "help"]);
  });

  it("matches modifiers exactly", () => {
    expect(matchShortcut(key("s", { ctrlKey: true }), false)).toBe("project.save");
    expect(matchShortcut(key("S", { ctrlKey: true, shiftKey: true }), false)).toBe("project.save_as");
    expect(matchShortcut(key("s"), false)).toBeNull();
    expect(matchShortcut(key("ArrowRight", { altKey: true }), false)).toBe("step.next");
    expect(matchShortcut(key("ArrowRight", { altKey: true, ctrlKey: true }), false)).toBeNull();
    expect(matchShortcut(key("F1"), false)).toBe("help");
  });

  it("uses ⌘ on macOS and the physical key on other layouts", () => {
    expect(matchShortcut(key("o", { metaKey: true }), true)).toBe("project.open");
    expect(matchShortcut(key("o", { ctrlKey: true }), true)).toBeNull();
    expect(matchShortcut(key("ㄴ", { ctrlKey: true }, "KeyS"), false)).toBe("project.save");
    expect(matchShortcut(key("!", { ctrlKey: true, shiftKey: false }, "Digit1"), false)).toBe("step.1");
  });

  it("writes hints for each platform", () => {
    expect(shortcutText("project.save_as", false)).toBe("Ctrl+Shift+S");
    expect(shortcutText("step.prev", false)).toBe("Alt+←");
    expect(shortcutText("project.save_as", true)).toBe("⌘⇧S");
    expect(shortcutText("help", false)).toBe("F1");
  });
});
