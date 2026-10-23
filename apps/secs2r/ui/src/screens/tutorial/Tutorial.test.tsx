// @vitest-environment happy-dom
import { act, fireEvent, screen } from "@testing-library/react";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { useState } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { STEPS, canLeave, type FrameModel, type StepId } from "../../app/model";
import { translate } from "../../i18n";
import { renderWith } from "../../test/render";
import { INITIAL, glossaryFor, placeCoach, resumeIndex, tutorialReducer } from "./engine";
import { GLOSSARY } from "./glossary";
import { sampleQuests, type SampleView } from "./sampleQuests";
import { TutorialLayer } from "./Tutorial";
import { GLOSSARY_TERMS, type Quest, type TutorialActions, type TutorialState } from "./types";

/** Every id docs/M5-reach.md §11 cites for the engine. */
const REACH_IDS = [
  "tutorial",
  "tutorial.overlay",
  "tutorial.overlay.look",
  "tutorial.overlay.act",
  "tutorial.coach",
  "tutorial.coach.pause",
  "tutorial.coach.why",
  "tutorial.coach.rule",
  "tutorial.coach.prev",
  "tutorial.coach.next",
  "tutorial.coach.blocked",
  "tutorial.toast",
  "tutorial.panel",
  "tutorial.panel.fold",
  "tutorial.panel.pill",
  "tutorial.panel.restart",
  "tutorial.panel.end",
  "tutorial.resume",
  "tutorial.end",
  "tutorial.end.restart",
  "tutorial.end.finish",
] as const;

type View = { helpOpen: boolean; computing: boolean; computed: boolean; blocked: boolean };

const meta = (steps: StepId[]) => steps.map((step) => ({ step }));

/* ── Pure parts ─────────────────────────────────────────────────────────── */

