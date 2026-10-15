// The step header: the step's title, one line on what it is for, the one
// primary action, and, when the step is blocked, why and where to go.
import type { ReactNode } from "react";
import { useT } from "../i18n/react";
import { Button } from "../ui/Button";
import { Icon } from "../ui/Icon";
import { BLOCKED_TEXT, STEP_PURPOSE, STEP_TITLE, type StepId, type StepState } from "./model";

export type PrimaryAction = {
  /** Stable id for the tutorial (e.g. "measure.position.start"). */
  id: string;
  label: string;
  onClick: () => void;
  busy?: boolean | undefined;
};

export function StepHeader({
  step,
  state,
  primary,
  status,
}: {
  step: StepId;
  state: StepState;
  primary?: PrimaryAction | undefined;
  /** A pill beside the title (보정 낡음, 측정 중 …). */
  status?: ReactNode;
}) {
  const t = useT();
  const blocked = state.status === "blocked" && state.reason ? BLOCKED_TEXT[state.reason] : null;
  const reasonId = `frame.header.${step}.blocked`;
  return (
    <header className="step-header" data-ui="frame.header">
      <div className="step-header__text">
        <div className="step-header__title-row">
          <h1 className="step-header__title" id="frame.header.title">
            {t(STEP_TITLE[step])}
          </h1>
          {status}
        </div>
        <p className="step-header__purpose">{t(STEP_PURPOSE[step])}</p>
        {blocked && (
          <p className="step-header__blocked" id={reasonId} data-ui="frame.header.blocked">
            <Icon name="lock" size={16} />
            <span>{t(blocked.full, { count: state.count ?? 0 })}</span>
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
            disabled={!!blocked}
            aria-describedby={blocked ? reasonId : undefined}
            onClick={primary.onClick}
          >
            {primary.label}
          </Button>
        </div>
      )}
    </header>
  );
}
