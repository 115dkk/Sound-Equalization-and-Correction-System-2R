// @vitest-environment happy-dom
// Select: the label above (forms) or on the left (tool bars), tied to the
// box either way; the disabled box says so.
import { fireEvent, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { renderWith } from "../test/render";
import { Select } from "./controls";

const OPTIONS = [
  { value: "L", label: "L" },
  { value: "R", label: "R" },
] as const;

describe("select", () => {
  it("puts the label on the left with `inline` and keeps it tied to the box", () => {
    const onChange = vi.fn();
    renderWith(<Select id="s.channel" label="채널" inline value="L" options={OPTIONS} onChange={onChange} />);
    const select = screen.getByLabelText("채널") as HTMLSelectElement;
    expect(select.getAttribute("data-ui")).toBe("s.channel");
    const field = select.closest(".ui-field");
    expect(field?.className).toContain("ui-field--inline");
    expect(field?.className).toContain("ui-select-field--inline");
    fireEvent.change(select, { target: { value: "R" } });
    expect(onChange).toHaveBeenCalledWith("R");
  });

  it("keeps the label above by default and marks a disabled box", () => {
    renderWith(<Select id="s.mode" label="모드" value="L" options={OPTIONS} onChange={() => {}} disabled />);
    const select = screen.getByLabelText("모드") as HTMLSelectElement;
    expect(select.closest(".ui-field")?.className).not.toContain("ui-field--inline");
    expect(select.disabled).toBe(true);
    expect(select.closest(".ui-select")?.className).toContain("ui-select--disabled");
  });
});