describe("tutorial flow", () => {
  const quests = meta(["system", "system", "calibrate"]);

  it("moves on, says when a step ends, and ends after the last quest", () => {
    let s = tutorialReducer(INITIAL, { type: "next", quests });
    expect(s.index).toBe(1);
    expect(s.toast).toBeNull();
    s = tutorialReducer(s, { type: "next", quests, note: "tutorial.sample.help.note" });
    expect(s.index).toBe(2);
    expect(s.cleared).toEqual(["system"]);
    expect(s.toast).toEqual({ key: "tutorial.toast.step_done", params: { done: 1, total: 2, step: "system" }, note: "tutorial.sample.help.note" });
    s = tutorialReducer(s, { type: "next", quests });
    expect(s.finished).toBe(true);
    expect(s.completed).toEqual([0, 1, 2]);
  });

  it("arms an act quest on the first 계속 and shakes the card", () => {
    const s = tutorialReducer(INITIAL, { type: "arm" });
    expect(s.armed).toBe(true);
    expect(s.shake).toBe(1);
    expect(tutorialReducer(s, { type: "next", quests }).armed).toBe(false);
  });

  it("goes back and uncounts the step; skips the way it was going", () => {
    let s = tutorialReducer(tutorialReducer(INITIAL, { type: "next", quests }), { type: "next", quests });
    s = tutorialReducer(s, { type: "prev", quests });
    expect(s.index).toBe(1);
    expect(s.cleared).toEqual([]);
    expect(tutorialReducer(s, { type: "skip", quests }).index).toBe(0);
  });

  it("pauses, then resumes where it was or where a step went back", () => {
    const at2: TutorialState = { ...INITIAL, index: 2, completed: [0, 1], cleared: ["system"] };
    const paused = tutorialReducer(at2, { type: "pause" });
    expect(paused.paused).toBe(true);
    expect(tutorialReducer(paused, { type: "resume", index: 2, quests, rewound: false })).toMatchObject({ paused: false, index: 2 });
    const back = tutorialReducer(paused, { type: "resume", index: 0, quests, rewound: true });
    expect(back).toMatchObject({ paused: false, index: 0, completed: [], cleared: [] });
    expect(back.toast?.key).toBe("tutorial.toast.rewound");
  });

  it("finds the quest to resume at: the first unfinished act quest of a step that cannot be left", () => {
    const qs: Quest<View>[] = [
      { id: "a", step: "system", kind: "look", targets: [], title: "tutorial.sample.rail.title", text: "tutorial.sample.rail" },
      { id: "b", step: "system", kind: "act", targets: [], title: "tutorial.sample.help.title", text: "tutorial.sample.help", done: (v) => v.helpOpen },
      { id: "c", step: "calibrate", kind: "act", targets: [], title: "tutorial.sample.compute.title", text: "tutorial.sample.compute" },
    ];
    const view: View = { helpOpen: false, computing: false, computed: false, blocked: false };
    expect(resumeIndex(qs, 2, view, (_v, step) => step !== "system")).toBe(1);
    expect(resumeIndex(qs, 2, view, () => true)).toBe(2);
    expect(resumeIndex(qs, 2, { ...view, helpOpen: true }, (_v, step) => step !== "system")).toBe(0);
  });

  it("resumes at the first unfinished act quest even when act quests carry `blocked`, else at the gate", () => {
    const blocked = () => null;
    const qs: Quest<View>[] = [
      { id: "a", step: "system", kind: "look", targets: [], title: "tutorial.sample.rail.title", text: "tutorial.sample.rail" },
      { id: "b", step: "system", kind: "act", targets: [], title: "tutorial.sample.help.title", text: "tutorial.sample.help", done: (v) => v.helpOpen, blocked },
      { id: "c", step: "system", kind: "act", targets: [], title: "tutorial.sample.help.title", text: "tutorial.sample.help", done: (v) => v.computed, blocked },
      { id: "d", step: "system", kind: "act", targets: [], title: "tutorial.sample.help.title", text: "tutorial.sample.help", done: () => false, blocked, gate: true },
      { id: "e", step: "calibrate", kind: "act", targets: [], title: "tutorial.sample.compute.title", text: "tutorial.sample.compute" },
    ];
    const view: View = { helpOpen: true, computing: false, computed: false, blocked: false };
    const closed = (_v: View, step: StepId) => step !== "system";
    expect(resumeIndex(qs, 4, view, closed)).toBe(2);
    expect(resumeIndex(qs, 4, { ...view, computed: true }, closed)).toBe(3);
  });

  it("reads from the rail whether a step can be left (App's stepOpen, upstream can_advance)", () => {
    const steps = (patch: Partial<FrameModel["steps"]>): FrameModel["steps"] => ({
      system: { status: "done" },
      connect: { status: "done" },
      measure: { status: "ready" },
      calibrate: { status: "ready" },
      export: { status: "blocked", reason: "no_result" },
      ...patch,
    });
    const open = (s: FrameModel["steps"]) => STEPS.filter((step) => canLeave(s, step));
    // Partly measured, no result yet: every step can be left (보정 never holds; upstream opens 내보내기 without a result).
    expect(open(steps({}))).toEqual([...STEPS]);
    expect(open(steps({ system: { status: "blocked", reason: "config_invalid", count: 1 } }))).not.toContain("system");
    expect(open(steps({ connect: { status: "ready", reason: "no_levels", count: 3 } }))).not.toContain("connect");
    expect(open(steps({ connect: { status: "skipped" } }))).toContain("connect");
    // 측정·가져오기 is held by the step after it: no measurement, or one running.
    expect(open(steps({ calibrate: { status: "blocked", reason: "no_measurements" } }))).toEqual(["system", "connect", "calibrate", "export"]);
    expect(open(steps({ measure: { status: "done" }, calibrate: { status: "blocked", reason: "measuring" } }))).not.toContain("measure");
    // A level check running ends by itself; it holds nothing.
    expect(open(steps({ measure: { status: "blocked", reason: "device_busy" } }))).toEqual([...STEPS]);
  });

  it("puts each glossary term under the first quest that names it", () => {
    const qs: Quest<View>[] = [
      { id: "a", step: "system", kind: "look", targets: [], title: "tutorial.sample.rail.title", text: "tutorial.sample.rail", terms: ["xo"], whyTerms: ["fir"] },
      { id: "b", step: "system", kind: "look", targets: [], title: "tutorial.sample.rail.title", text: "tutorial.sample.rail", terms: ["xo", "rc"], whyTerms: ["xo"] },
    ];
    const g = glossaryFor(qs);
    expect(g.get(0)).toEqual({ text: ["xo"], why: ["fir"] });
    expect(g.get(1)).toEqual({ text: ["rc"], why: [] });
  });

  it("has the 25 upstream glossary terms", () => {
    expect(GLOSSARY_TERMS).toHaveLength(25);
    expect(Object.keys(GLOSSARY).sort()).toEqual([...GLOSSARY_TERMS].sort());
  });

  it("places the coach beside the target, off the places to keep clear, and keeps a clear spot", () => {
    const bounds = { x: 268, y: 128, w: 996, h: 620 };
    const card = { w: 340, h: 200 };
    const target = { x: 1118, y: 38, w: 142, h: 60 };
    const panel = { x: 1000, y: 128, w: 268, h: 140 };
    const at = placeCoach([target], card, bounds, [panel]);
    const box = { ...at, ...card };
    const hits = (a: typeof box, b: typeof target) => a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h;
    expect(hits(box, target)).toBe(false);
    expect(hits(box, panel)).toBe(false);
    expect(placeCoach([target], card, bounds, [panel], { x: 300, y: 500 })).toEqual({ x: 300, y: 500 });
    expect(placeCoach([], card, bounds)).toEqual({ x: 268 + (996 - 340) / 2, y: 128 + (620 - 200) / 2 });
  });
});

