// 마이크 보정 파일 on the native path (ADR 0015): choose the file that came
// with the mic, see what it holds, and give it to sweeps measured without
// one. REW's own file stays a read-only fact in the REW fields.
import type { MessageKey } from "../../i18n";
import type { Translate } from "../../i18n/react";
import { useT } from "../../i18n/react";
import type { IpcError } from "../../ipc/bindings/IpcError";
import type { MicCalibrationFile } from "../../ipc/bindings/MicCalibrationFile";
import { errorText } from "../../app/jobs/jobText";
import { Button, ErrorNotice, HelpTip, Icon, StatusPill, formatHz } from "../../ui";
import type { ConnectActions, MicCalibrationView } from "./types";

/** Parser issues the service names (`params.issue`); the ones with a line say which line. */
const ISSUES = ["unreadable", "too_large", "not_text", "header", "row", "order", "range", "too_few_points", "both_conventions", "no_file"] as const;
type Issue = (typeof ISSUES)[number];
const WITH_LINE: ReadonlySet<Issue> = new Set(["header", "row", "order", "range"]);
const NEXT: Record<Issue, MessageKey> = {
  unreadable: "connect.mic_cal.error.unreadable.next",
  too_large: "connect.mic_cal.error.file.next",
  not_text: "connect.mic_cal.error.file.next",
  header: "connect.mic_cal.error.file.next",
  row: "connect.mic_cal.error.file.next",
  order: "connect.mic_cal.error.file.next",
  range: "connect.mic_cal.error.file.next",
  too_few_points: "connect.mic_cal.error.file.next",
  both_conventions: "connect.mic_cal.error.file.next",
  no_file: "connect.mic_cal.error.no_file.next",
};

const isIssue = (value: string | undefined): value is Issue => (ISSUES as readonly string[]).includes(value ?? "");

/** A mic calibration failure as cause and next step; other failures read as the app's usual error text. */
export function micCalibrationError(t: Translate, error: IpcError): { cause: string; next: string } {
  const { reason, issue, line, file } = error.params;
  if (error.code === "measurement_input" && reason === "mic_calibration" && isIssue(issue) && (!WITH_LINE.has(issue) || /^\d+$/.test(line ?? ""))) {
    const params = { file: file ?? "", line: line ?? "" };
    return { cause: t(`connect.mic_cal.error.${issue}` as MessageKey, params), next: t(NEXT[issue], params) };
  }
  const text = errorText(t, error);
  return { cause: text.cause, next: text.next };
}

/** "시리얼 7000001 · 10.1–20k Hz · 주파수 615개" */
function summary(t: Translate, file: MicCalibrationFile): string {
  const values = { serial: file.serial ?? "", low: formatHz(file.min_hz), high: formatHz(file.max_hz), points: file.points };
  return t(file.serial ? "connect.mic_cal.summary" : "connect.mic_cal.summary.no_serial", values);
}

export function MicCalibrationField({ view, actions }: { view: MicCalibrationView; actions: Pick<ConnectActions, "pickMicCalibration" | "clearMicCalibration" | "attachMicCalibration"> }) {
  const t = useT();
  const { status, busy, error } = view;
  const file = status?.file ?? null;
  const failure = error ? micCalibrationError(t, error) : null;
  const attached = status?.attached ?? 0;
  const attachable = file ? (status?.attachable ?? 0) : 0;
  return (
    <section className="connect-miccal" aria-labelledby="connect.mic_cal-label" aria-busy={busy !== null} data-ui="connect.mic_cal">
      <div className="connect-miccal__head">
        <span className="ui-field__label" id="connect.mic_cal-label">{t("connect.mic_cal")}</span>
        <HelpTip id="connect.mic_cal.help" labelledBy="connect.mic_cal-label" content={t("connect.mic_cal.help")} />
      </div>
      <div className="connect-miccal__body">
        {file ? (
          <div className="connect-miccal__file" data-ui="connect.mic_cal.file">
            <span className="connect-miccal__name" title={file.path}>{file.name}</span>
            <span className="connect-miccal__facts tabular">{summary(t, file)}</span>
            {file.notes[0] && <span className="connect-miccal__note">{file.notes[0]}</span>}
          </div>
        ) : (
          <span className="connect-readout">
            <StatusPill tone="warning" data-ui="connect.mic_cal.none">{t("connect.mic_cal.none")}</StatusPill>
          </span>
        )}
        <div className="connect-miccal__actions">
          <Button size="sm" data-ui="connect.mic_cal.pick" busy={busy === "pick"} disabled={busy !== null && busy !== "pick"} onClick={actions.pickMicCalibration}>
            {t(file ? "connect.mic_cal.replace" : "connect.mic_cal.pick")}
          </Button>
          {file && (
            <Button size="sm" variant="quiet" data-ui="connect.mic_cal.clear" busy={busy === "clear"} disabled={busy !== null && busy !== "clear"} onClick={actions.clearMicCalibration}>
              {t("connect.mic_cal.clear")}
            </Button>
          )}
        </div>
      </div>
      {attached > 0 && (
        <p className="connect-hint tabular" data-ui="connect.mic_cal.attached">
          {t("connect.mic_cal.attached", { count: attached })}
        </p>
      )}
      {attachable > 0 && (
        <div className="connect-note connect-miccal__attach" data-ui="connect.mic_cal.attachable">
          <Icon name="info" size={18} className="connect-note__icon" />
          <span className="connect-miccal__attach-text">{t("connect.mic_cal.attachable", { count: attachable })}</span>
          <Button size="sm" data-ui="connect.mic_cal.attach" busy={busy === "attach"} disabled={busy !== null && busy !== "attach"} onClick={actions.attachMicCalibration}>
            {t("connect.mic_cal.attach", { count: attachable })}
          </Button>
        </div>
      )}
      {failure && error && (
        <ErrorNotice id="connect.mic_cal.error" cause={failure.cause} next={failure.next} detail={error.detail} code={error.code} />
      )}
    </section>
  );
}
