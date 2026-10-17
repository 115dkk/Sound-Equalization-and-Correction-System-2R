// The tutorial's moves as pure functions: the quest flow (upstream
// tutorial_overlay.Tutorial.next/prev/next_pressed/pause/resume), the
// glossary placement (tutorial_quests.glossary_for) and where the coach
// card goes (Tutorial._coach_spot). The component wires them to the screen.
import { STEPS, type StepId } from "../../app/model";
import type { GlossaryTerm, Quest, Said, TutorialState } from "./types";

export const INITIAL: TutorialState = {
  index: 0,
  completed: [],
  cleared: [],
  armed: false,
  whyOpen: false,
  paused: false,
  folded: false,
  userOpened: false,
  finished: false,
  dir: 1,
  toast: null,
  autoBlocked: false,
  shake: 0,
};

type Meta = { step: StepId };

export type TutorialAction =
  | { type: "next"; quests: readonly Meta[]; praise?: Said["key"] | undefined; note?: Said["key"] | undefined }
  | { type: "prev"; quests: readonly Meta[] }
  | { type: "skip"; quests: readonly Meta[] }
  | { type: "arm" }
  | { type: "auto_blocked" }
  | { type: "why"; open: boolean }
  | { type: "pause" }
  | { type: "resume"; index: number; quests: readonly Meta[]; rewound: boolean }
  | { type: "fold"; folded: boolean; byUser?: boolean }
  | { type: "outside"; act: boolean }
  | { type: "toast_done" }
  | { type: "restart" };

/** Steps in rail order that have quests, for the "단계 k/n 끝" count. */
export function questSteps(quests: readonly Meta[]): StepId[] {
  return STEPS.filter((step) => quests.some((quest) => quest.step === step));
}

/** The quest at `index` moved to: clears per-quest flags. */
function showing(state: TutorialState, index: number, dir: 1 | -1): TutorialState {
  return { ...state, index, dir, armed: false, whyOpen: false, autoBlocked: false, userOpened: false };
}

export function tutorialReducer(state: TutorialState, action: TutorialAction): TutorialState {
  switch (action.type) {
    case "next": {
      const { quests } = action;
      const here = quests[state.index];
      if (!here || state.finished) return state;
      const completed = state.completed.includes(state.index) ? state.completed : [...state.completed, state.index];
      const following = quests[state.index + 1];
      let cleared = state.cleared;
      let toast: Said | null = null;
      if (!following || following.step !== here.step) {
        cleared = cleared.includes(here.step) ? cleared : [...cleared, here.step];
        const steps = questSteps(quests);
        toast = { key: "tutorial.toast.step_done", params: { done: cleared.length, total: steps.length, step: here.step }, ...(action.note ? { note: action.note } : {}) };
      } else if (action.note) {
        toast = { key: action.note };
      } else if (action.praise) {
        toast = { key: action.praise };
      }
      if (!following) return { ...state, completed, cleared, toast, finished: true, armed: false, autoBlocked: false };
      return { ...showing({ ...state, completed, cleared, toast }, state.index + 1, 1) };
    }
    case "prev": {
      if (state.paused || state.index <= 0 || state.finished) return state;
      const index = state.index - 1;
      const step = action.quests[index]?.step;
      return showing({ ...state, cleared: state.cleared.filter((s) => s !== step), toast: null }, index, -1);
    }
    case "skip": {
      // The act quest's target is missing (upstream _skip): keep moving the way we were going.
      if (state.dir < 0 && state.index > 0) return showing(state, state.index - 1, -1);
      return tutorialReducer(state, { type: "next", quests: action.quests });
    }
    case "arm":
      return { ...state, armed: true, shake: state.shake + 1 };
    case "auto_blocked":
      return { ...state, autoBlocked: true, shake: state.shake + 1 };
    case "why":
      return { ...state, whyOpen: action.open };
    case "pause":
      return state.paused || state.finished ? state : { ...state, paused: true, toast: null };
    case "resume": {
      if (!state.paused) return state;
      const resumed = { ...state, paused: false };
      if (!action.rewound) return resumed;
      const index = action.index;
      const step = action.quests[index]?.step;
      const order = STEPS.indexOf(step ?? "system");
      return {
        ...showing(resumed, index, 1),
        completed: state.completed.filter((i) => i < index),
        cleared: state.cleared.filter((s) => STEPS.indexOf(s) < order),
        toast: { key: "tutorial.toast.rewound", params: { step: step ?? "system" } },
      };
    }
    case "fold":
      return { ...state, folded: action.folded, userOpened: action.byUser ? !action.folded : state.userOpened };
    case "outside":
      return {
        ...state,
        shake: state.shake + 1,
        toast: state.toast ?? { key: action.act ? "tutorial.toast.outside_act" : "tutorial.toast.outside_look" },
      };
    case "toast_done":
      return { ...state, toast: null };
    case "restart":
      return { ...INITIAL };
  }
}

/**
 * Where to go back to on resume (upstream _resume_index): if an earlier step
 * can no longer be left (`stepOpen` false), its first unfinished act quest
 * that is not the gate, else its gate quest (`gate`, upstream `_next_to`:
 * there every quest with `blocked` is one), else its first quest.
 */