/* ── The layer on screen ────────────────────────────────────────────────── */

const RECTS: Record<string, [number, number, number, number]> = {
  "frame.rail": [0, 0, 252, 800],
  "frame.help": [20, 760, 200, 40],
  // The step header ends where the content begins; its primary action sits at its right end.
  "frame.header": [252, 0, 1028, 120],
  "frame.content": [252, 120, 1028, 620],
  // The folded pill where tutorial.css puts it: 32 px tall, centred on the header's bottom edge, ending 32 px from the right.
  "tutorial.panel.pill": [1040, 104, 208, 32],
  other: [400, 300, 120, 36],
  primary: [1100, 60, 160, 36],
  // Late targets (the export step's rows): one far below the content's fold, one inside it.
  "late.far": [400, 1500, 160, 36],
  "late.near": [400, 300, 160, 36],
};

beforeEach(() => {
  vi.spyOn(Element.prototype, "getBoundingClientRect").mockImplementation(function (this: Element) {
    const id = this.getAttribute("data-ui") ?? "";
    const [x, y, w, h] = id === "tutorial" ? [0, 0, 1280, 800] : (RECTS[id] ?? [0, 0, 0, 0]);
    return { x, y, left: x, top: y, width: w, height: h, right: x + w, bottom: y + h, toJSON: () => ({}) } as DOMRect;
  });
});

afterEach(() => {
  vi.restoreAllMocks();
});

function quests(host: { openHelp: () => void; compute: () => void }): Quest<View>[] {
  return [
    { id: "rail", step: "system", kind: "look", targets: ["frame.rail"], title: "tutorial.sample.rail.title", text: "tutorial.sample.rail", why: "tutorial.sample.rail.why", terms: ["mimo"] },
    {
      id: "help",
      step: "system",
      kind: "act",
      targets: ["frame.help"],
      title: "tutorial.sample.help.title",
      text: "tutorial.sample.help",
      hint: "tutorial.sample.help.title",
      done: (v) => v.helpOpen,
      auto: { run: host.openHelp, note: "tutorial.sample.help.note" },
      ok: "tutorial.sample.help.ok",
    },
    {
      id: "compute",
      step: "calibrate",
      kind: "act",
      targets: ["primary"],
      title: "tutorial.sample.compute.title",
      text: "tutorial.sample.compute",
      done: (v) => v.computed,
      busy: { when: (v) => v.computing, text: "tutorial.sample.compute.busy" },
      blocked: (v) => (v.blocked ? "tutorial.coach.blocked.default" : null),
      auto: { run: host.compute, note: "tutorial.sample.compute.note" },
    },
  ];
}

