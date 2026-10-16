// @vitest-environment happy-dom
import { act, fireEvent, screen } from "@testing-library/react";
import { useState } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { STEPS, canLeave, type FrameModel, type StepId } from "../../app/model";
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
  "frame.content": [252, 120, 1028, 620],
  other: [400, 300, 120, 36],
  primary: [1100, 40, 160, 36],
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
      <div data-ui="frame.content">
        <button type="button" data-ui="other" onClick={actions.other}>
          other
        </button>
        <button type="button" data-ui="primary" onClick={() => setView((v) => ({ ...v, computing: true }))}>
          primary
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
    expect(screen.getByText(/단계는 순서대로 이어집니다/)).toBeTruthy();
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
    expect(ui("tutorial.coach.rule")?.textContent).toContain("'계속'을 두 번 누르면");
    fireEvent.click(ui("tutorial.coach.next")!);
    expect(ui("tutorial.coach.rule")?.textContent).toContain("한 번 더 누르면");
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
