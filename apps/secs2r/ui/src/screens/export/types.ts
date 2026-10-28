// 내보내기: what the screen draws and what it reports. Upstream
// src/secs/wizard/step8_mdat.py (the export step) and the hidden export card
// of step_calibration.py (staged mdat, U3).
import type { StepState } from "../../app/model";
import type { CalibrationSettings } from "../../ipc/bindings/CalibrationSettings";
import type { CamillaCaptureDevice } from "../../ipc/bindings/CamillaCaptureDevice";
import type { CamillaDevices } from "../../ipc/bindings/CamillaDevices";
import type { CamillaPlaybackDevice } from "../../ipc/bindings/CamillaPlaybackDevice";
import type { CamillaWasapiFormat } from "../../ipc/bindings/CamillaWasapiFormat";
import type { CalibrationSummary } from "../../ipc/bindings/CalibrationSummary";
import type { ExportBlock } from "../../ipc/bindings/ExportBlock";
import type { ExportFolderResult } from "../../ipc/bindings/ExportFolderResult";
import type { ExportKind } from "../../ipc/bindings/ExportKind";
import type { IpcError } from "../../ipc/bindings/IpcError";
import type { MdatScope } from "../../ipc/bindings/MdatScope";
import type { MeasureDevice } from "../../ipc/bindings/MeasureDevice";
import type { Mode } from "../../ipc/bindings/Mode";
import type { Notice } from "../../ipc/bindings/Notice";
import type { ResultsSummary } from "../../ipc/bindings/ResultsSummary";
import { CALIBRATION_LIMITS } from "../../ipc/bindings/rules";

/**
 * The order the export kinds appear in: the filters for the DSP or player
 * (the primary's CamillaDSP first, then the filter text, Equalizer APO and
 * the convolution IR), then the responses for REW (the staged mdat under 고급).
 */
export const EXPORT_KINDS = ["camilla_dsp", "filter_text", "equalizer_apo", "convolution", "before_after_mdat", "staged_mdat"] as const satisfies readonly ExportKind[];

/**
 * The filter exports, in screen order: the kinds that take a design name
 * and write `secs-design.json` (DEVIATIONS F21). The REW mdat exports take
 * neither.
 */
export const FILTER_KINDS = ["camilla_dsp", "filter_text", "equalizer_apo", "convolution"] as const satisfies readonly ExportKind[];

export function isFilterKind(kind: ExportKind): kind is (typeof FILTER_KINDS)[number] {
  return (FILTER_KINDS as readonly ExportKind[]).includes(kind);
}

/** CamillaDSP's WASAPI sample formats, in the order the choice shows them. F32 is the shared-mode format and the default. */
export const WASAPI_FORMATS = ["S16", "S24", "S32", "F32"] as const satisfies readonly CamillaWasapiFormat[];

/** File placeholders on both sides: today's CamillaDSP export (DEVIATIONS F20). */
export const FILE_DEVICES: CamillaDevices = { capture: { type: "file" }, playback: { type: "file" } };

/** One side of the CamillaDSP devices. */
export type DeviceSide = keyof CamillaDevices;
export type CamillaDevice = CamillaCaptureDevice | CamillaPlaybackDevice;

/** What the REW mdat can carry, in the order the choice shows them (`MdatScope`, DEVIATIONS F19). */
export const MDAT_SCOPES = ["all", "measurements", "results"] as const satisfies readonly MdatScope[];

/** Upstream `MIMO_MIN_POSITIONS`: MIMO needs three measured positions. */
export const MIMO_MIN_POSITIONS = 3;

/** How the last export of one kind ended, for the current result. */
export type ExportRun =
  /** `ExportResult.files`, and the folder they went to (the common parent of the files). */
  | { status: "done"; files: string[]; folder: string }
  /** The job's `IpcError` (export_failed, stale_inputs, cancelled, …). */
  | { status: "failed"; error: IpcError }
  /** In the tutorial nothing is written (upstream `trial`): P33 records this instead of calling `export`. */
  | { status: "tutorial" };

