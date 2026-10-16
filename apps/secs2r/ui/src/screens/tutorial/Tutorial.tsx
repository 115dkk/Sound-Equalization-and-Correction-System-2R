// The tutorial layer (upstream tutorial_overlay.py): it wraps the frame and
// draws, on top of it, the dim with holes for the quest's targets, their
// outlines (look: dashed, not clickable; act: the tutorial accent with a
// pulsing ring, clickable), the coach card, the quest list (folds into a
// pill), the toast, and the resume button while paused. Clicks outside an
// act target shake the card and say what to do; the wheel always scrolls.
// A card that opens takes the focus; Tab moves between the tutorial's own
// controls and the act target (the rest of the app does not answer), and
// Esc leaves: it hides the cards, and on the end card it ends the tutorial.
import { useCallback, useEffect, useId, useLayoutEffect, useMemo, useReducer, useRef, useState, type KeyboardEvent as ReactKeyboardEvent, type PointerEvent as ReactPointerEvent, type ReactNode, type Ref, type SyntheticEvent } from "react";
import { STEP_TITLE, type StepId } from "../../app/model";
import type { MessageKey } from "../../i18n";
import { useT, type Translate } from "../../i18n/react";
import { Button, IconButton, ProgressBar, Spinner } from "../../ui";
import { Icon } from "../../ui/Icon";
import { INITIAL, glossaryFor, overlap, pad, placeCoach, resumeIndex, tutorialReducer, union, type Box } from "./engine";
import { GLOSSARY } from "./glossary";
import type { GlossaryTerm, Quest, Said, TutorialActions, TutorialState } from "./types";
import "./tutorial.css";

const HOLE = 6;
const TICK_MS = 150;
const FOCUSABLE = "button, [href], input, select, textarea, [tabindex]";

type Geo = {
  /** One box per outlined group (apart), or one around all. Layer coordinates, padded. */
  rings: Box[];
  /** The content area (frame.content) or the whole layer. */
  bounds: Box;
  layer: { w: number; h: number };
  /** Boxes the card must not cover (quest.keep). */
  keep: Box[];
  missing: boolean;
};

const EMPTY_GEO: Geo = { rings: [], bounds: { x: 0, y: 0, w: 0, h: 0 }, layer: { w: 0, h: 0 }, keep: [], missing: false };

function said(t: Translate, value: Said): string {
  const params = { ...value.params };
  if (typeof params.step === "string" && params.step in STEP_TITLE) params.step = t(STEP_TITLE[params.step as StepId]);
  return t(value.key, params);
}