function Harness({ initial, start = {}, actions }: { initial?: Partial<TutorialState>; start?: Partial<View>; actions: TutorialActions & { other: () => void } }) {
  const [view, setView] = useState<View>({ helpOpen: false, computing: false, computed: false, blocked: false, ...start });
  const [step, setStep] = useState<StepId>("system");
  const qs = quests({ openHelp: () => setView((v) => ({ ...v, helpOpen: true })), compute: () => setView((v) => ({ ...v, computing: true })) });
  return (
    <>
      {/* The service finishing a job: outside the layer, like the IPC. */}
      <button type="button" data-ui="finish-compute" onClick={() => setView((v) => ({ ...v, computing: false, computed: true }))}>
        finish
      </button>
      <TutorialLayer
      quests={qs}
      view={view}
      step={step}
      initial={initial}
      actions={{
        ...actions,
        goTo: (next) => {
          actions.goTo(next);
          setStep(next);
        },
      }}
    >
      <div data-ui="frame.rail">rail</div>
      <header data-ui="frame.header" style={{ paddingRight: "32px" }}>
        <button type="button" data-ui="primary" onClick={() => setView((v) => ({ ...v, computing: true }))}>
          primary
        </button>
      </header>
      <div data-ui="frame.content">
        <button type="button" data-ui="other" onClick={actions.other}>
          other
        </button>
      </div>
      <button type="button" data-ui="frame.help" onClick={() => setView((v) => ({ ...v, helpOpen: true }))}>
        help
      </button>
      </TutorialLayer>
    </>
  );
}

const host = () => ({ goTo: vi.fn<(step: StepId) => void>(), finish: vi.fn<() => void>(), restart: vi.fn<() => void>(), other: vi.fn<() => void>() });
const ui = (id: string) => document.querySelector<HTMLElement>(`[data-ui="${id}"]`);
const tick = async () => act(async () => vi.advanceTimersByTimeAsync(200));

