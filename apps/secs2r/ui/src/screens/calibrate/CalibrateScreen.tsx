// The 보정 step (upstream wizard/step_calibration.py). Results first: the
// verdict, the per-channel scorecard and one graph; then the settings that
// change them, in five sections grouped by what they change. The screen
// draws the view and reports what the user did; P33 wires the actions.
import type { ReactNode } from "react";
import type { PrimaryAction } from "../../app/StepHeader";
import type { Translate } from "../../i18n/react";
import { StatusPill } from "../../ui";
import { ResultGraph } from "./ResultGraph";
import { Scorecard, Verdict } from "./Results";
import { Settings } from "./Settings";
import { RUNNING_TEXT } from "./text";
import type { CalibrateActions, CalibrateView } from "./types";
import "./calibrate.css";

export type CalibrateScreenProps = {
  view: CalibrateView;
  actions: CalibrateActions;
  /** Gallery and tests: start with the target menu open, or the BA 고급 part open on this sub. */
  initial?: { targetMenu?: boolean | undefined; editSub?: string | undefined } | undefined;
};

export function CalibrateScreen({ view, actions, initial }: CalibrateScreenProps) {
  const computed = view.results.calibration !== null;
  return (
    <div className="calibrate" data-ui="calibrate">
      <Verdict view={view} />
      <div className={["calibrate-results", !computed && "calibrate-results--preview"].filter(Boolean).join(" ")}>
        {computed && (
          <div className="calibrate-scorecard-slot">
            <Scorecard view={view} actions={actions} />
          </div>
        )}
        <ResultGraph view={view} actions={actions} menuOpen={initial?.targetMenu} />
      </div>
      <Settings view={view} actions={actions} editSub={initial?.editSub} />
    </div>
  );
}

/**
 * The step header: 보정 계산 (one primary action) and, beside the title, the
 * job of this screen that runs or the stale result. The primary is busy
 * while the correction runs; while another of this screen's jobs runs it is
 * left out, so a second job cannot start (the frame's PrimaryAction has no
 * disabled state, and a spinner on 보정 계산 would name the wrong job).
 */
export function calibrateHeader(view: CalibrateView, actions: CalibrateActions, t: Translate): { primary?: PrimaryAction; status?: ReactNode } {
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
  if (running !== null && running !== "calibrate") return { status };
  return {
    primary: { id: "calibrate.compute", label: t("frame.step.calibrate.primary"), onClick: actions.calibrate, busy: running === "calibrate" },
    status,
  };
}
