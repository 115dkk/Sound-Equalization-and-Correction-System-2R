// @vitest-environment happy-dom
// Menu choices: a checked item is a menu radio item with a check; a checked
// choice that only reports a state stays readable and cannot be chosen.
import { fireEvent, screen, within } from "@testing-library/react";
import { createRef } from "react";
import { describe, expect, it, vi } from "vitest";
import { renderWith } from "../test/render";
import { Menu, type MenuItem } from "./Popover";

function items(onSelect: (id: string) => void, current: "a" | "b" | null): MenuItem[] {
  return [
    { id: "m.a", label: "Alpha", checked: current === "a", onSelect: () => onSelect("a") },
    { id: "m.b", label: "Beta", checked: current === "b", onSelect: () => onSelect("b") },
    { id: "m.custom", label: "Custom", checked: current === null, disabled: true, onSelect: () => onSelect("custom") },
    { id: "m.save", label: "Save", icon: "save", separator: true, onSelect: () => onSelect("save") },
  ];
}

function open(list: MenuItem[], onClose = () => {}) {
  const anchor = createRef<HTMLButtonElement>();
  renderWith(
    <>
      <button type="button" ref={anchor}>
        open
      </button>
      <Menu open onClose={onClose} anchor={anchor} label="Target" items={list} id="m" />
    </>,
  );
  return screen.getByRole("menu", { name: "Target" });
}

describe("menu choices", () => {
  it("draws choices as radio items with the current one checked, and commands as plain items", () => {
    const menu = open(items(() => {}, "b"));
    const alpha = within(menu).getByRole("menuitemradio", { name: "Alpha" });
    const beta = within(menu).getByRole("menuitemradio", { name: "Beta" });
    expect(alpha.getAttribute("aria-checked")).toBe("false");
    expect(beta.getAttribute("aria-checked")).toBe("true");
    expect(beta.querySelector(".ui-menu__check")).not.toBeNull();
    expect(alpha.querySelector(".ui-menu__check")).toBeNull();
    expect(beta.className).toContain("ui-menu__item--checked");
    const save = within(menu).getByRole("menuitem", { name: "Save" });
    expect(save.hasAttribute("aria-checked")).toBe(false);
    expect(save.previousElementSibling?.getAttribute("role")).toBe("separator");
  });

  it("chooses an item and closes", () => {
    const onSelect = vi.fn();
    const onClose = vi.fn();
    const menu = open(items(onSelect, "b"), onClose);
    fireEvent.click(within(menu).getByRole("menuitemradio", { name: "Alpha" }));
    expect(onSelect).toHaveBeenCalledWith("a");
    expect(onClose).toHaveBeenCalled();
  });

  it("checks a state-only choice without letting it be chosen, and skips it with the keys", () => {
    const onSelect = vi.fn();
    const menu = open(items(onSelect, null));
    const custom = within(menu).getByRole("menuitemradio", { name: "Custom" }) as HTMLButtonElement;
    expect(custom.getAttribute("aria-checked")).toBe("true");
    expect(custom.disabled).toBe(true);
    expect(custom.className).toContain("ui-menu__item--checked");
    fireEvent.click(custom);
    expect(onSelect).not.toHaveBeenCalled();
    // ↓ goes Alpha → Beta → Save.
    within(menu).getByRole("menuitemradio", { name: "Alpha" }).focus();
    fireEvent.keyDown(menu, { key: "ArrowDown" });
    fireEvent.keyDown(menu, { key: "ArrowDown" });
    expect(document.activeElement?.textContent).toBe("Save");
  });
});