export function resumeIndex<V>(quests: readonly Quest<V>[], index: number, view: V, stepOpen?: (view: V, step: StepId) => boolean): number {
  const here = quests[index];
  if (!here || !stepOpen) return index;
  for (const step of STEPS.slice(0, STEPS.indexOf(here.step))) {
    if (stepOpen(view, step)) continue;
    const same = quests.map((quest, i) => ({ quest, i })).filter(({ quest, i }) => i < index && quest.step === step);
    if (same.length === 0) continue;
    const todo = same.find(({ quest }) => quest.kind === "act" && quest.done && !quest.base && !quest.gate && !quest.done(view, undefined));
    if (todo) return todo.i;
    const gate = same.find(({ quest }) => quest.gate);
    return (gate ?? same[0])!.i;
  }
  return index;
}

/** For each quest index, the glossary terms first named there (under the text, then under 왜?). */
export function glossaryFor<V>(quests: readonly Quest<V>[]): Map<number, { text: GlossaryTerm[]; why: GlossaryTerm[] }> {
  const out = new Map<number, { text: GlossaryTerm[]; why: GlossaryTerm[] }>();
  const seen = new Set<GlossaryTerm>();
  quests.forEach((quest, index) => {
    for (const slot of ["text", "why"] as const) {
      for (const term of (slot === "text" ? quest.terms : quest.whyTerms) ?? []) {
        if (seen.has(term)) continue;
        seen.add(term);
        const entry = out.get(index) ?? { text: [], why: [] };
        entry[slot].push(term);
        out.set(index, entry);
      }
    }
  });
  return out;
}

/* ── Geometry ───────────────────────────────────────────────────────────── */

export type Box = { x: number; y: number; w: number; h: number };

export const pad = (box: Box, by: number): Box => ({ x: box.x - by, y: box.y - by, w: box.w + 2 * by, h: box.h + 2 * by });

export function union(boxes: readonly Box[]): Box | null {
  if (boxes.length === 0) return null;
  const x = Math.min(...boxes.map((b) => b.x));
  const y = Math.min(...boxes.map((b) => b.y));
  const right = Math.max(...boxes.map((b) => b.x + b.w));
  const bottom = Math.max(...boxes.map((b) => b.y + b.h));
  return { x, y, w: right - x, h: bottom - y };
}

export function overlap(a: Box, b: Box): number {
  const w = Math.min(a.x + a.w, b.x + b.w) - Math.max(a.x, b.x);
  const h = Math.min(a.y + a.h, b.y + b.h) - Math.max(a.y, b.y);
  return w > 0 && h > 0 ? w * h : 0;
}

/**
 * The coach card's top-left: inside `bounds`, beside the target (below,
 * above, right, left; each aligned to either end), beside the places to keep
 * clear, or in a corner inside the target. The spot covering the least of
 * the target (weight 20) and of the places to keep clear (weight 10) wins;
 * among equals, the one nearest the target. `prefer` (the current spot)
 * stays while it covers nothing, so a card that grows does not jump. No
 * target: centred.
 */
export function placeCoach(targets: readonly Box[], card: { w: number; h: number }, bounds: Box, avoid: readonly Box[] = [], prefer?: { x: number; y: number }): { x: number; y: number } {
  const lo = { x: bounds.x, y: bounds.y };
  const hi = { x: Math.max(bounds.x, bounds.x + bounds.w - card.w), y: Math.max(bounds.y, bounds.y + bounds.h - card.h) };
  const clamp = (x: number, y: number) => ({ x: Math.min(Math.max(lo.x, x), hi.x), y: Math.min(Math.max(lo.y, y), hi.y) });
  const all = union(targets);
  if (!all) return clamp(bounds.x + (bounds.w - card.w) / 2, bounds.y + (bounds.h - card.h) / 2);
  const covered = (at: { x: number; y: number }) => {
    const box = { x: at.x, y: at.y, w: card.w, h: card.h };
    return 20 * targets.reduce((sum, t) => sum + overlap(box, t), 0) + 10 * avoid.reduce((sum, a) => sum + overlap(box, a), 0);
  };
  if (prefer) {
    const kept = clamp(prefer.x, prefer.y);
    if (covered(kept) === 0) return kept;
  }
  const gap = 14;
  const spots: { x: number; y: number }[] = [];
  for (const x of [all.x, all.x + all.w - card.w]) spots.push(clamp(x, all.y + all.h + gap));
  for (const x of [all.x, all.x + all.w - card.w]) spots.push(clamp(x, all.y - gap - card.h));
  for (const y of [all.y, all.y + all.h - card.h]) {
    spots.push(clamp(all.x + all.w + gap, y));
    spots.push(clamp(all.x - gap - card.w, y));
  }
  for (const spot of spots.slice(0, 4)) {
    for (const a of avoid) {
      spots.push(clamp(spot.x, a.y + a.h + gap));
      spots.push(clamp(a.x - gap - card.w, spot.y));
    }
  }
  for (const [x, y] of [
    [all.x + all.w - card.w - 12, all.y + all.h - card.h - 12],
    [all.x + all.w - card.w - 12, all.y + 12],
    [all.x + 12, all.y + all.h - card.h - 12],
    [all.x + 12, all.y + 12],
  ] as const) {
    spots.push(clamp(x, y));
  }
  const cx = all.x + all.w / 2;
  const cy = all.y + all.h / 2;
  let best: { cost: number; at: { x: number; y: number } } | null = null;
  for (const at of spots) {
    const distance = Math.hypot(at.x + card.w / 2 - cx, at.y + card.h / 2 - cy);
    const cost = covered(at) + distance;
    if (!best || cost < best.cost) best = { cost, at };
  }
  return best!.at;
}
