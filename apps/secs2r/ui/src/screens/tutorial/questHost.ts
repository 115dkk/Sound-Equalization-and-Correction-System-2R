import type { StepId } from "../../app/model";
import type { SystemActions, SystemView } from "../system/types";
import type { ConnectActions, ConnectView } from "../connect/types";
import type { MeasureActions, MeasureView } from "../measure/types";
import type { CalibrateActions, CalibrateView } from "../calibrate/types";
import type { ExportView } from "../export/types";

/** Only screen data used by the quests; no service calls or DOM clicks. */
export type TutorialView = {
  step: StepId;
  system: Pick<SystemView, "config" | "notices">;
  connect: Pick<ConnectView, "path" | "settings" | "methods" | "levels" | "level">;
  measure: Pick<MeasureView, "project" | "job">;
  calibrate: Pick<CalibrateView, "tab" | "positions" | "settings" | "results" | "lines" | "running" | "measuring">;
  export: Pick<ExportView, "computing" | "results">;
};

export type TutorialHost = {
  system: Pick<SystemActions, "setOutput" | "renumberOutputs" | "next">;
  connect: Pick<ConnectActions, "choosePath" | "skipLevels" | "next">;
  measure: Pick<MeasureActions, "loadSample" | "openCalibrate" | "setGraphHidden">;
  calibrate: Pick<CalibrateActions, "calibrate" | "calibrateSubs" | "selectTab" | "changeSetting">;
  goTo: (step: StepId) => void;
};

/** Upstream plant_wrong_out: never overwrite a non-demo or occupied output. */
export function wrongOutput(config: SystemView["config"]): number | null {
  const index = config.speakers.findIndex((speaker) => speaker.role === "SW3" && speaker.bands[0]?.out === 6);
  return index >= 0 && !config.speakers.some((speaker) => speaker.bands.some((band) => band.out === 16)) ? index : null;
}
