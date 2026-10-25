// @vitest-environment happy-dom
import { fireEvent, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { renderWith } from "../test/render";
import { NumberField, checkNumber, stepNumber } from "./NumberField";

describe("number field", () => {
  it("checks the range and reads minus signs and decimal commas", () => {
    expect(checkNumber("20", 10, 200)).toEqual({ ok: true, value: 20 });
    expect(checkNumber("5", 10, 200)).toEqual({ ok: false, reason: "below" });
    expect(checkNumber("250", 10, 200)).toEqual({ ok: false, reason: "above" });
    expect(checkNumber("−12,5", -30, 30)).toEqual({ ok: true, value: -12.5 });
    expect(checkNumber("12dB", 0, 100)).toEqual({ ok: false, reason: "nan" });
    expect(checkNumber(" ", 0, 100)).toEqual({ ok: false, reason: "empty" });
  });

  it("steps inside the range and rounds to the shown decimals", () => {
    expect(stepNumber(195, 10, 10, 200, 0)).toBe(200);
    expect(stepNumber(10, -5, 10, 200, 0)).toBe(10);
    expect(stepNumber(0.1, 0.2, 0, 1, 1)).toBe(0.3);
  });

  function setup(value = 20) {
    const onChange = vi.fn();
    renderWith(<NumberField id="rc_lo" label="RC 하한" value={value} onChange={onChange} min={10} max={200} step={5} unit="Hz" />);
    const input = screen.getByRole("spinbutton", { name: "RC 하한" }) as HTMLInputElement;
    return { onChange, input };
  }

  it("tells a form whether the typed draft can be committed, each time that changes", () => {
    const onDraftValidity = vi.fn();
    renderWith(<NumberField id="rc_lo" label="RC 하한" value={20} onChange={() => {}} min={10} max={200} step={5} unit="Hz" onDraftValidity={onDraftValidity} />);
    const input = screen.getByRole("spinbutton", { name: "RC 하한" }) as HTMLInputElement;
    expect(onDraftValidity.mock.calls).toEqual([[true]]);
    fireEvent.change(input, { target: { value: "abc" } });
    fireEvent.change(input, { target: { value: "250" } });
    expect(onDraftValidity.mock.calls).toEqual([[true], [false]]);
    // Escape restores the committed value, which is valid again.
    fireEvent.keyDown(input, { key: "Escape" });
    expect(onDraftValidity.mock.calls).toEqual([[true], [false], [true]]);
  });

  it("marks an out-of-range value invalid with the range in words and does not commit it", () => {
    const { onChange, input } = setup();
    fireEvent.change(input, { target: { value: "250" } });
    fireEvent.blur(input);
    expect(input.getAttribute("aria-invalid")).toBe("true");
    const error = document.getElementById("rc_lo-error");
    expect(error?.textContent).toBe("10–200 Hz 사이의 값을 넣으세요.");
    expect(input.getAttribute("aria-describedby")).toContain("rc_lo-error");
    expect(onChange).not.toHaveBeenCalled();
  });

  it("commits a valid value on Enter and restores the last value on Escape", () => {
    const { onChange, input } = setup();
    fireEvent.change(input, { target: { value: "150" } });
    fireEvent.keyDown(input, { key: "Enter" });
    expect(onChange).toHaveBeenCalledWith(150);
    fireEvent.change(input, { target: { value: "abc" } });
    fireEvent.keyDown(input, { key: "Escape" });
    expect(input.value).toBe("20");
    expect(input.getAttribute("aria-invalid")).toBeNull();
  });

  it("steps with the arrow keys and stops at the limits", () => {
    const { onChange, input } = setup(200);
    fireEvent.keyDown(input, { key: "ArrowUp" });
    expect(onChange).not.toHaveBeenCalled();
    fireEvent.keyDown(input, { key: "ArrowDown" });
    expect(onChange).toHaveBeenLastCalledWith(195);
    fireEvent.keyDown(input, { key: "ArrowDown", shiftKey: true });
    expect(onChange).toHaveBeenLastCalledWith(145);
    expect(input.getAttribute("aria-valuemin")).toBe("10");
    expect(input.getAttribute("aria-valuemax")).toBe("200");
  });
});