describe("tutorial layer", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  it("shows a look quest with its outline, glossary and 왜?, and moves on with 계속", async () => {
    renderWith(<Harness actions={host()} />);
    await tick();
    expect(ui("tutorial.coach")?.textContent).toContain("시스템 · 1 / 2");
    expect(ui("tutorial.overlay.look")).not.toBeNull();
    expect(screen.getByText("MIMO")).toBeTruthy();
    fireEvent.click(ui("tutorial.coach.why")!);
    expect(screen.getByText(/단계를 순서대로 밟게 하기 위함입니다/)).toBeTruthy();
    fireEvent.click(ui("tutorial.coach.next")!);
    await tick();
    expect(ui("tutorial.coach")?.textContent).toContain("시스템 · 2 / 2");
    expect(ui("tutorial.overlay.act")).not.toBeNull();
  });

  it("blocks clicks outside the act target, shakes the card and says what to do; the target stays clickable", async () => {
    const h = host();
    renderWith(<Harness actions={h} initial={{ index: 1, completed: [0] }} />);
    await tick();
    fireEvent.pointerDown(ui("other")!);
    fireEvent.click(ui("other")!);
    expect(h.other).not.toHaveBeenCalled();
    expect(ui("tutorial.toast")?.textContent).toContain("강조한 테두리 안을 누르세요.");
    expect(ui("tutorial.coach")?.className).toContain("tutorial-coach--shake");
    fireEvent.click(ui("frame.help")!);
    await tick();
    // Done by the user: the step ends, the toast says so, the next quest is on 보정.
    expect(ui("tutorial.toast")?.textContent).toContain("단계 1/2 끝 · 시스템");
    expect(h.goTo).toHaveBeenCalledWith("calibrate");
  });

  it("arms an act quest on the first 계속 and runs its auto on the second", async () => {
    renderWith(<Harness actions={host()} initial={{ index: 1, completed: [0] }} />);
    await tick();
    const rule = () => ui("tutorial.coach.rule")!;
    // The card shows only what to do; the first 계속 highlights it instead of explaining the double press.
    expect(rule().textContent).toBe(translate("ko", "tutorial.sample.help.title"));
    expect(rule().className).not.toContain("tutorial-coach__rule--armed");
    fireEvent.click(ui("tutorial.coach.next")!);
    expect(rule().className).toContain("tutorial-coach__rule--armed");
    expect(rule().textContent).toBe(translate("ko", "tutorial.sample.help.title"));
    fireEvent.click(ui("tutorial.coach.next")!);
    await tick();
    expect(ui("tutorial.toast")?.textContent).toContain("도움말을 열었습니다.");
  });

  it("waits while busy, shows a blocked reason, and moves on when done", async () => {
    const { rerender } = renderWith(<Harness actions={host()} initial={{ index: 2, completed: [0, 1] }} start={{ blocked: true }} />);
    await tick();
    expect(ui("tutorial.coach.blocked")?.textContent).toContain("이 화면에서 먼저 할 일이 있습니다.");
    rerender(<div />);
    renderWith(<Harness actions={host()} initial={{ index: 2, completed: [0, 1] }} start={{ computing: true }} />);
    await tick();
    expect(ui("tutorial.coach")?.textContent).toContain("계산하는 중입니다");
    expect((ui("tutorial.coach.next") as HTMLButtonElement).disabled).toBe(true);
    fireEvent.click(ui("finish-compute")!);
    await tick();
    expect(ui("tutorial.end")).not.toBeNull();
  });

  it("pauses to a resume button and comes back", async () => {
    const h = host();
    renderWith(<Harness actions={h} />);
    await tick();
    fireEvent.click(ui("tutorial.coach.pause")!);
    expect(ui("tutorial.coach")).toBeNull();
    expect(ui("tutorial.overlay")).toBeNull();
    // Paused: the screen is free.
    fireEvent.click(ui("other")!);
    expect(h.other).toHaveBeenCalledTimes(1);
    fireEvent.click(ui("tutorial.resume")!);
    await tick();
    expect(ui("tutorial.coach")).not.toBeNull();
    // Esc hides it too.
    fireEvent.keyDown(window, { key: "Escape" });
    expect(ui("tutorial.coach")).toBeNull();
    expect(ui("tutorial.resume")).not.toBeNull();
  });

  it("folds the list into a pill and back; restarts and ends", async () => {
    const h = host();
    renderWith(<Harness actions={h} />);
    await tick();
    fireEvent.click(ui("tutorial.panel.fold")!);
    await tick();
    fireEvent.click(ui("tutorial.panel.pill")!);
    await tick();
    expect(ui("tutorial.panel")).not.toBeNull();
    fireEvent.click(ui("tutorial.panel.restart")!);
    fireEvent.click(ui("tutorial.panel.end")!);
    expect(h.restart).toHaveBeenCalledTimes(1);
    expect(h.finish).toHaveBeenCalledTimes(1);
  });

  it("hangs the list, its pill and the resume button from the step header's bottom edge, at the end of its primary action", async () => {
    const { unmount } = renderWith(<Harness actions={host()} />);
    await tick();
    // The shared corner comes from the header: its bottom edge, and its right end less its padding.
    const layer = ui("tutorial")!;
    expect(layer.style.getPropertyValue("--tutorial-line")).toBe("120px");
    expect(layer.style.getPropertyValue("--tutorial-inset")).toBe("32px");
    // None of the three places itself; each takes the corner (and so the same top right).
    expect(ui("tutorial.panel")!.style.top).toBe("");
    fireEvent.click(ui("tutorial.panel.fold")!);
    await tick();
    expect(ui("tutorial.panel.pill")!.style.top).toBe("");
    fireEvent.click(ui("tutorial.coach.pause")!);
    expect(ui("tutorial.resume")!.style.top).toBe("");
    unmount();
    // tutorial.css: one rule for the three, centred on the edge, so the 32 px pill sits half in
    // the header's 16 px bottom padding and half in the content's 24 px top padding (over no card).
    const css = readFileSync(join(import.meta.dirname, "tutorial.css"), "utf8");
    const corner = /\.tutorial-panel,\s*\.tutorial-pill,\s*\.tutorial-resume\s*\{([^}]*)\}/.exec(css)?.[1] ?? "";
    expect(corner).toMatch(/top:\s*calc\(var\(--tutorial-line[^;]*\)\s*-\s*var\(--control-h-sm\)\s*\/\s*2\);/);
    expect(corner).toMatch(/right:\s*var\(--tutorial-inset[^;]*\);/);
    const pill = [...css.matchAll(/\.tutorial-pill,\s*\.tutorial-resume\s*\{([^}]*)\}/g)].map((m) => m[1]).join("");
    expect(pill).toMatch(/\bheight:\s*var\(--control-h-sm\);/);
    const app = readFileSync(join(import.meta.dirname, "../../app/app.css"), "utf8");
    expect(/\.step-header\s*\{[^}]*padding:[^;]*var\(--space-4\)[^;]*;/.exec(app)?.[0]).toMatch(/padding:\s*\S+\s+\S+\s+var\(--space-4\)/);
    expect(/\.app-content\s*\{[^}]*\}/.exec(app)?.[0]).toMatch(/padding:\s*var\(--space-6\)/);
  });

  it("hides the folded pill while its own box would cover an outline, and shows it again when it would not", async () => {
    // A look quest on the rail: the pill is far from it.
    const first = renderWith(<Harness actions={host()} initial={{ folded: true }} />);
    await tick();
    expect(ui("tutorial.panel.pill")!.className).not.toContain("tutorial-pill--hidden");
    expect(ui("tutorial.panel.pill")!.hasAttribute("inert")).toBe(false);
    first.unmount();
    // The act quest on the header's primary action: its ring (and pulse) reaches the pill's box.
    const second = renderWith(<Harness actions={host()} initial={{ index: 2, completed: [0, 1], folded: true }} />);
    await tick();
    const pill = ui("tutorial.panel.pill")!;
    expect(pill.className).toContain("tutorial-pill--hidden");
    // Hidden, it is no Tab stop and nothing to click; it keeps its box to be measured.
    expect(pill.hasAttribute("inert")).toBe(true);
    second.unmount();
    // The same quest with the pill lower (its own box, not a guess): it shows.
    const kept = RECTS["tutorial.panel.pill"]!;
    RECTS["tutorial.panel.pill"] = [1040, 140, 208, 32];
    try {
      renderWith(<Harness actions={host()} initial={{ index: 2, completed: [0, 1], folded: true }} />);
      await tick();
      expect(ui("tutorial.panel.pill")!.className).not.toContain("tutorial-pill--hidden");
    } finally {
      RECTS["tutorial.panel.pill"] = kept;
    }
  });

  it("keeps the coach card off the pill's own box, and off nothing while the pill hides", async () => {
    const card = { w: 340, h: 200 };
    vi.spyOn(HTMLElement.prototype, "offsetWidth", "get").mockImplementation(function (this: HTMLElement) {
      return this.getAttribute("data-ui") === "tutorial.coach" ? card.w : 0;
    });
    vi.spyOn(HTMLElement.prototype, "offsetHeight", "get").mockImplementation(function (this: HTMLElement) {
      return this.getAttribute("data-ui") === "tutorial.coach" ? card.h : 0;
    });
    const coachBox = () => {
      const el = ui("tutorial.coach")!;
      return { x: parseFloat(el.style.left), y: parseFloat(el.style.top), ...card };
    };
    const hits = (a: { x: number; y: number; w: number; h: number }, [x, y, w, h]: [number, number, number, number]) => a.x < x + w && x < a.x + a.w && a.y < y + h && y < a.y + a.h;
    const kept = RECTS["tutorial.panel.pill"]!;
    // A pill that shows beside the outlined primary action, reaching into the card's bounds.
    RECTS["tutorial.panel.pill"] = [1040, 140, 208, 32];
    try {
      const shown = renderWith(<Harness actions={host()} initial={{ index: 2, completed: [0, 1], folded: true }} />);
      await tick();
      expect(ui("tutorial.panel.pill")!.className).not.toContain("tutorial-pill--hidden");
      expect(hits(coachBox(), RECTS["tutorial.panel.pill"])).toBe(false);
      shown.unmount();
    } finally {
      RECTS["tutorial.panel.pill"] = kept;
    }
    // Hidden over the outline, the pill keeps nothing clear: the card goes right under its target (the bounds' top).
    renderWith(<Harness actions={host()} initial={{ index: 2, completed: [0, 1], folded: true }} />);
    await tick();
    expect(ui("tutorial.panel.pill")!.className).toContain("tutorial-pill--hidden");
    expect(coachBox()).toMatchObject({ x: 268 + 996 - card.w, y: 132 });
  });

  it("moves the focus into each card: an alert card on 계속, an act card on itself, then Tab goes round its buttons, the target and the list", async () => {
    const h = host();
    renderWith(<Harness actions={h} />);
    await tick();
    // An alert card: only 계속 to press.
    expect(document.activeElement).toBe(ui("tutorial.coach.next"));
    fireEvent.click(ui("tutorial.coach.next")!);
    await tick();
    // An act card: the card itself, named by its step line and the quest title, described by its text.
    const coach = ui("tutorial.coach")!;
    expect(document.activeElement).toBe(coach);
    expect(coach.tabIndex).toBe(-1);
    const name = (coach.getAttribute("aria-labelledby") ?? "").split(" ").map((id) => document.getElementById(id)?.textContent).join(" ");
    expect(name).toBe("시스템 · 2 / 2 도움말 열기");
    expect(document.getElementById(coach.getAttribute("aria-describedby")!)?.textContent).toContain("도움말을 여세요.");
    const tab = (shiftKey = false) => fireEvent.keyDown(document.activeElement!, { key: "Tab", shiftKey });
    const order = ["tutorial.coach.pause", "tutorial.coach.prev", "tutorial.coach.next", "frame.help", "tutorial.panel.fold", "tutorial.panel.restart", "tutorial.panel.end"];
    for (const id of order) {
      tab();
      expect(document.activeElement, id).toBe(ui(id));
    }
    // Round again; the rest of the app (it does not answer now) is never a stop.
    tab();
    expect(document.activeElement).toBe(ui("tutorial.coach.pause"));
    tab(true);
    expect(document.activeElement).toBe(ui("tutorial.panel.end"));
    ui("tutorial.coach.next")!.focus();
    tab();
    expect(document.activeElement).toBe(ui("frame.help"));
    tab(true);
    expect(document.activeElement).toBe(ui("tutorial.coach.next"));
    // The target answers: the next card (on 보정) takes the focus.
    ui("frame.help")!.focus();
    fireEvent.click(ui("frame.help")!);
    await tick();
    expect(h.goTo).toHaveBeenCalledWith("calibrate");
    expect(document.activeElement).toBe(ui("tutorial.coach"));
  });

  it("focuses 튜토리얼 끝내기 on the end card, and Esc there ends the tutorial; hidden, Esc does nothing", async () => {
    const h = host();
    const { unmount } = renderWith(<Harness actions={h} initial={{ finished: true, index: 2, completed: [0, 1, 2] }} />);
    await tick();
    expect(document.activeElement).toBe(ui("tutorial.end.finish"));
    expect(ui("tutorial.end.finish")?.getAttribute("aria-keyshortcuts")).toBe("Escape");
    fireEvent.keyDown(window, { key: "Escape" });
    expect(h.finish).toHaveBeenCalledTimes(1);
    unmount();
    // Hidden: Esc leaves the app alone (the resume button stays).
    const paused = host();
    renderWith(<Harness actions={paused} initial={{ paused: true, index: 1, completed: [0] }} />);
    await tick();
    fireEvent.keyDown(window, { key: "Escape" });
    expect(paused.finish).not.toHaveBeenCalled();
    expect(ui("tutorial.resume")).not.toBeNull();
  });

  it("offers start over or end at the end", async () => {
    const h = host();
    renderWith(<Harness actions={h} initial={{ finished: true, index: 2, completed: [0, 1, 2] }} />);
    await tick();
    fireEvent.click(ui("tutorial.end.restart")!);
    expect(h.restart).toHaveBeenCalledTimes(1);
    expect(ui("tutorial.coach")).not.toBeNull();
  });

  it("runs the sample quests on the frame ids", () => {
    const qs = sampleQuests({ openHelp: () => {}, compute: () => {} });
    const view: SampleView = { helpOpen: true, computing: false, computed: false };
    expect(qs.map((q) => q.targets[0])).toEqual(["frame.rail", "frame.status", "frame.help", "jobs.tray", "calibrate.primary"]);
    expect(qs[2]?.done?.(view, undefined)).toBe(true);
  });

  it("draws every id the reach table cites", async () => {
    const seen = new Set<string>();
    const states: Partial<TutorialState>[] = [{}, { index: 1, completed: [0], folded: true }, { paused: true }, { finished: true, index: 2, completed: [0, 1, 2] }];
    for (const initial of states) {
      const { unmount } = renderWith(<Harness actions={host()} initial={initial} start={{ blocked: true }} />);
      await tick();
      fireEvent.pointerDown(ui("other")!);
      for (const node of document.querySelectorAll("[data-ui]")) seen.add(node.getAttribute("data-ui") ?? "");
      unmount();
    }
    {
      const { unmount } = renderWith(<Harness actions={host()} initial={{ index: 2, completed: [0, 1] }} start={{ blocked: true }} />);
      await tick();
      for (const node of document.querySelectorAll("[data-ui]")) seen.add(node.getAttribute("data-ui") ?? "");
      unmount();
    }
    expect(REACH_IDS.filter((id) => !seen.has(id))).toEqual([]);
  });
});

