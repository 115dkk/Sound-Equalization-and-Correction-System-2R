// @vitest-environment happy-dom
// The graph kit's gestures: target points and the XO line report every step
// (onChange) and commit once per gesture (onCommit); a double click on a
// target point stays with the point; the RC band can have only a lower
// edge; Graph's axis range can be held by the screen.
import { act, fireEvent, screen } from "@testing-library/react";
import { useState } from "react";
import { afterAll, afterEach, beforeAll, describe, expect, it, vi } from "vitest";
import { installFakeCanvas } from "../../test/canvas";
import { renderWith } from "../../test/render";
import type { AxisRangeValue } from "./AxisRange";
import { TargetHandles, XoLine } from "./editors";
import { Graph } from "./Graph";
import { LogPlot, readPalette, type PlotScale } from "./LogPlot";
import type { Curve } from "./series";
import type { TargetPoint } from "./targetEdit";

let recorded: ReturnType<typeof installFakeCanvas> = [];
const calls = () => recorded;

beforeAll(() => {
  recorded = installFakeCanvas(["fillRect", "font"]);
  // happy-dom does no layout: an element's positioned parent is its parent, at 0, 0.
  Object.defineProperty(HTMLElement.prototype, "offsetParent", {
    configurable: true,
    get(this: HTMLElement) {
      return this.parentElement;
    },
  });
});

afterAll(() => {
  Reflect.deleteProperty(HTMLElement.prototype, "offsetParent");
});

/** 100 px per decade from 1 Hz, 1 px per dB around 100 px. */
const SCALE: PlotScale = {
  xToPx: (hz) => Math.log10(hz) * 100,
  pxToX: (px) => 10 ** (px / 100),
  yToPx: (value) => 100 - value,
  pxToY: (px) => 100 - px,
  width: 500,
  height: 300,
  x: [5, 20000],
  y: [-30, 15],
};

const START: TargetPoint[] = [
  { hz: 20, db: 6 },
  { hz: 100, db: 2 },
  { hz: 1000, db: 0 },
];

const ui = (id: string) => {
  const element = document.querySelector<HTMLElement>(`[data-ui="${id}"]`);
  if (!element) throw new Error(`missing [data-ui="${id}"]`);
  return element;
};

/** The handles with their value held by the parent, as a screen holds its draft. */
function HeldHandles({ onChange, onCommit }: { onChange: (points: TargetPoint[]) => void; onCommit: (points: TargetPoint[]) => void }) {
  const [points, setPoints] = useState(START);
  return (
    <div style={{ position: "relative" }}>
      <TargetHandles
        id="t"
        points={points}
        scale={SCALE}
        onChange={(next) => {
          setPoints(next);
          onChange(next);
        }}
        onCommit={onCommit}
      />
    </div>
  );
}

function HeldXo({ onChange, onCommit }: { onChange: (hz: number) => void; onCommit: (hz: number) => void }) {
  const [hz, setHz] = useState(80);
  return (
    <div style={{ position: "relative" }}>
      <XoLine
        id="xo"
        hz={hz}
        scale={SCALE}
        onChange={(next) => {
          setHz(next);
          onChange(next);
        }}
        onCommit={onCommit}
      />
    </div>
  );
}