export type ExportView = {
  /** `results`: the latest correction and MIMO summaries. */
  results: ResultsSummary;
  /**
   * Current `calibration_settings_get` mode, ultra limit and MIMO switch.
   * The computed mode and ultra limit are in `results.calibration.mode`
   * and `results.calibration.ultra_ms`.
   */
  settings: Pick<CalibrationSettings, "mode" | "ultra_ms" | "mimo_on">;
  /** `project_info().positions.length`: MIMO needs three positions or more; none means nothing to put in the 원 측정 mdat. */
  positions: number;
  /** The REW mdat's chosen contents (session host, `all` until changed; a new project starts at `all`). */
  mdatScope: MdatScope;
  /** `result_lines().rc`: how the RC upper limit was set (`rc_limit`, `rc_room_volume`, `rc_no_room_volume`, `rc_auto_cap`). */
  rc: Notice[];
  /** `export_folder_get`; null until it arrives. */
  folder: ExportFolderResult | null;
  /** A `calibrate` job runs (job events): the result is about to change. */
  computing: boolean;
  /** A calibration job is running or awaiting cancellation; the recovery action stays busy until it finishes. */
  recalibrating: boolean;
  /** The kind of the running `export` job, or null. Export jobs run one at a time. */
  exporting: ExportKind | null;
  /** The last export of each kind for the current result. P33 keeps it for the session and clears it when a new result arrives. */
  runs: Partial<Record<ExportKind, ExportRun>>;
  /** Tutorial mode (the demo project): exports write nothing. */
  tutorial: boolean;
  /**
   * The design name field's text (session host), `""` when unset. A valid
   * name sends the filter exports to `SECS_<kind>_<name>` and replaces that
   * folder on the next export with the same name (DEVIATIONS F21).
   */
  designName: string;
  /** The CamillaDSP export's capture and playback (session host), file placeholders until changed (DEVIATIONS F20). */
  camillaDevices: CamillaDevices;
  /**
   * The WASAPI endpoints to choose from (`measure_methods().native_devices`),
   * or null when there is no list (not Windows, or enumeration failed): the
   * user then types a device name.
   */
  nativeDevices: MeasureDevice[] | null;
};

export type ExportActions = {
  /**
   * `export { kind, folder: "" }` (job; the service picks the subfolder). The
   * REW mdat adds `mdat_scope: mdatScopeFor(results.calibration, mdatScope)`.
   * In the tutorial P33 records `{ status: "tutorial" }` instead. Upstream
   * opened the written folder after a successful export: P33 follows with
   * `open_folder { path: run.folder }`.
   */
  exportFiles: (kind: ExportKind) => void;
  /** Host: what the REW mdat carries. */
  setMdatScope: (scope: MdatScope) => void;
  /** Host: the design name field's text, as typed (the service trims it). */
  setDesignName: (name: string) => void;
  /** Host: the CamillaDSP export's capture and playback. */
  setCamillaDevices: (devices: CamillaDevices) => void;
  /** `dialog_pick_folder { start: folder }`, then `export_folder_set { folder }` when one was picked. */
  changeFolder: () => void;
  /** `open_folder { path }`: the export folder, or the folder a run wrote. */
  openFolder: (path: string) => void;
  /** Host: the frame goes to 보정. */
  goToCalibrate: () => void;
  /** `calibrate { settings: null, subs_only: false }`: recompute with current settings without leaving export. */
  recalibrate: () => void;
};

/** Whether MIMO goes into the export, and if not, why (upstream `show_mimo`: on, computed, current, three positions or more). */
export type MimoInclusion =
  | { included: true; kind: string; supports: number; channels: number }
  | { included: false; reason: "off" | "positions" | "none" | "stale" };

export function mimoInclusion(view: ExportView): MimoInclusion {
  switch (view.results.mimo_export) {
    case "off": return { included: false, reason: "off" };
    case "too_few_positions": return { included: false, reason: "positions" };
    case "none": return { included: false, reason: "none" };
    case "stale": return { included: false, reason: "stale" };
    // The existing screen shows current MIMO even when it has no support paths.
    case "no_supports":
    case "included": {
      const mimo = view.results.mimo!;
      return { included: true, kind: mimo.kind, supports: mimo.support_count, channels: mimo.input_count };
    }
  }
}