/* ── A target that appears after the quest starts ─────────────────────── */

/**
 * One quest on 내보내기 whose target the step screen draws late (after goTo,
 * or once its data arrives), inside a scrolling content area 600 px tall
 * holding 2000 px. happy-dom does no layout: the rects come from RECTS and
 * the content's heights from the spies below.
 */
function LateHarness({ target }: { target: "late.far" | "late.near" }) {
  const [step, setStep] = useState<StepId>("system");
  const [drawn, setDrawn] = useState(false);
  const quest: Quest<View> = { id: "late", step: "export", kind: "look", targets: [target], title: "tutorial.sample.rail.title", text: "tutorial.sample.rail" };
  return (
    <>
      {/* The screen's data arriving (or going): outside the layer, like the IPC. */}
      <button type="button" data-ui="late.toggle" onClick={() => setDrawn((value) => !value)}>
        toggle
      </button>
      <TutorialLayer quests={[quest]} view={{ helpOpen: false, computing: false, computed: false, blocked: false }} step={step} actions={{ ...host(), goTo: setStep }}>
        <div data-ui="frame.content" style={{ overflowY: "auto" }}>
          {step === "export" && drawn && (
            <button type="button" data-ui={target}>
              target
            </button>
          )}
        </div>
      </TutorialLayer>
    </>
  );
}

