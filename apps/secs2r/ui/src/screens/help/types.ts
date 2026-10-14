// 도움말: a panel beside the screen (upstream help_dialog.py, a non-modal
// 640×720 window) with the step's help text (content.ts).
import type { StepId } from "../../app/model";
import type { MessageKey } from "../../i18n";

export type HelpTabId = "intro" | "steps" | "checks" | "trouble" | "real" | "mdat" | "camilla";

export type HelpBlock =
  /** The step's goal, one paragraph above the walkthrough. */
  | { kind: "lead"; text: MessageKey }
  | { kind: "paragraphs"; items: readonly MessageKey[] }
  | { kind: "heading"; text: MessageKey }
  /** The walkthrough: what to do, then what you see when it worked. */
  | { kind: "steps"; items: readonly { do: MessageKey; see: MessageKey }[] }
  | { kind: "bullets"; items: readonly MessageKey[] }
  | { kind: "numbered"; items: readonly MessageKey[] }
  /** Symptom and fix. */
  | { kind: "pairs"; items: readonly { term: MessageKey; text: MessageKey }[] }
  /** Commands and addresses, shown in the code face. */
  | { kind: "code"; items: readonly MessageKey[] }
  /** The mdat channel-name table (MDAT_CHANNEL_ROWS). */
  | { kind: "channels" }
  /** The 튜토리얼 시작 button. */
  | { kind: "tutorial" };

export type HelpTab = { id: HelpTabId; blocks: readonly HelpBlock[] };

export type ChannelRow = { group: MessageKey; use: string; also: string };

export type HelpView = {
  /** The step the frame shows; the panel follows it. */
  step: StepId;
  /** Tutorial mode: the intro's start button reads as in progress. */
  tutorial: boolean;
};

export type HelpActions = {
  /** Host: closes the panel (F1 and Esc toggle it; focus goes back to the screen). */
  close: () => void;
  /** Host: opens the 튜토리얼 시작 dialog (project/ProjectDialogs, kind "tutorial"). */
  startTutorial: () => void;
};
