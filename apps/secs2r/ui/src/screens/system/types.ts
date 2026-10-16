// 시스템: what is in the room and how it will be measured (upstream steps 2
// and 4). The screen draws `SystemView` and reports what the user did
// through `SystemActions`; P33 wires both to the service.
import type { ConfigResult } from "../../ipc/bindings/ConfigResult";
import type { DspFields } from "../../ipc/bindings/DspFields";
import type { DspResult } from "../../ipc/bindings/DspResult";
import type { LayoutResult } from "../../ipc/bindings/LayoutResult";
import type { LayoutTiming } from "../../ipc/bindings/LayoutTiming";
import type { Notice } from "../../ipc/bindings/Notice";
import type { PositionEstimateResult } from "../../ipc/bindings/PositionEstimateResult";

export type SystemView = {
  /** config_get, and the config every config_* call answers with. */
  config: ConfigResult;
  /** config_validate, after every config change. */
  notices: readonly Notice[];
  /** dsp_get / dsp_set. */
  dsp: DspResult;
  /** layout_get / layout_set (arrangement, start delay, timing reference). */
  layout: LayoutResult;
  /** position_estimate after every config, layout or sweep-length change; null while unknown. */
  estimate: PositionEstimateResult | null;
  /**
   * Output channels of the measurement output device (`config.device`, chosen
   * on 연결·점검): `MeasureDevice.output_channels` of the native device, or
   * REW's output count; null while unknown.
   */
  outputChannels: number | null;
  /** Resolved measurement output name; falls back to the selected device ID. */
  outputDeviceName?: string;
  /** That device's output names, index 0 = output 1 (REW reports names); empty when only numbers are known. */
  outputNames: readonly string[];
  /** Positions with measurements (`ProjectInfo.positions`); non-empty means preset, sub-count and arrangement changes clear them (U5). */
  positionsDone: readonly number[];
};

export type SystemActions = {
  /**
   * config_apply_preset { preset, confirm: true } (`custom` = an empty
   * configuration). The screen asks first when `positionsDone` is not empty.
   * Resolves the undo token when measurements were cleared, else null.
   */
  applyPreset: (preset: string) => Promise<string | null>;
  /** config_set_sub_count { count, confirm: true }; asked first like applyPreset. Resolves the undo token or null. */
  setSubCount: (count: number) => Promise<string | null>;
  /** config_set_speaker { index, role } (a sub role makes the speaker a single SUB band). */
  setRole: (index: number, role: string) => void;
  /** config_set_speaker { index, size } with `small` (crossover applied) or `large` (full range). */
  setSize: (index: number, size: "small" | "large") => void;
  /** config_set_speaker { index, band_index, out }. */
  setOutput: (index: number, band: number, out: number) => void;
  /** config_set_speaker { index, band_index, pol } with `+` normal or `-` inverted. */
  setPolarity: (index: number, band: number, pol: "+" | "-") => void;
  /** config_add_speaker (first unused role on the next free output). */
  addSpeaker: () => void;
  /** config_remove_speaker { index }. */
  removeSpeaker: (index: number) => void;
  /** config_renumber. */
  renumberOutputs: () => void;
  /** dialog_save_file { kind: "patch_sheet", suggested: "secs_patch_sheet.csv" }, then patch_sheet_csv { path } unless cancelled. */
  savePatchSheet: () => void;
  /** dsp_set { preset }. */
  setDspPreset: (key: string) => void;
  /** dsp_set { fields } (a manual edit makes the profile `custom`). */
  setDspFields: (fields: DspFields) => void;
  /** layout_set { count, confirm: true }; asked first like applyPreset. Resolves the undo token or null. */
  setArrangement: (count: number) => Promise<string | null>;
  /** layout_set { start_delay } with the stored value (`끄기`, `5 초`, `10 초`, `15 초`). */
  setStartDelay: (delay: string) => void;
  /** layout_set { timing }. */
  setTiming: (timing: LayoutTiming) => void;
  /** undo { token } (the toast after a change that cleared measurements). */
  undo: (token: string) => Promise<void>;
  /** Host: opens 연결·점검 (the step header's primary action). */
  next: () => void;
};