describe("tutorial layer: a target that appears late", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    const content = (el: Element) => el.getAttribute("data-ui") === "frame.content";
    vi.spyOn(Element.prototype, "scrollHeight", "get").mockImplementation(function (this: Element) {
      return content(this) ? 2000 : 0;
    });
    vi.spyOn(HTMLElement.prototype, "clientHeight", "get").mockImplementation(function (this: HTMLElement) {
      return content(this) ? 600 : 0;
    });
    vi.spyOn(HTMLElement.prototype, "offsetHeight", "get").mockImplementation(function (this: HTMLElement) {
      return this.getAttribute("data-ui")?.startsWith("late.") ? 36 : 0;
    });
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  const contentEl = () => ui("frame.content")!;

  it("scrolls it into the content once, the first time it appears, and not again after the user scrolls", async () => {
    renderWith(<LateHarness target="late.far" />);
    await tick();
    // The quest went to its step, but the screen has not drawn the target yet.
    expect(ui("tutorial.coach")).not.toBeNull();
    expect(ui("late.far")).toBeNull();
    expect(contentEl().scrollTop).toBe(0);
    fireEvent.click(ui("late.toggle")!);
    await tick();
    // 1500 − 120 (the content's top) + 36 − 600 + 20: its bottom 20 px above the content's.
    expect(contentEl().scrollTop).toBe(836);
    // The user scrolls back up; the screen draws the target again (new data): no pull.
    contentEl().scrollTop = 0;
    fireEvent.click(ui("late.toggle")!);
    await tick();
    expect(ui("late.far")).toBeNull();
    fireEvent.click(ui("late.toggle")!);
    await tick();
    await tick();
    expect(ui("late.far")).not.toBeNull();
    expect(contentEl().scrollTop).toBe(0);
  });

  it("leaves a target that appears already in view where it is", async () => {
    renderWith(<LateHarness target="late.near" />);
    await tick();
    contentEl().scrollTop = 40;
    fireEvent.click(ui("late.toggle")!);
    await tick();
    expect(ui("late.near")).not.toBeNull();
    expect(contentEl().scrollTop).toBe(40);
  });
});
