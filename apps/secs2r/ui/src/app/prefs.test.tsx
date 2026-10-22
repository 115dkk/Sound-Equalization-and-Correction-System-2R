// @vitest-environment happy-dom
// The skin axis (ADR 0010): a scope carries data-skin for Hardware Rack and
// Signal Matrix and nothing for the default skin, and the settings popover
// switches it on <html> for the app.
import { fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { SettingsButton } from "./menus";
import { PrefsScope, type Prefs } from "./prefs";

const base: Prefs = { locale: "ko", theme: "light", accent: "default" };

function scopeOf(prefs: Prefs) {
  const { container } = render(
    <PrefsScope initial={prefs}>
      <span>내용</span>
    </PrefsScope>,
  );
  const scope = container.querySelector(".prefs-scope");
  if (!(scope instanceof HTMLElement)) throw new Error("no prefs scope");
  return scope;
}

afterEach(() => {
  delete document.documentElement.dataset.skin;
});

describe("skin scope", () => {
  it("sets data-skin for rack and matrix", () => {
    expect(scopeOf({ ...base, skin: "rack" }).getAttribute("data-skin")).toBe("rack");
    expect(scopeOf({ ...base, skin: "matrix" }).getAttribute("data-skin")).toBe("matrix");
  });

  it("sets no data-skin for the default skin or when the skin is left out", () => {
    expect(scopeOf({ ...base, skin: "default" }).hasAttribute("data-skin")).toBe(false);
    expect(scopeOf(base).hasAttribute("data-skin")).toBe(false);
  });

  it("switches the app's skin on <html> from the settings popover", () => {
    render(
      <PrefsScope initial={{ ...base, skin: "rack" }} root>
        <SettingsButton initiallyOpen />
      </PrefsScope>,
    );
    const html = document.documentElement;
    expect(html.dataset.skin).toBe("rack");

    const group = screen.getByRole("radiogroup", { name: "스킨" });
    expect(within(group).getByRole("radio", { name: "하드웨어 랙" }).getAttribute("aria-checked")).toBe("true");

    fireEvent.click(within(group).getByRole("radio", { name: "시그널 매트릭스" }));
    expect(html.dataset.skin).toBe("matrix");

    fireEvent.click(within(group).getByRole("radio", { name: "기본" }));
    expect(html.hasAttribute("data-skin")).toBe(false);
  });
});