/**
 * Why `kind` cannot carry the current result, or null when it can (and when
 * there is no result). Follows the service's `filter_export_block`
 * (crates/secs-types/src/ipc.rs, upstream `calibration.filter_export_block`):
 * the filter text carries RC only, so subs block it first and MIMO next;
 * CamillaDSP, Equalizer APO, the convolution IR (subs, LFE and MIMO in its
 * paths) and the mdat exports carry everything (upstream stops MIMO at
 * Equalizer APO too; DEVIATIONS F12). MIMO counts
 * only when it goes into the export (`mimoInclusion`).
 */
export function exportBlock(view: ExportView, kind: ExportKind): ExportBlock | null {
  const result = view.results.calibration;
  if (!result) return null;
  // The original measurements need no design, so an old cache does not stop them.
  if (kind === "before_after_mdat" && mdatScopeFor(result, view.mdatScope) === "measurements") return null;
  if ((kind === "staged_mdat" || kind === "before_after_mdat") && result.cached && result.stale) return "stale_cache";
  const subs = result.sub_roles.length > 0;
  const mimo = mimoInclusion(view).included;
  if (kind === "filter_text" && subs) return "subs";
  if (kind === "filter_text" && mimo) return "mimo";
  return null;
}

/**
 * The mode in the filter files' names (upstream `_filter_plan`): the mode,
 * and for 초고지연 its limit in ms as Python's `:g` writes it (`ultra200`,
 * `ultra62.5`; six significant digits cover the 30–200 ms the limit takes).
 * Folder names keep the bare mode (`SECS_<kind>_<what>_ultra_…`).
 */
export function filterModeName(settings: Pick<CalibrationSettings, "mode" | "ultra_ms">): string {
  return settings.mode === "ultra" ? `ultra${Number(settings.ultra_ms.toPrecision(6))}` : settings.mode;
}

/**
 * The contents the REW mdat is written with: the choice, or the original
 * measurements alone while there is no result to carry (the service needs
 * none for them).
 */
export function mdatScopeFor(result: CalibrationSummary | null | undefined, chosen: MdatScope): MdatScope {
  return result ? chosen : "measurements";
}

const MODE_LABEL: Record<Mode, string> = { zero: "최소 지연 모드", mixed: "저지연 모드", linear: "표준 모드", ultra: "초고지연 모드" };

/**
 * The REW mdat's file name (`secs_export::rew_mdat_filename`, DEVIATIONS
 * F19): `원 측정` when it holds measurements, `보정 결과` when it holds
 * computed responses, then the mode as `CalibrationSettings::label` writes
 * it. The service writes Korean names in every language.
 */
export function mdatFileName(scope: MdatScope, settings: Pick<CalibrationSettings, "mode" | "ultra_ms">): string {
  if (scope === "measurements") return "SECS 원 측정.mdat";
  const [lo, hi] = CALIBRATION_LIMITS.ultra_ms;
  const label = settings.mode === "ultra" ? `${MODE_LABEL.ultra} ${Number(Math.min(hi, Math.max(lo, settings.ultra_ms)).toPrecision(6))} ms` : MODE_LABEL[settings.mode];
  return `SECS ${scope === "results" ? "보정 결과" : "원 측정+보정 결과"} ${label}.mdat`;
}

/**
 * Whether `kind`'s button can start an export now (upstream `ready()`: a
 * result and no correction running). A stale result exports as it is, as
 * upstream does; the screen warns about it. The REW mdat of the original
 * measurements needs measurements only. Export jobs run one at a time.
 */
export function canExport(view: ExportView, kind?: ExportKind): boolean {
  if (view.exporting !== null) return false;
  if (kind === "before_after_mdat" && mdatScopeFor(view.results.calibration, view.mdatScope) === "measurements") return view.positions > 0;
  return !!view.results.calibration && !view.computing;
}

/**
 * Why a design name cannot name a folder, one value per rule of the
 * service's `validate_design_name` (crates/secs-service/src/design_export.rs),
 * checked in its order on the trimmed name: 1 to 64 characters, no control
 * characters, none of `\ / : * ? " < > |`, no `.` or space at the end, and
 * not a Windows device name (`CON`, `PRN`, `AUX`, `NUL`, `COM1`–`COM9`,
 * `LPT1`–`LPT9`) before the first dot, in any case.
 */
