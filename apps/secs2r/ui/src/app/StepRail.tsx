// The five-step rail. Each step shows whether it is done, the current one,
// or blocked (with the short reason under the name; the full sentence is in
// the step header and the tooltip). Blocked steps can still be opened to
// read what they need; skipped steps (not on this path) cannot.
import { useT } from "../i18n/react";
import { Icon } from "../ui/Icon";
import { Tooltip } from "../ui/Tooltip";
import { BLOCKED_TEXT, STEPS, STEP_TITLE, type FrameModel, type StepId } from "./model";
import { STEP_SHORTCUTS, shortcutText } from "./shortcuts";

export function StepRail({ model, onNavigate }: { model: FrameModel; onNavigate: (step: StepId) => void }) {
  const t = useT();
  return (
    <nav className="rail-steps" aria-label={t("frame.rail")} data-ui="frame.rail">
      <ol className="rail-steps__list">
        {STEPS.map((id, index) => {
          const step = model.steps[id];
          const current = model.current === id;
          const reason = step.status === "blocked" && step.reason ? BLOCKED_TEXT[step.reason] : null;
          const stateLabel = step.status === "done" ? t("frame.rail.done") : step.status === "blocked" ? t("frame.rail.blocked") : null;
          const button = (
            <button
              type="button"
              className={`rail-step rail-step--${step.status}${current ? " rail-step--current" : ""}`}
              aria-current={current ? "step" : undefined}
              disabled={step.status === "skipped"}
              data-ui={`frame.rail.${id}`}
              id={`frame.rail.${id}`}
              onClick={() => onNavigate(id)}
            >
              <span className="rail-step__mark" aria-hidden="true">
                {step.status === "done" ? (
                  <Icon name="check" size={14} weight="semibold" />
                ) : step.status === "blocked" ? (
                  <Icon name="lock" size={14} />
                ) : (
                  <span className="tabular">{index + 1}</span>
                )}
              </span>
              <span className="rail-step__text">
                <span className="rail-step__title">{t(STEP_TITLE[id])}</span>
                {reason && <span className="rail-step__reason">{t(reason.short)}</span>}
                {step.status === "skipped" && <span className="rail-step__reason">{t("frame.rail.skipped")}</span>}
                {stateLabel && <span className="visually-hidden">{`, ${stateLabel}`}</span>}
              </span>
              <span className="rail-step__key" aria-hidden="true">
                {shortcutText(STEP_SHORTCUTS[index] ?? "step.1")}
              </span>
            </button>
          );
          return (
            <li key={id}>
              {reason ? <Tooltip content={t(reason.full, { count: step.count ?? 0 })}>{button}</Tooltip> : button}
            </li>
          );
        })}
      </ol>
    </nav>
  );
}
