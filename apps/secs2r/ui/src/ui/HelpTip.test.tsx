// @vitest-environment happy-dom
import { act, cleanup, createEvent, fireEvent, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { renderWith } from "../test/render";
import { createRef } from "react";
import { Popover } from "./Popover";
import { Dialog } from "./Dialog";
import { HelpTip } from "./HelpTip";
import { NumberField } from "./NumberField";
import { Select, Toggle } from "./controls";
import { TOOLTIP_CLOSE_DELAY_MS, TOOLTIP_DELAY_MS } from "./Tooltip";

afterEach(() => { cleanup(); vi.useRealTimers(); vi.restoreAllMocks(); });
const help = (id: string) => document.querySelector<HTMLButtonElement>(`[data-ui="${id}.help"]`)!;
const hover = (button: HTMLElement) => fireEvent.pointerEnter(button.closest(".ui-tooltip-anchor")!, { pointerType: "mouse" });

describe("contextual help", () => {
  it("keeps field names, stable descriptions and disabled-field help without changing values", () => {
    const change = vi.fn();
    renderWith(<Select id="channel" label="Microphone input" value="1" disabled help="Use the input connected to the microphone." hint="Device unavailable" options={[{ value: "1", label: "1" }]} onChange={change} />);
    const select = screen.getByRole("combobox", { name: "Microphone input" });
    expect(select.getAttribute("aria-describedby")).toBe("channel-hint channel.help-content");
    expect(document.getElementById("channel.help-content")?.textContent).toContain("connected to the microphone");
    expect(document.querySelector("label button")).toBeNull();
    expect(help("channel").disabled).toBe(false);
    expect(screen.getByRole("button", { name: "Microphone input 도움말" })).toBe(help("channel"));
    fireEvent.click(help("channel"));
    expect(screen.getByRole("tooltip").textContent).toContain("connected to the microphone");
    expect(change).not.toHaveBeenCalled();
    expect((select as HTMLSelectElement).value).toBe("1");
  });

  it("waits 400ms for pointer hover and remains readable while moving onto the tooltip", () => {
    vi.useFakeTimers();
    renderWith(<HelpTip id="read" label="Details" content="Explanation" />);
    const button = screen.getByRole("button");
    hover(button);
    act(() => vi.advanceTimersByTime(TOOLTIP_DELAY_MS - 1));
    expect(screen.queryByRole("tooltip")).toBeNull();
    act(() => vi.advanceTimersByTime(1));
    const tip = screen.getByRole("tooltip");
    fireEvent.pointerLeave(button.closest(".ui-tooltip-anchor")!);
    fireEvent.pointerEnter(tip);
    act(() => vi.advanceTimersByTime(1000));
    expect(screen.getByRole("tooltip")).toBe(tip);
    fireEvent.pointerLeave(tip);
    act(() => vi.advanceTimersByTime(TOOLTIP_CLOSE_DELAY_MS));
    expect(screen.queryByRole("tooltip")).toBeNull();
  });

  it("opens immediately on keyboard focus and leaves the field action unchanged", () => {
    const change = vi.fn();
    renderWith(<Toggle id="enabled" label="Use supports" checked help="Adds support speakers." onChange={change} />);
    const button = help("enabled");
    vi.spyOn(button, "matches").mockImplementation((selector) => selector === ":focus-visible");
    fireEvent.focus(button);
    expect(screen.getByRole("tooltip").textContent).toBe("Adds support speakers.");
    expect(change).not.toHaveBeenCalled();
    fireEvent.blur(button);
    expect(screen.queryByRole("tooltip")).toBeNull();
  });

  it("does not let a pending hover replace a more recent keyboard-focused help", () => {
    vi.useFakeTimers();
    renderWith(<><HelpTip id="older" label="Older" content="Older pointer help" /><HelpTip id="newer" label="Newer" content="New keyboard help" /></>);
    const older = screen.getByRole("button", { name: "Older 도움말" });
    const newer = screen.getByRole("button", { name: "Newer 도움말" });
    hover(older);
    vi.spyOn(newer, "matches").mockReturnValue(true);
    fireEvent.focus(newer);
    expect(screen.getByRole("tooltip").textContent).toBe("New keyboard help");
    act(() => vi.advanceTimersByTime(TOOLTIP_DELAY_MS));
    expect(screen.getByRole("tooltip").textContent).toBe("New keyboard help");
  });

  it("keeps the first touch click open even when focus preceded the click, and toggles on a second click", () => {
    renderWith(<HelpTip id="touch" label="Details" content="Touch explanation" />);
    const button = screen.getByRole("button");
    vi.spyOn(button, "matches").mockReturnValue(true);
    fireEvent.pointerDown(button, { pointerType: "touch" });
    fireEvent.focus(button);
    fireEvent.click(button);
    expect(button.getAttribute("aria-expanded")).toBe("true");
    fireEvent.pointerLeave(button.closest(".ui-tooltip-anchor")!);
    const readingPress = createEvent.pointerDown(screen.getByRole("tooltip"), { pointerType: "touch", cancelable: true });
    fireEvent(screen.getByRole("tooltip"), readingPress);
    expect(readingPress.defaultPrevented).toBe(false);
    fireEvent.pointerUp(screen.getByRole("tooltip"), { pointerType: "touch" });
    // Chrome's touch compatibility mouse sequence occurs after pointerup.
    const readingMouse = createEvent.mouseDown(screen.getByRole("tooltip"), { cancelable: true });
    fireEvent(screen.getByRole("tooltip"), readingMouse);
    expect(readingMouse.defaultPrevented).toBe(false);
    fireEvent.blur(button, { relatedTarget: null });
    fireEvent.mouseUp(screen.getByRole("tooltip"));
    fireEvent.click(screen.getByRole("tooltip"));
    expect(screen.getByRole("tooltip")).toBeTruthy();
    fireEvent.click(button);
    expect(screen.queryByRole("tooltip")).toBeNull();
    expect(button.getAttribute("aria-expanded")).toBe("false");
  });

  it("opens only one help at a time and dismisses on an outside press", () => {
    renderWith(<><HelpTip id="one" label="One" content="First explanation" /><HelpTip id="two" label="Two" content="Second explanation" /><button>Other action</button></>);
    fireEvent.click(screen.getByRole("button", { name: "One 도움말" }));
    fireEvent.click(screen.getByRole("button", { name: "Two 도움말" }));
    expect(screen.getAllByRole("tooltip")).toHaveLength(1);
    expect(screen.getByRole("tooltip").textContent).toBe("Second explanation");
    fireEvent.pointerDown(screen.getByRole("button", { name: "Other action" }));
    expect(screen.queryByRole("tooltip")).toBeNull();
  });

  it("keeps an owned tooltip inside a popover's reading boundary and still dismisses unrelated layers", () => {
    const close = vi.fn();
    const anchor = createRef<HTMLButtonElement>();
    renderWith(<>
      <button ref={anchor}>Settings</button>
      <Popover open onClose={close} anchor={anchor} label="Preferences">
        <Toggle id="crossover" label="Crossover PEQ" checked={false} onChange={() => {}} help="Corrects the combined response near the crossover." />
      </Popover>
      <HelpTip id="outside" label="Other context" content="Unrelated help" />
    </>);
    const trigger = help("crossover");
    fireEvent.click(trigger);
    const tip = screen.getByRole("tooltip");
    fireEvent.pointerDown(tip, { pointerType: "touch" });
    fireEvent.pointerUp(tip, { pointerType: "touch" });
    fireEvent.mouseDown(tip);
    fireEvent.blur(trigger, { relatedTarget: null });
    fireEvent.mouseUp(tip);
    fireEvent.click(tip);
    expect(close).not.toHaveBeenCalled();
    expect(screen.getByRole("dialog", { name: "Preferences" })).toBeTruthy();
    expect(screen.getByRole("tooltip")).toBe(tip);
    fireEvent.click(screen.getByRole("button", { name: "Other context 도움말" }));
    fireEvent.pointerDown(screen.getByRole("tooltip"), { pointerType: "touch" });
    expect(close).toHaveBeenCalledOnce();
  });

  it("uses the first Escape for help and the next Escape for the containing dialog", () => {
    const close = vi.fn();
    renderWith(<Dialog open onClose={close} title="Settings" help="Dialog instructions" id="settings"><NumberField id="amount" label="Amount" value={2} min={0} max={10} step={1} help="Choose an amount." onChange={() => {}} /></Dialog>);
    expect(screen.queryByRole("tooltip")).toBeNull();
    fireEvent.click(help("amount"));
    fireEvent.keyDown(help("amount"), { key: "Escape" });
    expect(screen.queryByRole("tooltip")).toBeNull();
    expect(close).not.toHaveBeenCalled();
    fireEvent.keyDown(help("amount"), { key: "Escape" });
    expect(close).toHaveBeenCalledOnce();
  });

  it("keeps a number field's draft-cancel Escape when help is closed", () => {
    const close = vi.fn();
    renderWith(<Dialog open onClose={close} title="Settings"><NumberField id="amount" label="Amount" value={2} min={0} max={10} step={1} help="Choose an amount." onChange={() => {}} /></Dialog>);
    const input = screen.getByRole("spinbutton", { name: "Amount" });
    fireEvent.change(input, { target: { value: "8" } });
    fireEvent.keyDown(input, { key: "Escape" });
    expect((input as HTMLInputElement).value).toBe("2");
    expect(close).not.toHaveBeenCalled();
  });
});