describe("target points", () => {
  it("commits a drag once, when the pointer is released, with the points it ended on", () => {
    const onChange = vi.fn();
    const onCommit = vi.fn();
    renderWith(<HeldHandles onChange={onChange} onCommit={onCommit} />);
    const handle = ui("t.point.2");
    fireEvent.pointerDown(handle, { button: 0, pointerId: 1 });
    fireEvent.pointerMove(handle, { pointerId: 1, clientX: 210, clientY: 97 });
    fireEvent.pointerMove(handle, { pointerId: 1, clientX: 220, clientY: 95 });
    expect(onChange).toHaveBeenCalledTimes(2);
    expect(onCommit).not.toHaveBeenCalled();
    fireEvent.pointerUp(handle, { pointerId: 1 });
    expect(onCommit).toHaveBeenCalledTimes(1);
    const [ended] = onCommit.mock.calls[0] as [TargetPoint[]];
    expect(ended[1]).toEqual({ hz: 158.5, db: 5 });
    expect(ended).toEqual(onChange.mock.lastCall?.[0]);
  });

  it("commits nothing for a press without a move", () => {
    const onCommit = vi.fn();
    renderWith(<HeldHandles onChange={() => {}} onCommit={onCommit} />);
    const handle = ui("t.point.2");
    fireEvent.pointerDown(handle, { button: 0, pointerId: 1 });
    fireEvent.pointerUp(handle, { pointerId: 1 });
    expect(onCommit).not.toHaveBeenCalled();
  });

  it("commits key nudges when the key is released (a held key repeats)", () => {
    const onChange = vi.fn();
    const onCommit = vi.fn();
    renderWith(<HeldHandles onChange={onChange} onCommit={onCommit} />);
    const handle = ui("t.point.2");
    fireEvent.keyDown(handle, { key: "ArrowUp" });
    fireEvent.keyDown(handle, { key: "ArrowUp" });
    fireEvent.keyDown(handle, { key: "ArrowUp", shiftKey: true });
    expect(onChange).toHaveBeenCalledTimes(3);
    expect(onCommit).not.toHaveBeenCalled();
    fireEvent.keyUp(handle, { key: "ArrowUp" });
    expect(onCommit).toHaveBeenCalledTimes(1);
    expect((onCommit.mock.calls[0] as [TargetPoint[]])[0][1]).toEqual({ hz: 100, db: 6 });
    // A key that moves nothing commits nothing.
    fireEvent.keyDown(handle, { key: "a" });
    fireEvent.keyUp(handle, { key: "a" });
    expect(onCommit).toHaveBeenCalledTimes(1);
  });

  it("commits a nudge when focus leaves before the key is released", () => {
    const onCommit = vi.fn();
    renderWith(<HeldHandles onChange={() => {}} onCommit={onCommit} />);
    const handle = ui("t.point.1");
    fireEvent.keyDown(handle, { key: "ArrowDown" });
    fireEvent.blur(handle);
    expect(onCommit).toHaveBeenCalledTimes(1);
  });

  it("commits a removal at once (Delete or right click), never below two points", () => {
    const onCommit = vi.fn();
    renderWith(<HeldHandles onChange={() => {}} onCommit={onCommit} />);
    fireEvent.keyDown(ui("t.point.3"), { key: "Delete" });
    expect(onCommit).toHaveBeenCalledTimes(1);
    expect(onCommit.mock.lastCall?.[0]).toHaveLength(2);
    fireEvent.contextMenu(ui("t.point.1"));
    expect(onCommit).toHaveBeenCalledTimes(1);
    expect(document.querySelectorAll(".plot-handle")).toHaveLength(2);
  });
});

describe("XO line", () => {
  it("steps by 5 Hz on keys and commits once when the key is released", () => {
    const onChange = vi.fn();
    const onCommit = vi.fn();
    renderWith(<HeldXo onChange={onChange} onCommit={onCommit} />);
    const handle = ui("xo.handle");
    fireEvent.keyDown(handle, { key: "ArrowRight" });
    fireEvent.keyDown(handle, { key: "ArrowRight" });
    expect(onChange.mock.calls).toEqual([[85], [90]]);
    expect(onCommit).not.toHaveBeenCalled();
    fireEvent.keyUp(handle, { key: "ArrowRight" });
    expect(onCommit.mock.calls).toEqual([[90]]);
    fireEvent.keyDown(handle, { key: "End" });
    fireEvent.keyUp(handle, { key: "End" });
    expect(onCommit.mock.lastCall).toEqual([200]);
  });

  it("commits a drag once on release", () => {
    const onCommit = vi.fn();
    renderWith(<HeldXo onChange={() => {}} onCommit={onCommit} />);
    const handle = ui("xo.handle");
    fireEvent.pointerDown(handle, { button: 0, pointerId: 2 });
    fireEvent.pointerMove(handle, { pointerId: 2, clientX: 200 });
    fireEvent.pointerMove(handle, { pointerId: 2, clientX: 205 });
    expect(onCommit).not.toHaveBeenCalled();
    fireEvent.pointerUp(handle, { pointerId: 2 });
    expect(onCommit.mock.calls).toEqual([[110]]);
  });
});

/* ── LogPlot and Graph on the real uPlot (fake canvas) ──────────────────── */

const FREQS = [10, 100, 1000, 10000];
const CURVES: Curve[] = [{ id: "L", role: "L", kind: "main", position: 1, values: [0, 1, 2, 3] }];
const isSub = () => false;

