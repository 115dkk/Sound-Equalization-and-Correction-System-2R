// The frame's update indicator: a status line in the rail's footer, above
// 설정 and 도움말, drawn only while there is something to know (model.ts
// `badgeShown`). Two lines in every state: the state with its dot, then a
// plain bar or one small secondary button under the words (받아서 설치,
// 지금 다시 시작, 릴리스 페이지, 다시 확인), on a line as tall as that
// button. It grows the footer up into the scrolling middle, so the steps,
// the step header and the content keep their places when it appears.
//
// 지금 다시 시작 (지금 설치 where installing asks for the password) installs
// and relaunches without the close flow's save prompt, so with unsaved
// changes or running jobs it first says what is lost, in one confirm.
import { useState, type ReactNode } from "react";
import { useT } from "../../i18n/react";
import { Button } from "../../ui/Button";
import { ConfirmDialog } from "../../ui/Dialog";
import { ProgressBar } from "../../ui/meters";
import { Tooltip } from "../../ui/Tooltip";
import { badgeShown, phaseText, phaseTone, phaseVersion, restartLabel } from "./model";
import { ReleaseLink, UpdateGlyph } from "./parts";
import { useUpdateApi, type UpdateApi } from "./scope";

/** The badge for the frame's update scope (none without one). */
export function RailUpdateBadge() {
  const api = useUpdateApi();
  return api ? <UpdateBadge api={api} /> : null;
}

export function UpdateBadge({ api }: { api: UpdateApi }) {
  const t = useT();
  const { view, actions } = api;
  const [confirming, setConfirming] = useState(false);
  const [pressed, setPressed] = useState(false);
  const phase = view.status?.phase;
  if (!phase || !badgeShown(phase, view.userAction)) return null;
  const text = phaseText(t, phase);
  const label = t(restartLabel(phase));
  const restart = () => {
    setPressed(true);
    void actions.restart().finally(() => setPressed(false));
  };
  const lost = [view.dirty && t("update.restart.unsaved"), view.busy && t("update.restart.jobs")].filter(Boolean).join(" ");

  let row: ReactNode = null;
  if (phase.kind === "available") {
    row = (
      <Button size="sm" data-ui="update.badge.download" onClick={() => void actions.download()}>
        {t("settings.updates.download")}
      </Button>
    );
  } else if (phase.kind === "downloading") {
    const known = phase.total !== null && phase.total > 0;
    row = <ProgressBar label={text} indeterminate={!known} done={phase.received} total={phase.total ?? 0} />;
  } else if (phase.kind === "ready") {
    const button = (
      <Button size="sm" data-ui="update.restart" busy={pressed} onClick={() => (lost ? setConfirming(true) : restart())}>
        {label}
      </Button>
    );
    row = phase.install === "on_request" ? <Tooltip content={t("update.install_now.note")}>{button}</Tooltip> : button;
  } else if (phase.kind === "installing") {
    row = <ProgressBar label={text} indeterminate />;
  } else if (phase.kind === "portable") {
    const version = phase.version;
    row = (
      <ReleaseLink id="update.portable" onOpen={() => void actions.openRelease(version)}>
        {t("update.release_page")}
      </ReleaseLink>
    );
  } else if (phase.kind === "failed") {
    row = (
      <Button size="sm" data-ui="update.badge.retry" onClick={() => void actions.check()}>
        {t("update.retry")}
      </Button>
    );
  }

  return (
    <div className="update-badge" data-ui="update.badge" data-state={phase.kind} data-version={phaseVersion(phase)} role="status">
      {/* The bar under the words shows the work; a spinner beside it would say it twice. */}
      <UpdateGlyph tone={phase.kind === "downloading" || phase.kind === "installing" ? "info" : phaseTone(phase)} />
      <span className="update-badge__text">{text}</span>
      <div className="update-badge__row">{row}</div>
      <ConfirmDialog
        id="update.restart.confirm"
        open={confirming && phase.kind === "ready"}
        onClose={() => setConfirming(false)}
        onConfirm={restart}
        title={label}
        body={lost}
        confirmLabel={label}
      />
    </div>
  );
}
