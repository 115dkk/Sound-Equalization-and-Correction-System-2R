// The tutorial engine's vocabulary (upstream tutorial_quests.Quest and
// tutorial_overlay.Tutorial). Quests point at screen elements by their
// data-ui ids; P35 writes the real quest list on the screens' ids.
import type { StepId } from "../../app/model";
import type { MessageKey } from "../../i18n";

export type QuestKind = "look" | "act";

/** The 25 glossary terms (upstream tutorial_quests.py GLOSSARY), catalog keys tutorial.glossary.<term>. */
export const GLOSSARY_TERMS = [
  "rew",
  "api",
  "calibration",
  "sweep",
  "polarity",
  "dsp",
  "fir",
  "iir",
  "dbfs",
  "timing",
  "mimo",
  "mdat",
  "rc",
  "spl",
  "target",
  "gd",
  "xo",
  "peq",
  "boost",
  "preset",
  "lp_hp",
  "gain",
  "delay",
  "allpass",
  "dba",
] as const;
export type GlossaryTerm = (typeof GLOSSARY_TERMS)[number];

/**
 * One quest. `look` quests point at a place and move on with 계속; `act`
 * quests wait for the user to do something (`done`), and the second 계속
 * runs `auto` instead. `V` is the app view P33 hands the engine.
 */
export type Quest<V> = {
  id: string;
  step: StepId;
  kind: QuestKind;
  /** data-ui ids. A nested list is one group (one outline); `apart` outlines each group on its own, otherwise one outline goes round all. */
  targets: readonly (string | readonly string[])[];
  apart?: boolean;
  /** Short name in the quest list. */
  title: MessageKey;
  /** What this place is, or what to do. */
  text: MessageKey;
  /** Answer to "why?" (why this matters, not how). */
  why?: MessageKey;
  /** Act: the concrete action, shown above the rule line. */
  hint?: MessageKey;
  /** Glossary under the text; a term shows on the first quest that names it. */
  terms?: readonly GlossaryTerm[];
  /** Glossary under 왜?. */
  whyTerms?: readonly GlossaryTerm[];
  /** Keep the quest only when this holds at start (upstream `when`: BA quests need two subs). */
  when?: (view: V) => boolean;
  /** Act: a value taken when the quest shows, handed to `done` (so a change, not a state, finishes it). */
  base?: (view: V) => unknown;
  /** Act: finished. */
  done?: (view: V, base: unknown) => boolean;
  /** Act: the second 계속 does it instead, and the toast says what was done. `false` means it could not (the card shows `blocked`). */
  auto?: { run: (view: V) => void | false; note: MessageKey };
  /** Act: why it cannot be done now (null when it can); the card shows the reason. */
  blocked?: (view: V) => MessageKey | null;
  /** Act: the step's 다음 quest (upstream `_next_to`). Resuming into a step that cannot be left lands here when no other act quest is left to do. */
  gate?: boolean;
  /** Act: a long action runs; the card shows `text` and waits. */
  busy?: { when: (view: V) => boolean; text: MessageKey };
  /** Toast when the user did it (default 확인됨). */
  ok?: MessageKey;
  /** ids the coach card must not cover (meters, status lines). */
  keep?: readonly string[];
  /** Moves the screen while the card shows (upstream tab cycling): `step(k)` every `every` ms, `end()` on leaving. */
  animate?: { every: number; step: (k: number) => void; end?: () => void };
};

export type TutorialActions = {
  /** Frame navigation: the quest's step must be on screen. */
  goTo: (step: StepId) => void;
  /** Host: leave the tutorial and reopen the project from before it (upstream end_trial_mode). */
  finish: () => void;
  /** Host: back to the first quest (upstream restart_tutorial resets the demo too). */
  restart: () => void;
};

/** A message with its parameters (toasts). */
export type Said = { key: MessageKey; params?: Record<string, string | number>; note?: MessageKey };

export type TutorialState = {
  /** Current quest (into the list kept at start). */
  index: number;
  /** Quests already done; going back to one does not ask for it again. */
  completed: readonly number[];
  /** Steps whose last quest is done. */
  cleared: readonly StepId[];
  /** Act: the first 계속 was pressed; the second runs `auto`. */
  armed: boolean;
  whyOpen: boolean;
  /** Everything hidden; a resume button stays. */
  paused: boolean;
  /** The quest list folded into a pill. */
  folded: boolean;
  /** The user unfolded the list; it then folds only to uncover the target. */
  userOpened: boolean;
  /** All quests done: the end card. */
  finished: boolean;
  /** Last move (skips go the same way). */
  dir: 1 | -1;
  toast: Said | null;
  /** Reason an `auto` could not run. */
  autoBlocked: boolean;
  /** Bumped to replay the shake. */
  shake: number;
};
