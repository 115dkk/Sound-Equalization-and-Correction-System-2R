// The step header: the step's title, one line on what it is for, the one
// primary action, and, when the step is blocked, why and where to go.
import type { ReactNode } from "react";
import { useT } from "../i18n/react";
import { Button } from "../ui/Button";
import { HelpTip } from "../ui/HelpTip";
import { Icon } from "../ui/Icon";
import { BLOCKED_TEXT, STEP_PURPOSE, STEP_TITLE, type StepId, type StepState } from "./model";

export type PrimaryAction = {
  /** Stable id for the tutorial (e.g. "measure.position.start"). */
  id: string;
  label: string;
  onClick: () => void;
  /** This action's own job runs: a spinner, the label kept, clicks blocked. */
  busy?: boolean | undefined;
  /**
   * Off for a reason of the screen's own (another of its jobs runs). The
   * header status beside the title says why; a blocked step turns it off
   * without this.
   */
  disabled?: boolean | undefined;
};

export type RecoveryAction = Pick<PrimaryAction, "id" | "label" | "onClick">;

export function StepHeader({
  step,
  state,
  primary,
  status,
  recovery,
}: {
  step: StepId;
  state: StepState;
  primary?: PrimaryAction | undefined;
  /** A pill beside the title (보정 낡음, 측정 중 …). */
  status?: ReactNode;
  /** Opens the control that resolves this blocked state. */
  recovery?: RecoveryAction | undefined;
}) {
  const t = useT();
  const blocked = state.status === "blocked" && state.reason ? BLOCKED_TEXT[state.reason] : null;
  const reasonId = `frame.header.${step}.blocked`;
  const statusId = `frame.header.${step}.status`;
  // The primary names why it is off: the blocked reason, or the status pill.
  const describedBy = blocked ? reasonId : primary?.disabled && status ? statusId : undefined;
  return (
    <header className="step-header" data-ui="frame.header">
      <div className="step-header__text">
        <div className="step-header__title-row">
          <h1 className="step-header__title" id="frame.header.title">
            {t(STEP_TITLE[step])}
          </h1>
          <HelpTip id={`frame.header.${step}.help`} labelledBy="frame.header.title" content={t(STEP_PURPOSE[step])} />
          {status && (
            <div className="step-header__status" id={statusId}>
              {status}
            </div>
          )}
        </div>
        {blocked && (
          <p className="step-header__blocked" id={reasonId} data-ui="frame.header.blocked">
            <Icon name="lock" size={16} />
            <span>{t(blocked.short, { count: state.count ?? 0 })}</span>
            {recovery && (
              <Button variant="quiet" className="step-header__recovery" id={recovery.id} data-ui={recovery.id} onClick={recovery.onClick}>
                {recovery.label}
              </Button>
            )}
          </p>
        )}
      </div>
      {primary && (
        <div className="step-header__actions">
          <Button
            variant="primary"
            id={primary.id}
            data-ui={primary.id}
            busy={primary.busy}
            disabled={!!blocked || !!primary.disabled}
            aria-describedby={describedBy}
            onClick={primary.onClick}
          >
            {primary.label}
          </Button>
        </div>
      )}
    </header>
  );
}
