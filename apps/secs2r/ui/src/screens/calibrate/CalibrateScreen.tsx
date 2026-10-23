// The 보정 step (upstream wizard/step_calibration.py). Results first: the
// verdict with the run beside it, the per-channel scorecard and one graph;
// then the settings that change them, in five sections grouped by what they
// change. The screen draws the view and reports what the user did; P33
// wires the actions.
import { useState, type ReactNode } from "react";
import type { PrimaryAction } from "../../app/StepHeader";
import type { Translate } from "../../i18n/react";
import { StatusPill } from "../../ui";
import { ResultGraph } from "./ResultGraph";
import { Scorecard, Verdict } from "./Results";
import { Settings } from "./Settings";
import { RUNNING_TEXT } from "./text";
import { calibrateStepState, type CalibrateActions, type CalibrateView } from "./types";
import "./calibrate.css";

export type CalibrateScreenProps = {
  view: CalibrateView;
  actions: CalibrateActions;
  /** Gallery and tests: start with the target menu open, or the BA 고급 part open on this sub. */
  initial?: { targetMenu?: boolean | undefined; editSub?: string | undefined } | undefined;
  /** The step is blocked (the header says why); the screen's own rule when omitted. */
  blocked?: boolean | undefined;
};

export function CalibrateScreen({ view, actions, initial, blocked }: CalibrateScreenProps) {
  const computed = view.results.calibration !== null;
  const [sampleRateRequest, setSampleRateRequest] = useState(0);
  const openSampleRateSettings = () => setSampleRateRequest((request) => request + 1);
  return (
    <div className="calibrate" data-ui="calibrate">
      <Verdict view={view} actions={actions} blocked={blocked ?? calibrateStepState(view).status === "blocked"} onSampleRateSettings={openSampleRateSettings} />
      <div className={["calibrate-results", !computed && "calibrate-results--preview"].filter(Boolean).join(" ")}>
        {computed && (
          <div className="calibrate-scorecard-slot">
            <Scorecard view={view} actions={actions} />
          </div>
        )}
        <ResultGraph view={view} actions={actions} menuOpen={initial?.targetMenu} />
      </div>
      <Settings view={view} actions={actions} editSub={initial?.editSub} sampleRateRequest={sampleRateRequest} onSampleRateSettings={openSampleRateSettings} />
    </div>
  );
}

/**
 * The step header: beside the title, the job of this screen that runs or
 * the stale result. 보정 계산 is not here but in the result card, beside
 * what it computes from (DESIGN.md §1); it is off while another of this
 * screen's jobs runs (XO 추천, 서브만, MIMO), and this pill names that job.
 */
export function calibrateHeader(view: CalibrateView, t: Translate): { primary?: PrimaryAction; status?: ReactNode } {
  const running = view.running;
  const status = running ? (
    <StatusPill tone="busy" data-ui="calibrate.running">
      {t(RUNNING_TEXT[running])}
    </StatusPill>
  ) : view.results.calibration?.stale ? (
    <StatusPill tone="warning" data-ui="calibrate.stale">
      {t("frame.status.calibration.stale")}
    </StatusPill>
  ) : undefined;
  return { status };
}