export function TutorialLayer<V>({
  quests: all,
  view,
  step,
  actions,
  stepOpen,
  initial,
  toastMs = 1800,
  children,
}: {
  quests: readonly Quest<V>[];
  /** The app view the quests' predicates read (P33/P35). */
  view: V;
  /** The step the frame shows now. */
  step: StepId;
  actions: TutorialActions;
  /** Whether a step can be left now (upstream can_advance_step): resume goes back to the first step that cannot. */
  stepOpen?: ((view: V, step: StepId) => boolean) | undefined;
  /** Gallery and tests: start in a given state. */
  initial?: Partial<TutorialState> | undefined;
  /** How long a toast stays; 0 keeps it (gallery). */
  toastMs?: number | undefined;
  children: ReactNode;
}) {
  const t = useT();
  const [quests, setQuests] = useState(() => all.filter((quest) => !quest.when || quest.when(view)));
  const [state, dispatch] = useReducer(tutorialReducer, { ...INITIAL, ...initial });
  const gloss = useMemo(() => glossaryFor(quests), [quests]);
  const layerRef = useRef<HTMLDivElement>(null);
  const coachRef = useRef<HTMLDivElement>(null);
  const panelRef = useRef<HTMLElement>(null);
  const pillRef = useRef<HTMLButtonElement>(null);
  const endRef = useRef<HTMLDivElement>(null);
  const baseRef = useRef<unknown>(undefined);
  const missingRef = useRef(0);
  const viewRef = useRef(view);
  viewRef.current = view;
  const [geo, setGeo] = useState<Geo>(EMPTY_GEO);
  const [spot, setSpot] = useState<{ x: number; y: number; key: string } | null>(null);
  const [dragged, setDragged] = useState<{ x: number; y: number } | null>(null);

  const quest = state.finished ? undefined : quests[state.index];
  const engaged = !state.paused && !state.finished && !!quest;
  const done = state.completed.includes(state.index);
  const pending = quest?.kind === "act" && !done;
  const busy = pending && !!quest?.busy?.when(view);
  const blockedReason: MessageKey | null = pending ? (quest?.blocked?.(view) ?? (state.autoBlocked ? "tutorial.coach.blocked.default" : null)) : null;

  /* ── Measuring the targets ── */

  const elementsOf = useCallback((ids: readonly string[]): HTMLElement[] => {
    const layer = layerRef.current;
    if (!layer) return [];
    return ids.flatMap((id) => [...layer.querySelectorAll<HTMLElement>(`[data-ui="${id}"]`)].filter((el) => !el.closest("[data-tutorial-ui]")));
  }, []);

  const measure = useCallback(() => {
    const layer = layerRef.current;
    if (!layer) return;
    const lr = layer.getBoundingClientRect();
    const rel = (r: DOMRect): Box => ({ x: r.left - lr.left, y: r.top - lr.top, w: r.width, h: r.height });
    const contentEl = layer.querySelector<HTMLElement>('[data-ui="frame.content"]');
    const content = contentEl ? rel(contentEl.getBoundingClientRect()) : null;
    const clip = (box: Box, el: HTMLElement): Box | null => {
      const inside = content && contentEl?.contains(el) ? content : { x: 0, y: 0, w: lr.width, h: lr.height };
      const x = Math.max(box.x, inside.x);
      const y = Math.max(box.y, inside.y);
      const w = Math.min(box.x + box.w, inside.x + inside.w) - x;
      const h = Math.min(box.y + box.h, inside.y + inside.h) - y;
      return w > 0 && h > 0 ? { x, y, w, h } : null;
    };
    const groups: Box[] = [];
    let found = 0;
    for (const group of quest?.targets ?? []) {
      const ids = typeof group === "string" ? [group] : group;
      const boxes = elementsOf(ids)
        .map((el) => {
          const r = el.getBoundingClientRect();
          return r.width > 0 && r.height > 0 ? clip(rel(r), el) : null;
        })
        .filter((box): box is Box => box !== null);
      found += boxes.length;
      const all = union(boxes);
      if (all) groups.push(all);
    }
    const rings = quest?.apart ? groups : groups.length ? [union(groups)!] : [];
    const keep = elementsOf(quest?.keep ?? []).map((el) => rel(el.getBoundingClientRect()));
    const bounds = content ? { x: content.x + 16, y: content.y + 12, w: content.w - 32, h: content.h - 24 } : { x: 16, y: 16, w: lr.width - 32, h: lr.height - 32 };
    // Outlines stay inside the layer (a target at the edge, like the job tray, keeps its whole outline in view).
    const inLayer = (r: Box): Box => {
      const x = Math.max(2, r.x);
      const y = Math.max(2, r.y);
      return { x, y, w: Math.min(r.x + r.w, lr.width - 2) - x, h: Math.min(r.y + r.h, lr.height - 2) - y };
    };
    const next: Geo = { rings: rings.map((r) => inLayer(pad(r, HOLE))), bounds, layer: { w: lr.width, h: lr.height }, keep, missing: (quest?.targets.length ?? 0) > 0 && found === 0 };
    setGeo((prev) => (JSON.stringify(prev) === JSON.stringify(next) ? prev : next));
  }, [quest, elementsOf]);

  /* ── Showing a quest ── */

  useEffect(() => {
    if (!quest || state.paused) return;
    if (step !== quest.step) actions.goTo(quest.step);
    baseRef.current = quest.kind === "act" && quest.base ? quest.base(viewRef.current) : undefined;
    missingRef.current = 0;
    setDragged(null);
    setSpot(null);
    // Scroll the first target into view inside its own scroll area (never the page).
    const first = typeof quest.targets[0] === "string" ? quest.targets[0] : quest.targets[0]?.[0];
    const el = first ? elementsOf([first])[0] : undefined;
    if (el) revealInside(el, layerRef.current);
    // Only when the quest changes or the tutorial comes back.
  }, [state.index, state.paused, state.finished, quests]);

  // Upstream animate: move the screen while the card shows.
  useEffect(() => {
    const animate = quest?.animate;
    if (!animate || !engaged) return;
    let k = 0;
    animate.step(k++);
    const timer = window.setInterval(() => animate.step(k++), animate.every);
    return () => {
      window.clearInterval(timer);
      animate.end?.();
    };
  }, [quest, engaged]);

  // Finished act quests move on (upstream _tick: done, unless busy).
  useEffect(() => {
    if (!engaged || !quest || quest.kind !== "act" || done || busy) return;
    if (quest.done?.(view, baseRef.current)) dispatch({ type: "next", quests, praise: quest.ok ?? "tutorial.toast.ok" });
  }, [engaged, quest, done, busy, view, quests]);

  // Measure now and every 150 ms; skip an act quest whose target stays missing (> 0.45 s).
  useLayoutEffect(() => {
    if (!quest && !state.finished) return;
    measure();
    const timer = window.setInterval(() => {
      measure();
      if (quest?.kind === "act" && !done && !state.paused) {
        const missing = elementsOf(quest.targets.flatMap((g) => (typeof g === "string" ? [g] : [...g]))).length === 0;
        missingRef.current = missing ? missingRef.current + 1 : 0;
        if (missingRef.current >= 3) {
          missingRef.current = 0;
          dispatch({ type: "skip", quests });
        }
      }
    }, TICK_MS);
    const onResize = () => measure();
    window.addEventListener("resize", onResize);
    document.addEventListener("scroll", onResize, true);
    return () => {
      window.clearInterval(timer);
      window.removeEventListener("resize", onResize);
      document.removeEventListener("scroll", onResize, true);
    };
  }, [state.paused, state.finished, measure, quest, done, elementsOf, quests]);

  // Esc is the way out of whatever the tutorial shows: it hides the coach
  // card for now (the card's eye button does the same), and on the end card
  // it ends the tutorial (the card's 튜토리얼 끝내기, through the host's flow).
  // While hidden it does nothing. A dialog or panel that handles Esc itself
  // stops it before it gets here.
  const finishRef = useRef(actions.finish);
  finishRef.current = actions.finish;
  useEffect(() => {
    if (!engaged && !(state.finished && !state.paused)) return;
    const finished = state.finished;
    const onKey = (event: KeyboardEvent) => {
      if (event.key !== "Escape" || event.defaultPrevented) return;
      if (finished) finishRef.current();
      else dispatch({ type: "pause" });
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [engaged, state.finished, state.paused]);

  // A card that opens takes the focus once: an alert card (a look quest, or
  // an act quest already done) on its 계속; an act card on itself, so Tab
  // goes on to its buttons and the target; the end card on 튜토리얼 끝내기.
  // The coach card is placed after measuring, so this waits until it shows.
  const opened = state.paused ? null : state.finished ? "end" : engaged ? `quest.${state.index}` : null;
  const focusedRef = useRef<string | null>(null);
  useEffect(() => {
    if (opened === null) {
      focusedRef.current = null;
      return;
    }
    if (focusedRef.current === opened) return;
    const card = opened === "end" ? endRef.current : coachRef.current;
    if (!card || card.style.visibility === "hidden") return;
    const into = opened === "end" ? card.querySelector<HTMLElement>('[data-ui="tutorial.end.finish"]') : pending ? card : card.querySelector<HTMLElement>('[data-ui="tutorial.coach.next"]');
    (into ?? card).focus({ preventScroll: true });
    focusedRef.current = opened;
  });

  // Toasts leave on their own.
  useEffect(() => {
    if (!state.toast || toastMs === 0) return;
    const timer = window.setTimeout(() => dispatch({ type: "toast_done" }), toastMs);
    return () => window.clearTimeout(timer);
  }, [state.toast, toastMs]);

  // Place the coach card away from the target, the rail and the header
  // (bounds = the content area). While the target stays put, the current
  // spot wins ties, so a card that grows (왜?) stays where it is.
  useLayoutEffect(() => {
    const coach = coachRef.current;
    if (!coach || !engaged || dragged || geo.layer.w === 0) return;
    const card = { w: coach.offsetWidth, h: coach.offsetHeight };
    const avoid = [...geo.keep];
    // The quest list (or its pill) keeps its corner; the card goes elsewhere.
    for (const el of [panelRef.current, pillRef.current]) if (el) avoid.push({ x: el.offsetLeft, y: el.offsetTop, w: el.offsetWidth, h: el.offsetHeight });
    const key = JSON.stringify([state.index, geo.rings, geo.bounds, geo.keep, state.folded]);
    const prefer = spot && spot.key === key ? spot : undefined;
    const next = placeCoach(geo.rings, card, geo.bounds, avoid, prefer);
    if (!spot || spot.key !== key || next.x !== spot.x || next.y !== spot.y) setSpot({ ...next, key });
  });

  // Fold the quest list when it covers the target or the card (unless the user opened it: then only the target).
  useLayoutEffect(() => {
    const panel = panelRef.current;
    const coach = coachRef.current;
    if (!panel || state.folded || !engaged) return;
    const box = { x: panel.offsetLeft, y: panel.offsetTop, w: panel.offsetWidth, h: panel.offsetHeight };
    const onTarget = geo.rings.some((ring) => overlap(box, pad(ring, 24)) > 0);
    const onCard = !state.userOpened && coach && overlap(box, { x: coach.offsetLeft, y: coach.offsetTop, w: coach.offsetWidth, h: coach.offsetHeight }) > 0;
    if (onTarget || onCard) dispatch({ type: "fold", folded: true });
  });

  /* ── Moves ── */

  const onContinue = () => {
    if (!quest) return;
    if (quest.kind === "act" && !done) {
      if (busy) return;
      if (!state.armed) {
        dispatch({ type: "arm" });
        return;
      }
      if (quest.auto) {
        const result = quest.auto.run(view);
        if (result === false) {
          dispatch({ type: "auto_blocked" });
          return;
        }
        dispatch({ type: "next", quests, note: quest.auto.note });
        return;
      }
    }
    dispatch({ type: "next", quests });
  };

  const onResume = () => {
    const index = resumeIndex(quests, state.index, view, stepOpen);
    dispatch({ type: "resume", index, quests, rewound: index !== state.index });
  };

  const onRestart = () => {
    actions.restart();
    setQuests(all.filter((q) => !q.when || q.when(view)));
    dispatch({ type: "restart" });
  };

  /* ── Tab order ── */

  // Not a modal trap: the act target lives in the app. While the focus is on
  // a card, its buttons, the act target or the quest list, Tab goes round
  // those in that order; the rest of the app does not answer clicks then, so
  // it is skipped. Focus elsewhere moves as the browser moves it.
  const tabRing = (): { home: HTMLElement | null; ring: HTMLElement[] } => {
    if (state.paused) return { home: null, ring: [] };
    if (state.finished) return { home: endRef.current, ring: tabStops(endRef.current) };
    if (!engaged || !quest) return { home: null, ring: [] };
    const targets = pending ? elementsOf(quest.targets.flatMap((g) => (typeof g === "string" ? [g] : [...g]))).flatMap(targetStops) : [];
    const list = panelRef.current ?? pillRef.current;
    return { home: coachRef.current, ring: [...tabStops(coachRef.current), ...targets, ...(list === pillRef.current ? targetStops(list) : tabStops(list))] };
  };

  const onKeyDownCapture = (event: ReactKeyboardEvent) => {
    if (event.key !== "Tab" || event.altKey || event.ctrlKey || event.metaKey) return;
    const { home, ring } = tabRing();
    if (ring.length === 0) return;
    const active = document.activeElement;
    const at = ring.findIndex((el) => el === active);
    if (at < 0 && (!home || active !== home)) return;
    event.preventDefault();
    const next = at < 0 ? (event.shiftKey ? ring.length - 1 : 0) : (at + (event.shiftKey ? -1 : 1) + ring.length) % ring.length;
    ring[next]?.focus();
  };

  /* ── Click guard ── */

  const guard = (event: SyntheticEvent) => {
    if (state.paused) return;
    const layer = layerRef.current;
    const target = event.target as Element | null;
    if (!layer || !target || !layer.contains(target)) return; // portalled menus, dialogs and toasts
    if (target.closest?.("[data-tutorial-ui]")) return;
    if (!state.finished && pending && quest) {
      const ids = quest.targets.flatMap((g) => (typeof g === "string" ? [g] : [...g]));
      if (elementsOf(ids).some((el) => el.contains(target))) return;
    }
    event.preventDefault();
    event.stopPropagation();
    if (event.type === "pointerdown" && !state.finished) dispatch({ type: "outside", act: pending });
  };

  /* ── Drawing ── */

  const stepQuests = quest ? quests.filter((q) => q.step === quest.step) : [];
  const k = quest ? stepQuests.indexOf(quest) : -1;
  const stepName = quest ? t(STEP_TITLE[quest.step]) : "";
  const glossary = gloss.get(state.index);
  const coachAt = dragged ?? spot;

  return (
    <div
      ref={layerRef}
      className="tutorial-layer"
      data-ui="tutorial"
      onPointerDownCapture={guard}
      onMouseDownCapture={guard}
      onClickCapture={guard}
      onKeyDownCapture={onKeyDownCapture}
    >
      {children}
      {!state.paused && (state.finished || quest) && (
        <div className="tutorial-overlay" aria-hidden="true" data-ui="tutorial.overlay">
          <Dim w={geo.layer.w} h={geo.layer.h} holes={state.finished ? [] : geo.rings} />
          {!state.finished &&
            geo.rings.map((ring, i) => (
              <span
                key={i}
                className={`tutorial-ring tutorial-ring--${quest?.kind === "act" && !done ? "act" : "look"}`}
                data-ui={`tutorial.overlay.${quest?.kind === "act" && !done ? "act" : "look"}`}
                style={{ left: ring.x, top: ring.y, width: ring.w, height: ring.h }}
              />
            ))}
        </div>
      )}

      {engaged && quest && (
        <Coach
          cardRef={coachRef}
          at={coachAt}
          quest={quest}
          head={t("tutorial.coach.head", { step: stepName, n: k + 1, total: stepQuests.length })}
          state={state}
          pending={pending}
          busy={busy}
          blocked={blockedReason}
          terms={glossary}
          first={state.index === 0}
          onWhy={(open) => dispatch({ type: "why", open })}
          onPrev={() => dispatch({ type: "prev", quests })}
          onNext={onContinue}
          onPause={() => dispatch({ type: "pause" })}
          onDrag={(at) => setDragged(at)}
          bounds={geo.layer}
        />
      )}

      {engaged && quest && !state.folded && (
        <QuestPanel
          panelRef={panelRef}
          top={geo.bounds.y}
          stepName={stepName}
          items={stepQuests.map((q) => ({ id: q.id, title: t(q.title), state: quests.indexOf(q) < state.index ? "done" : q === quest ? "current" : "pending" }))}
          current={k}
          onFold={() => dispatch({ type: "fold", folded: true, byUser: true })}
          onRestart={onRestart}
          onEnd={actions.finish}
        />
      )}
      {engaged && quest && state.folded && !pillCovers(geo) && (
        <button ref={pillRef} type="button" className="tutorial-pill" style={{ top: geo.bounds.y }} data-tutorial-ui data-ui="tutorial.panel.pill" aria-label={t("tutorial.panel.unfold")} onClick={() => dispatch({ type: "fold", folded: false, byUser: true })}>
          <span className="tabular">{t("tutorial.panel.pill", { step: stepName, n: k + 1, total: stepQuests.length })}</span>
          <Icon name="chevronDown" size={16} />
        </button>
      )}

      {state.paused && (
        <button type="button" className="tutorial-resume" style={{ top: geo.bounds.y }} data-tutorial-ui data-ui="tutorial.resume" onClick={onResume}>
          <EyeIcon />
          <span>{t("tutorial.resume")}</span>
        </button>
      )}

      {state.finished && !state.paused && <EndCard cardRef={endRef} at={centre(geo)} onRestart={onRestart} onFinish={actions.finish} />}

      {state.toast && !state.paused && (
        <div className="tutorial-toast" style={toastAt(geo)} data-tutorial-ui data-ui="tutorial.toast" role="status" aria-live="polite">
          <span>{said(t, state.toast)}</span>
          {state.toast.note && <span className="tutorial-toast__note">{t(state.toast.note)}</span>}
        </div>
      )}
    </div>
  );
}

/** The middle of the content area (layer coordinates), for the toast and the end card. */
function centre(geo: Geo): { left: number; top: number } | undefined {
  return geo.layer.w ? { left: geo.bounds.x + geo.bounds.w / 2, top: geo.bounds.y + geo.bounds.h / 2 } : undefined;
}

function toastAt(geo: Geo): { left: number } | undefined {
  const at = centre(geo);
  return at ? { left: at.left } : undefined;
}

/** A folded pill at the top right hides while it would cover an outline (upstream: the pill covered the ring's corner). */
function pillCovers(geo: Geo): boolean {
  const pill = { x: geo.layer.w - 12 - 280, y: geo.bounds.y, w: 280, h: 36 };
  return geo.rings.some((ring) => overlap(pill, pad(ring, 14)) > 0);
}

const enabled = (el: HTMLElement) => !(el as HTMLButtonElement).disabled && !el.closest("[hidden], [inert]");

/** The Tab stops inside `root`, in document order. */
function tabStops(root: Element | null | undefined): HTMLElement[] {
  return root ? [...root.querySelectorAll<HTMLElement>(FOCUSABLE)].filter((el) => el.tabIndex >= 0 && enabled(el)) : [];
}

/** An act target: itself when it is a control (a segment that is not chosen too), else the Tab stops inside it. */
function targetStops(el: HTMLElement | null | undefined): HTMLElement[] {
  if (!el) return [];
  return el.matches(FOCUSABLE) ? (enabled(el) ? [el] : []) : tabStops(el);
}

/** Scrolls `el` into view inside its nearest scrolling ancestor below `stop`. */
function revealInside(el: HTMLElement, stop: HTMLElement | null) {
  for (let node = el.parentElement; node && node !== stop; node = node.parentElement) {
    const style = getComputedStyle(node);
    if (!/(auto|scroll)/.test(style.overflowY) || node.scrollHeight <= node.clientHeight) continue;
    const top = el.getBoundingClientRect().top - node.getBoundingClientRect().top + node.scrollTop;
    const bottom = top + el.offsetHeight;
    if (top < node.scrollTop + 20) node.scrollTop = Math.max(0, top - 20);
    else if (bottom > node.scrollTop + node.clientHeight - 20) node.scrollTop = bottom - node.clientHeight + 20;
    return;
  }
}

function Dim({ w, h, holes }: { w: number; h: number; holes: readonly Box[] }) {
  if (w === 0 || h === 0) return null;
  const r = 10;
  const hole = ({ x, y, w: bw, h: bh }: Box) =>
    `M${x + r},${y}H${x + bw - r}A${r},${r} 0 0 1 ${x + bw},${y + r}V${y + bh - r}A${r},${r} 0 0 1 ${x + bw - r},${y + bh}H${x + r}A${r},${r} 0 0 1 ${x},${y + bh - r}V${y + r}A${r},${r} 0 0 1 ${x + r},${y}Z`;
  return (
    <svg className="tutorial-dim" width={w} height={h} viewBox={`0 0 ${w} ${h}`}>
      <path fillRule="evenodd" d={`M0,0H${w}V${h}H0Z ${holes.map(hole).join(" ")}`} />
    </svg>
  );
}

function EyeIcon({ hidden = false }: { hidden?: boolean }) {
  // Eye (resume) and struck-through eye (hide): the pair readers know for show and hide.
  return (
    <svg viewBox="0 0 24 24" width={18} height={18} fill="none" stroke="currentColor" strokeWidth={1.5} strokeLinecap="round" strokeLinejoin="round" aria-hidden="true" className="ui-icon">
      <path d="M2.5 12c2.2-4 5.6-6 9.5-6s7.3 2 9.5 6c-2.2 4-5.6 6-9.5 6s-7.3-2-9.5-6z" />
      <path d="M12 9.2a2.8 2.8 0 1 0 0 5.6 2.8 2.8 0 0 0 0-5.6z" />
      {hidden && <path d="M4.5 4.5l15 15" />}
    </svg>
  );
}

type CoachProps<V> = {
  cardRef: Ref<HTMLDivElement>;
  at: { x: number; y: number } | null;
  quest: Quest<V>;
  head: string;
  state: TutorialState;
  pending: boolean;
  busy: boolean;
  blocked: MessageKey | null;
  terms: { text: GlossaryTerm[]; why: GlossaryTerm[] } | undefined;
  first: boolean;
  bounds: { w: number; h: number };
  onWhy: (open: boolean) => void;
  onPrev: () => void;
  onNext: () => void;
  onPause: () => void;
  onDrag: (at: { x: number; y: number }) => void;
};

function Coach<V>({ cardRef, at, quest, head, state, pending, busy, blocked, terms, first, bounds, onWhy, onPrev, onNext, onPause, onDrag }: CoachProps<V>) {
  const t = useT();
  const headId = useId();
  const titleId = useId();
  const textId = useId();
  const whyId = useId();
  const drag = useRef<{ dx: number; dy: number } | null>(null);
  const node = useRef<HTMLDivElement | null>(null);

  const onPointerDown = (event: ReactPointerEvent<HTMLDivElement>) => {
    if ((event.target as Element).closest("button")) return;
    const card = node.current;
    if (!card) return;
    drag.current = { dx: event.clientX - card.offsetLeft, dy: event.clientY - card.offsetTop };
    event.currentTarget.setPointerCapture?.(event.pointerId);
  };
  const onPointerMove = (event: ReactPointerEvent<HTMLDivElement>) => {
    const card = node.current;
    if (!drag.current || !card) return;
    const x = Math.min(Math.max(0, event.clientX - drag.current.dx), bounds.w - card.offsetWidth);
    const y = Math.min(Math.max(0, event.clientY - drag.current.dy), bounds.h - card.offsetHeight);
    onDrag({ x, y });
  };
  const onPointerUp = () => {
    drag.current = null;
  };

  return (
    <div
      ref={(el) => {
        node.current = el;
        if (typeof cardRef === "function") cardRef(el);
        else if (cardRef) cardRef.current = el;
      }}
      className={["tutorial-coach", state.shake > 0 && `tutorial-coach--shake-${state.shake % 2}`].filter(Boolean).join(" ")}
      data-tutorial-ui
      data-ui="tutorial.coach"
      role="dialog"
      aria-labelledby={`${headId} ${titleId}`}
      aria-describedby={textId}
      tabIndex={-1}
      style={at ? { left: at.x, top: at.y } : { visibility: "hidden" }}
    >
      <div className="tutorial-coach__head" title={t("tutorial.coach.drag")} onPointerDown={onPointerDown} onPointerMove={onPointerMove} onPointerUp={onPointerUp} onPointerCancel={onPointerUp}>
        <span className="tutorial-coach__step tabular" id={headId}>
          {head}
        </span>
        <span className="visually-hidden" id={titleId}>
          {t(quest.title)}
        </span>
        <kbd className="ui-kbd tutorial-coach__key" aria-hidden="true">
          Esc
        </kbd>
        <button type="button" className="tutorial-icon-button" aria-label={t("tutorial.coach.pause")} aria-keyshortcuts="Escape" title={t("tutorial.coach.pause")} data-ui="tutorial.coach.pause" onClick={onPause}>
          <EyeIcon hidden />
        </button>
      </div>
      <p className="tutorial-coach__text" id={textId} aria-live="polite">
        {busy && quest.busy ? (
          <>
            <Spinner size={16} className="tutorial-coach__spinner" />
            <span>{t(quest.busy.text)}</span>
          </>
        ) : (
          t(quest.text)
        )}
      </p>
      {terms && terms.text.length > 0 && <Terms terms={terms.text} />}
      {quest.why && (
        <div className="tutorial-coach__why">
          <button type="button" className="tutorial-why" data-ui="tutorial.coach.why" aria-expanded={state.whyOpen} aria-controls={whyId} onClick={() => onWhy(!state.whyOpen)}>
            <Icon name="chevronRight" size={16} className="tutorial-why__chevron" />
            <span>{t("tutorial.coach.why")}</span>
          </button>
          {state.whyOpen && (
            <div id={whyId} className="tutorial-coach__why-body">
              <p>{t(quest.why)}</p>
              {terms && terms.why.length > 0 && <Terms terms={terms.why} />}
            </div>
          )}
        </div>
      )}
      {pending && !busy && (
        <div className={["tutorial-coach__rule", state.armed && "tutorial-coach__rule--armed"].filter(Boolean).join(" ")} data-ui="tutorial.coach.rule">
          {quest.hint && <p className="tutorial-coach__hint">{t(quest.hint)}</p>}
          <p>{t(state.armed ? "tutorial.coach.rule_armed" : "tutorial.coach.rule")}</p>
        </div>
      )}
      {blocked && (
        <p className="tutorial-coach__blocked" data-ui="tutorial.coach.blocked" role="status">
          <Icon name="warning" size={16} />
          <span>{t("tutorial.coach.blocked", { reason: t(blocked) })}</span>
        </p>
      )}
      <div className="tutorial-coach__buttons">
        {!first ? (
          <Button variant="quiet" size="sm" icon="chevronLeft" data-ui="tutorial.coach.prev" onClick={onPrev}>
            {t("tutorial.coach.prev")}
          </Button>
        ) : (
          <span />
        )}
        <Button variant={pending ? "secondary" : "primary"} size="sm" iconEnd="chevronRight" data-ui="tutorial.coach.next" disabled={busy} onClick={onNext}>
          {t("tutorial.coach.next")}
        </Button>
      </div>
    </div>
  );
}

function Terms({ terms }: { terms: readonly GlossaryTerm[] }) {
  const t = useT();
  return (
    <dl className="tutorial-terms" aria-label={t("tutorial.coach.terms")}>
      {terms.map((term) => (
        <div key={term} className="tutorial-terms__item">
          <dt>{t(GLOSSARY[term].term)}</dt>
          <dd>{t(GLOSSARY[term].text)}</dd>
        </div>
      ))}
    </dl>
  );
}

function QuestPanel({
  panelRef,
  top,
  stepName,
  items,
  current,
  onFold,
  onRestart,
  onEnd,
}: {
    panelRef: Ref<HTMLElement>;
    top: number;
    stepName: string;
    items: { id: string; title: string; state: "done" | "current" | "pending" }[];
    current: number;
    onFold: () => void;
    onRestart: () => void;
    onEnd: () => void;
}) {
  const t = useT();
  const titleId = useId();
  const mark: Record<"done" | "current" | "pending", MessageKey> = { done: "tutorial.panel.item.done", current: "tutorial.panel.item.current", pending: "tutorial.panel.item.pending" };
  return (
    <section ref={panelRef} className="tutorial-panel" data-tutorial-ui data-ui="tutorial.panel" aria-labelledby={titleId} style={{ top }}>
      <header className="tutorial-panel__head">
        <h2 className="tutorial-panel__title" id={titleId}>
          {t("tutorial.panel.title", { step: stepName })}
        </h2>
        <IconButton icon="chevronUp" size="sm" label={t("tutorial.panel.fold")} data-ui="tutorial.panel.fold" onClick={onFold} />
      </header>
      <ProgressBar label={t("tutorial.panel.progress")} done={current} total={items.length} />
      <ol className="tutorial-panel__items">
        {items.map((item) => (
          <li key={item.id} className={`tutorial-panel__item tutorial-panel__item--${item.state}`} aria-current={item.state === "current" ? "step" : undefined}>
            <span className="tutorial-panel__mark" aria-hidden="true">
              {item.state === "done" ? <Icon name="check" size={14} weight="semibold" /> : item.state === "current" ? <Icon name="chevronRight" size={14} weight="semibold" /> : <Icon name="dot" size={14} />}
            </span>
            <span>{item.title}</span>
            <span className="visually-hidden">{`, ${t(mark[item.state])}`}</span>
          </li>
        ))}
      </ol>
      <footer className="tutorial-panel__foot">
        <Button variant="quiet" size="sm" icon="undo" data-ui="tutorial.panel.restart" onClick={onRestart}>
          {t("tutorial.panel.restart")}
        </Button>
        <Button variant="quiet" size="sm" data-ui="tutorial.panel.end" onClick={onEnd}>
          {t("tutorial.panel.end")}
        </Button>
      </footer>
    </section>
  );
}

function EndCard({ cardRef, at, onRestart, onFinish }: { cardRef: Ref<HTMLDivElement>; at: { left: number; top: number } | undefined; onRestart: () => void; onFinish: () => void }) {
  const t = useT();
  const titleId = useId();
  const bodyId = useId();
  return (
    <div ref={cardRef} className="tutorial-end" style={at} data-tutorial-ui data-ui="tutorial.end" role="dialog" aria-labelledby={titleId} aria-describedby={bodyId} tabIndex={-1}>
      <h2 className="tutorial-end__title" id={titleId}>
        {t("tutorial.end.title")}
      </h2>
      <p id={bodyId}>{t("tutorial.end.body")}</p>
      <div className="tutorial-end__buttons">
        <Button variant="secondary" icon="undo" data-ui="tutorial.end.restart" onClick={onRestart}>
          {t("tutorial.end.restart")}
        </Button>
        <Button variant="primary" data-ui="tutorial.end.finish" aria-keyshortcuts="Escape" onClick={onFinish}>
          {t("tutorial.end.finish")}
        </Button>
      </div>
    </div>
  );
}
