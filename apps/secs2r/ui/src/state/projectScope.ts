// What a project replacement empties (ADR 0056): the host fields and the
// resources one project owns, as one table. What is read again afterwards
// stays with the effect rules (`effectsOf`).
import type { StepId } from "../app/model";
import type { CamillaDevices } from "../ipc/bindings/CamillaDevices";
import type { ExportKind } from "../ipc/bindings/ExportKind";
import type { IpcError } from "../ipc/bindings/IpcError";
import type { MdatScope } from "../ipc/bindings/MdatScope";
import type { XoRecommendation } from "../ipc/bindings/XoRecommendation";
import type { CalibrateView } from "../screens/calibrate/types";
import type { ConnectPath, MicCalibrationView, RewProblem } from "../screens/connect/types";
import type { ApplyRun, ApplyTarget, ExportRun, ExportTab, HostFailure } from "../screens/export/types";
import type { MeasureView } from "../screens/measure/types";
import type { Session } from "./session";

/** Host fields one project owns: a replacement sets them to `PROJECT_HOST_DEFAULTS` and counts `opened` up. */
export type ProjectHostState = Pick<MeasureView, "selected" | "mdat" | "warnings"> & {
  path: ConnectPath | null;
  channel: string | null; xoGroup: string | null; previewHz: number | null;
  xoRecTurnedOffByEdit: boolean; xoRecommendation: XoRecommendation | null;
  runs: Partial<Record<ExportKind, ExportRun>>; mdatScope: MdatScope;
  /** 내보내기: the design name field's text (`""` = no name). */
  designName: string;
  /** 내보내기의 적용 (ADR 0018): how each target's last apply ended. */
  applyRuns: Partial<Record<ApplyTarget, ApplyRun>>;
  failure: CalibrateView["failure"]; rangeRejection: CalibrateView["rangeRejection"];
  micCalBusy: MicCalibrationView["busy"]; micCalError: MicCalibrationView["error"];
  /** 연결·점검: the refusal of the REW executable path sent last (the project stores the path). */
  rewExecutableError: IpcError | null;
  /** Projects opened or created in this window; screens reset what they hid for the previous one when it changes. */
  opened: number;
};

/** Host fields the window owns: a replacement keeps them. */
export type AppHostState = Pick<MeasureView, "graphHidden" | "plotRange"> & {
  computeLoading: boolean; computeSaving: boolean; computeError: IpcError | null;
  current: StepId;
  tab: CalibrateView["tab"];
  targetEdit: CalibrateView["targetEdit"]; xoEdit: CalibrateView["xoEdit"];
  /** 내보내기: the CamillaDSP devices (file placeholders by default) and the convolution IR's extra rates in Hz, kept for the session across projects. */
  camillaDevices: CamillaDevices; extraRates: number[];
  /** 내보내기: the destination tab the user picked, kept for the session across projects; null until one is picked. */
  exportTab: ExportTab | null;
  /** 내보내기의 적용: the CamillaDSP address field's unsaved text, an apply_status read and a revert_eqapo in flight. */
  applyAddressDraft: string | null; applyChecking: boolean; applyReverting: boolean;
  /** 관리: the CamillaDSP executable field's unsaved text, a host_install or host_stop_managing in flight, and the last of them that failed. */
  applyExeDraft: string | null; hostInstalling: boolean; hostStopping: boolean; hostFailure: HostFailure | null;
  rewLaunch: "idle" | "checking" | "launching";
  rewProblem: RewProblem | null; rewProblemDetail: string | null; readingRew: boolean;
  switchingRewDriver: boolean;
};

export type HostState = ProjectHostState & AppHostState;

/** A field in both tables is a type error: each host field has one owner. */
type Shared = keyof ProjectHostState & keyof AppHostState;
const owners: [Shared] extends [never] ? true : never = true;
void owners;

/** The project fields after a replacement; `opened` counts up instead. */
export const PROJECT_HOST_DEFAULTS = {
  path: null, selected: null, mdat: null, warnings: [],
  channel: null, xoGroup: null, previewHz: null,
  xoRecTurnedOffByEdit: false, xoRecommendation: null,
  runs: {}, mdatScope: "all", designName: "", applyRuns: {},
  failure: null, rangeRejection: null,
  micCalBusy: null, micCalError: null, rewExecutableError: null,
} satisfies Omit<ProjectHostState, "opened">;

type Stores = Session["stores"];
export type ProjectResource = (typeof PROJECT_RESOURCES)[number];
export type AppResource = Exclude<keyof Stores, ProjectResource>;

/** The resources one project owns: a replacement empties them. */
export const PROJECT_RESOURCES = [
  "project", "config", "notices", "dsp", "layout", "estimate", "measurementSettings", "records", "curves", "levels",
  "settings", "micCalibration", "modeSpec", "target", "results", "resultCurves", "lines", "subWarnings", "folder", "applyStatus",
] as const satisfies readonly (keyof Stores)[];

/** The resources the window owns (bootstrap, app settings, compute status, methods, REW, the level monitor): the complement of `PROJECT_RESOURCES`. */
export function appResources(stores: Stores): AppResource[] {
  return (Object.keys(stores) as (keyof Stores)[]).filter((key): key is AppResource => !(PROJECT_RESOURCES as readonly string[]).includes(key));
}

/** What `replaceProject` needs: the stores, the host, and the session's own counters (job epoch, curve generation, auto refresh, tutorial readiness). */
export type ProjectScope = Pick<Session, "host" | "stores"> & { resetProjectRuntime: () => void };

/**
 * Replaces the project in the window: the session's counters, every project
 * resource emptied, every project host field back to its default and
 * `opened` one higher. An external replacement also invalidates reads in
 * flight, whose answers may describe the previous project.
 */
export function replaceProject(scope: ProjectScope, source: "request" | "external") {
  scope.resetProjectRuntime();
  for (const key of PROJECT_RESOURCES) scope.stores[key].set(null);
  if (source === "external") for (const store of Object.values(scope.stores)) if ("invalidate" in store) store.invalidate();
  scope.host.set({ ...PROJECT_HOST_DEFAULTS, warnings: [], runs: {}, applyRuns: {}, opened: scope.host.getSnapshot().opened + 1 });
}