describe("LogPlot", () => {
  afterEach(() => {
    calls().length = 0;
  });

  it("keeps a double click on a target point away from the graph's own double click", () => {
    const onDoubleClick = vi.fn();
    const onChange = vi.fn();
    renderWith(
      <LogPlot
        id="p"
        label="graph"
        freqs={FREQS}
        curves={CURVES}
        roleOrder={["L"]}
        isSub={isSub}
        onDoubleClick={onDoubleClick}
        overlay={(scale) => <TargetHandles id="p.target" points={START} scale={scale} onChange={onChange} />}
      />,
    );
    fireEvent.doubleClick(ui("p.target.point.2"));
    expect(onDoubleClick).not.toHaveBeenCalled();
    expect(onChange).not.toHaveBeenCalled();
    // Empty graph space still adds a point.
    fireEvent.doubleClick(document.querySelector(".u-over")!);
    expect(onDoubleClick).toHaveBeenCalledTimes(1);
  });

  it("rebuilds with the tokens read again when a theme, accent or skin attribute changes above it", async () => {
    const view = renderWith(<LogPlot id="r" label="graph" freqs={FREQS} curves={CURVES} roleOrder={["L"]} isSub={isSub} />);
    const fonts = () => calls().filter((call) => call.method === "font").map((call) => String(call.args[0]));
    expect(fonts().length).toBeGreaterThan(0);
    expect(fonts().every((font) => font.startsWith("400 "))).toBe(true);
    // The host applies its attributes after the graph's first read (PrefsScope
    // does so in an effect); here the frame's own token stands in for the skin.
    const frame = document.querySelector<HTMLElement>(".plot__frame")!;
    frame.style.setProperty("--plot-weight", "700");
    calls().length = 0;
    await act(async () => {
      document.documentElement.setAttribute("data-skin", "rack");
      await new Promise((resolve) => setTimeout(resolve, 0));
    });
    expect(fonts().length).toBeGreaterThan(0);
    expect(fonts().every((font) => font.startsWith("700 "))).toBe(true);
    document.documentElement.removeAttribute("data-skin");
    view.unmount();
  });

  it("shades only below the lower edge while the upper edge is open", async () => {
    const draw = async (band: { lo: number; hi?: number | null; extra?: [number, number][] }) => {
      calls().length = 0;
      const view = renderWith(<LogPlot id="b" label="graph" freqs={FREQS} curves={CURVES} roleOrder={["L"]} isSub={isSub} band={band} />);
      await act(async () => {
        await new Promise((resolve) => setTimeout(resolve, 0));
      });
      const fills = calls().filter((call) => call.method === "fillRect").length;
      view.unmount();
      return fills;
    };
    // Each draw fills one rectangle per shaded side.
    const closed = await draw({ lo: 20, hi: 300 });
    const open = await draw({ lo: 20, hi: null });
    expect(open).toBeGreaterThan(0);
    expect(closed).toBe(2 * open);
    expect(await draw({ lo: 20 })).toBe(open);
    // No extra bands (ADR 0024) draws as before; one above the RC band adds the gap below it.
    expect(await draw({ lo: 20, hi: 300, extra: [] })).toBe(closed);
    expect(await draw({ lo: 20, hi: 300, extra: [[15000, 20000]] })).toBe(3 * open);
    expect(await draw({ lo: 20, hi: null, extra: [[15000, 20000]] })).toBe(open);
  });
});

describe("readPalette", () => {
  it("sets the axis figures' weight from --plot-weight, regular when a skin leaves it", () => {
    const frame = document.createElement("div");
    document.body.appendChild(frame);
    frame.style.setProperty("--font-ui", "Mono");
    expect(readPalette(frame, [], () => false).font).toBe("400 13px Mono");
    frame.style.setProperty("--plot-weight", "700");
    expect(readPalette(frame, [], () => false).font).toBe("700 13px Mono");
    frame.remove();
  });

  it("draws plain lines unless a skin's scope glass sets the --plot-glow tokens", () => {
    const frame = document.createElement("div");
    document.body.appendChild(frame);
    expect(readPalette(frame, [], () => false).scope).toEqual({ glow: 0, glowAlpha: 0, afterglowAlpha: 0, zeroAlpha: 0 });
    frame.style.setProperty("--plot-glow", "3");
    frame.style.setProperty("--plot-glow-alpha", "0.22");
    frame.style.setProperty("--plot-afterglow-alpha", "0.1");
    frame.style.setProperty("--plot-zero-alpha", "0.55");
    expect(readPalette(frame, [], () => false).scope).toEqual({ glow: 3, glowAlpha: 0.22, afterglowAlpha: 0.1, zeroAlpha: 0.55 });
    frame.remove();
  });
});

