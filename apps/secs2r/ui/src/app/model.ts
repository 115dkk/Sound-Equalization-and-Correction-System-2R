// What the frame shows, as plain data. The screens (P30–P32) and the IPC
// wiring (P33) compute it from the service; the frame only draws it.
import type { MessageKey } from "../i18n";

export const STEPS = ["system", "connect", "measure", "calibrate", "export"] as const;
export type StepId = (typeof STEPS)[number];

/** done: finished; ready: can be worked on; blocked: needs an earlier step; skipped: not on this path (mdat import skips 연결·점검). */
export type StepStatus = "done" | "ready" | "blocked" | "skipped";

export const BLOCKED_REASONS = ["no_speakers", "config_invalid", "no_path", "no_measurements", "measuring", "no_result", "computing", "device_busy"] as const;
export type BlockedReason = (typeof BLOCKED_REASONS)[number];

export type StepState = { status: StepStatus; reason?: BlockedReason; count?: number };

export type CalibrationState = "none" | "computing" | "done" | "stale" | "failed";
export type PathKind = "rew" | "native" | "mock" | "none";
export type PathState = "ok" | "warning" | "error";

export type ProjectSummary = {
  preset: string | null;
  subCount: number;
  outputs: number;
  positionsDone: number;
  positionsTotal: number;
  calibration: CalibrationState;
  path: { kind: PathKind; state: PathState; device?: string };
  fileName: string | null;
  dirty: boolean;
};

export type FrameModel = {
  current: StepId;
  steps: Record<StepId, StepState>;
  project: ProjectSummary;
};

export const STEP_TITLE: Record<StepId, MessageKey> = {
  system: "frame.step.system",
  connect: "frame.step.connect",
  measure: "frame.step.measure",
  calibrate: "frame.step.calibrate",
  export: "frame.step.export",
};

export const STEP_PURPOSE: Record<StepId, MessageKey> = {
  system: "frame.step.system.purpose",
  connect: "frame.step.connect.purpose",
  measure: "frame.step.measure.purpose",
  calibrate: "frame.step.calibrate.purpose",
  export: "frame.step.export.purpose",
};

export const STEP_PRIMARY: Record<StepId, MessageKey> = {
  system: "frame.step.system.primary",
  connect: "frame.step.connect.primary",
  measure: "frame.step.measure.primary",
  calibrate: "frame.step.calibrate.primary",
  export: "frame.step.export.primary",
};

export const BLOCKED_TEXT: Record<BlockedReason, { full: MessageKey; short: MessageKey }> = {
  no_speakers: { full: "frame.blocked.no_speakers", short: "frame.blocked.no_speakers.short" },
  config_invalid: { full: "frame.blocked.config_invalid", short: "frame.blocked.config_invalid.short" },
  no_path: { full: "frame.blocked.no_path", short: "frame.blocked.no_path.short" },
  no_measurements: { full: "frame.blocked.no_measurements", short: "frame.blocked.no_measurements.short" },
  measuring: { full: "frame.blocked.measuring", short: "frame.blocked.measuring.short" },
  no_result: { full: "frame.blocked.no_result", short: "frame.blocked.no_result.short" },
  computing: { full: "frame.blocked.computing", short: "frame.blocked.computing.short" },
  device_busy: { full: "frame.blocked.device_busy", short: "frame.blocked.device_busy.short" },
};

/** The step reached by moving `delta` from `from`, skipping steps that are not on this path. */
export function neighbourStep(steps: FrameModel["steps"], from: StepId, delta: -1 | 1): StepId | null {
  let index = STEPS.indexOf(from) + delta;
  while (index >= 0 && index < STEPS.length) {
    const id = STEPS[index];
    if (id && steps[id].status !== "skipped") return id;
    index += delta;
  }
  return null;
}