export type DesignNameProblem = "length" | "control" | "character" | "trailing" | "reserved";

/** Rust's `str::trim`: Unicode White_Space at both ends (JS `trim` also strips U+FEFF and keeps U+0085). */
const EDGE_SPACE = /^\p{White_Space}+|\p{White_Space}+$/gu;
const CONTROL = /\p{Cc}/u;
const FORBIDDEN = /[\\/:*?"<>|]/;
const RESERVED = /^(?:CON|PRN|AUX|NUL|COM[1-9]|LPT[1-9])$/;

/**
 * What is wrong with the design name field's text, or null when it can
 * name a folder. An empty field is no problem: it means no name, and the
 * filter exports go to timestamped folders as before.
 */
export function designNameProblem(name: string): DesignNameProblem | null {
  if (name === "") return null;
  const value = name.replace(EDGE_SPACE, "");
  const count = [...value].length;
  if (count < 1 || count > 64) return "length";
  if (CONTROL.test(value)) return "control";
  if (FORBIDDEN.test(value)) return "character";
  if (value.endsWith(".") || value.endsWith(" ")) return "trailing";
  // `to_ascii_uppercase`: only a–z change case.
  const stem = (value.split(".")[0] ?? value).replace(/[a-z]/g, (ch) => ch.toUpperCase());
  if (RESERVED.test(stem)) return "reserved";
  return null;
}

/**
 * The design name the service will use (trimmed), or null when there is
 * none, it cannot name a folder, or the tutorial writes nothing (it has no
 * export folder and so no name field).
 */
export function designNameFor(view: Pick<ExportView, "designName" | "tutorial">): string | null {
  if (view.tutorial || view.designName === "" || designNameProblem(view.designName)) return null;
  return view.designName.replace(EDGE_SPACE, "");
}

/** Why one side of the CamillaDSP devices cannot be written: an ASIO driver needs a name (the service's `camilla_asio_device`). */
export type DeviceProblem = "asio_device";

export function deviceProblem(device: CamillaDevice): DeviceProblem | null {
  return device.type === "asio" && device.device.trim() === "" ? "asio_device" : null;
}

/**
 * What the user has to fix on this screen before `kind` can export: the
 * design name for the filter exports, then the CamillaDSP devices for
 * CamillaDSP. Null when nothing blocks it. The field shows the message;
 * the buttons it blocks point at it.
 */
export type InputProblem =
  | { field: "design_name"; problem: DesignNameProblem }
  | { field: DeviceSide; problem: DeviceProblem };

export function inputProblem(view: ExportView, kind: ExportKind): InputProblem | null {
  if (!isFilterKind(kind)) return null;
  // The tutorial has no name field and writes nothing.
  const name = view.tutorial ? null : designNameProblem(view.designName);
  if (name) return { field: "design_name", problem: name };
  if (kind !== "camilla_dsp") return null;
  for (const side of ["capture", "playback"] as const) {
    const problem = deviceProblem(view.camillaDevices[side]);
    if (problem) return { field: side, problem };
  }
  return null;
}

/**
 * The devices a side can choose from: inputs for capture (or every output
 * when it records an output by loopback), outputs for playback. One entry
 * per name, since CamillaDSP opens a WASAPI device by its name.
 */
export function devicesFor(devices: readonly MeasureDevice[], side: DeviceSide, loopback = false): MeasureDevice[] {
  const outputs = side === "playback" || loopback;
  const seen = new Set<string>();
  return devices.filter((device) => {
    if ((outputs ? device.output_channels : device.input_channels) <= 0 || seen.has(device.name)) return false;
    seen.add(device.name);
    return true;
  });
}

/** The rail state of 내보내기: blocked while there is no result or one is being computed, done after an export of the result. */
export function exportStepState(view: ExportView): StepState {
  const result = view.results.calibration;
  if (view.computing) return { status: "blocked", reason: "computing" };
  if (!result) return { status: "blocked", reason: "no_result" };
  if (Object.values(view.runs).some((run) => run?.status === "done")) return { status: "done" };
  return { status: "ready" };
}