describe("Graph axis range", () => {
  const summary = () => document.querySelector("figure")?.getAttribute("aria-describedby") ?? "";
  const described = () => document.getElementById(summary())?.textContent;
  const openPopover = () => fireEvent.click(ui("g.axes"));

  it("draws a range the screen holds and reports the user's pick without taking it", () => {
    const onRangeChange = vi.fn();
    const held: AxisRangeValue = { x: [20, 2000], y: [-10, 10] };
    renderWith(<Graph id="g" label="graph" freqs={FREQS} curves={CURVES} channels={[{ role: "L", label: "L", sub: false }]} roleOrder={["L"]} range={held} onRangeChange={onRangeChange} />);
    expect(described()).toBe("곡선 1개, 20–2k Hz");
    expect(ui("g.axes").className).toContain("plot-axes-button--custom");
    openPopover();
    expect((ui("g.axes.x0") as HTMLInputElement).value).toBe("20");
    fireEvent.click(ui("g.axes.auto"));
    expect(onRangeChange).toHaveBeenCalledWith({ x: null, y: null });
    // The screen did not take the pick: the held range stays.
    expect(described()).toBe("곡선 1개, 20–2k Hz");
  });

  it("follows the held range when the screen changes it", () => {
    function Holder() {
      const [range, setRange] = useState<AxisRangeValue>({ x: null, y: null });
      return (
        <>
          <button type="button" onClick={() => setRange({ x: [50, 5000], y: null })}>
            narrow
          </button>
          <Graph id="g" label="graph" freqs={FREQS} curves={CURVES} channels={[{ role: "L", label: "L", sub: false }]} roleOrder={["L"]} xRange={[5, 20000]} range={range} onRangeChange={setRange} />
        </>
      );
    }
    renderWith(<Holder />);
    expect(described()).toBe("곡선 1개, 5–20k Hz");
    fireEvent.click(screen.getByText("narrow"));
    expect(described()).toBe("곡선 1개, 50–5k Hz");
    openPopover();
    fireEvent.click(ui("g.axes.auto"));
    expect(described()).toBe("곡선 1개, 5–20k Hz");
  });

  it("does not apply a field that is not a number: Enter keeps the popover open and goes to that field", async () => {
    const onRangeChange = vi.fn();
    renderWith(<Graph id="g" label="graph" freqs={FREQS} curves={CURVES} channels={[{ role: "L", label: "L", sub: false }]} roleOrder={["L"]} xRange={[5, 20000]} onRangeChange={onRangeChange} />);
    openPopover();
    const upper = ui("g.axes.x1") as HTMLInputElement;
    const level = ui("g.axes.y0") as HTMLInputElement;
    upper.focus();
    fireEvent.change(upper, { target: { value: "abc" } });
    // The user moves on and presses Enter in another field.
    fireEvent.blur(upper);
    level.focus();
    fireEvent.keyDown(level, { key: "Enter" });
    await act(() => new Promise((done) => setTimeout(done, 0)));
    expect(ui("g.axes.popover")).not.toBeNull();
    expect(document.activeElement).toBe(upper);
    expect(upper.getAttribute("aria-invalid")).toBe("true");
    expect(onRangeChange).not.toHaveBeenCalled();
    expect(described()).toBe("곡선 1개, 5–20k Hz");
  });

  it("does not apply an out-of-range field from 적용 either, and applies once it is fixed", async () => {
    const onRangeChange = vi.fn();
    renderWith(<Graph id="g" label="graph" freqs={FREQS} curves={CURVES} channels={[{ role: "L", label: "L", sub: false }]} roleOrder={["L"]} xRange={[5, 20000]} onRangeChange={onRangeChange} />);
    openPopover();
    const lower = ui("g.axes.x0") as HTMLInputElement;
    fireEvent.change(lower, { target: { value: "30000" } });
    fireEvent.blur(lower);
    fireEvent.click(ui("g.axes.apply"));
    expect(ui("g.axes.popover")).not.toBeNull();
    expect(document.activeElement).toBe(lower);
    expect(onRangeChange).not.toHaveBeenCalled();
    fireEvent.change(lower, { target: { value: "30" } });
    fireEvent.keyDown(lower, { key: "Enter" });
    await act(() => new Promise((done) => setTimeout(done, 0)));
    expect(onRangeChange).toHaveBeenCalledWith({ x: [30, 20000], y: [-35, 20] });
    expect(document.querySelector('[data-ui="g.axes.popover"]')).toBeNull();
  });

  it("keeps the pick itself when nobody holds it", () => {
    const onRangeChange = vi.fn();
    renderWith(<Graph id="g" label="graph" freqs={FREQS} curves={CURVES} channels={[{ role: "L", label: "L", sub: false }]} roleOrder={["L"]} xRange={[5, 20000]} onRangeChange={onRangeChange} />);
    openPopover();
    const lower = ui("g.axes.x0") as HTMLInputElement;
    fireEvent.change(lower, { target: { value: "100" } });
    fireEvent.blur(lower);
    fireEvent.submit(lower.closest("form")!);
    expect(onRangeChange).toHaveBeenCalledWith({ x: [100, 20000], y: [-35, 20] });
    expect(described()).toBe("곡선 1개, 100–20k Hz");
  });
});
