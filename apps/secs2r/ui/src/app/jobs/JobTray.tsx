// The job tray (U8): one place for every running job and for what jobs
// reported. Each running job shows its name, stage, progress (done/total or
// indeterminate), elapsed time and, once the rate is known, the time left,
// with a cancel that really stops it. The history keeps Log notices and
// results; errors carry the cause and the next step, the raw text behind 자세히.
import { useId, useState } from "react";
import { isActiveJob } from "../../state/jobRetention";
import { formatDuration, useT } from "../../i18n/react";
import { Button } from "../../ui/Button";
import { Icon, Spinner } from "../../ui/Icon";
import { ProgressBar } from "../../ui/meters";
import { ErrorNotice } from "../../ui/states";
import { elapsedMs, errorCount, remainingMs, type HistoryEntry } from "../../state/jobLedger";
import { JOB_NAME, WARNING_LOGS, errorText, logText, stageText } from "./jobText";
import { useJobs, type JobsApi, type JobView } from "./useJobs";

export function JobTray({ api, defaultOpen = false }: { api: JobsApi; defaultOpen?: boolean | undefined }) {
  const t = useT();
  const historyId = useId();
  const [open, setOpen] = useState(defaultOpen);
  const active = api.state.jobs.filter((job) => isActiveJob(job.status));
  const errors = errorCount(api.state);
  const count = api.state.history.length;
  return (
    <section className={["job-tray", open && "job-tray--open"].filter(Boolean).join(" ")} aria-label={t("jobs.tray")} data-ui="jobs.tray">
      {open && (
        <div className="job-tray__history" id={historyId} data-ui="jobs.history">
          <header className="job-tray__history-head">
            <h2 className="job-tray__history-title">{t("jobs.history")}</h2>
            {count > 0 && (
              <Button variant="quiet" size="sm" onClick={api.clearHistory} data-ui="jobs.history.clear">
                {t("jobs.history.clear")}
              </Button>
            )}
          </header>
          {count === 0 ? (
            <p className="job-tray__empty">{t("jobs.history.empty")}</p>
          ) : (
            <ol className="job-tray__entries">
              {api.state.history.map((entry) => (
                <HistoryItem key={entry.id} entry={entry} />
              ))}
            </ol>
          )}
        </div>
      )}
      <div className="job-tray__bar">
        <div className="job-tray__running" aria-live="polite">
          {active.length === 0 ? (
            <span className="job-tray__idle">{t("jobs.idle")}</span>
          ) : (
            active.map((job) => <JobRow key={job.id} job={job} now={api.now} onCancel={() => api.cancel(job.id)} />)
          )}
        </div>
        <button
          type="button"
          className="job-tray__toggle"
          aria-expanded={open}
          aria-controls={open ? historyId : undefined}
          data-ui="jobs.history.toggle"
          onClick={() => setOpen((value) => !value)}
        >
          <Icon name="list" size={18} />
          <span>{t("jobs.history")}</span>
          {count > 0 && <span className="job-tray__count tabular">{count}</span>}
          {errors > 0 && <span className="job-tray__errors tabular">{t("jobs.history.errors", { count: errors })}</span>}
          <Icon name={open ? "chevronDown" : "chevronUp"} size={16} />
        </button>
      </div>
    </section>
  );
}

function JobRow({ job, now, onCancel }: { job: JobView; now: number; onCancel: () => void }) {
  const t = useT();
  const name = t(JOB_NAME[job.kind]);
  const stage = stageText(t, job.kind, job.stage);
  const left = remainingMs(job, now);
  const known = job.total > 0;
  const cancelling = job.status === "cancel_requested";
  return (
    <div className="job-row" data-ui={`jobs.job.${job.kind}`}>
      <Spinner size={16} className="job-row__spinner" />
      <div className="job-row__text">
        <span className="job-row__name">{name}</span>
        {stage && <span className="job-row__stage">{stage}</span>}
      </div>
      <div className="job-row__progress">
        <ProgressBar
          label={name}
          done={job.done}
          total={job.total}
          indeterminate={!known}
          valueText={known ? t("jobs.progress", { done: job.done, total: job.total }) : t("jobs.status.running")}
        />
        {known && <span className="job-row__count tabular">{t("jobs.progress", { done: job.done, total: job.total })}</span>}
      </div>
      <span className="job-row__time tabular">
        {t("jobs.elapsed", { time: formatDuration(elapsedMs(job, now)) })}
        {left !== null && <span className="job-row__left">{t("jobs.remaining", { time: formatDuration(left) })}</span>}
      </span>
      <Button
        size="sm"
        variant="secondary"
        busy={cancelling}
        disabled={!job.cancellable && !cancelling}
        onClick={onCancel}
        data-ui={`jobs.cancel.${job.kind}`}
        static
      >
        {cancelling ? t("jobs.status.cancel_requested") : t("jobs.cancel")}
      </Button>
    </div>
  );
}

const clock = (at: number) => {
  const d = new Date(at);
  return [d.getHours(), d.getMinutes(), d.getSeconds()].map((v) => String(v).padStart(2, "0")).join(":");
};

function HistoryItem({ entry }: { entry: HistoryEntry }) {
  const t = useT();
  const job = entry.kind ? t(JOB_NAME[entry.kind]) : null;
  const time = <time className="job-entry__time tabular">{clock(entry.at)}</time>;
  if (entry.type === "log") {
    const { text, known } = logText(t, entry.code, entry.params);
    const warning = WARNING_LOGS.has(entry.code);
    return (
      <li className={`job-entry job-entry--${warning ? "warning" : "note"}`}>
        <Icon name={warning ? "warning" : "info"} size={16} className="job-entry__icon" />
        <div className="job-entry__body">
          <p>
            <span className="job-entry__job">{job}</span> {text}
          </p>
          {!known && (
            <details className="ui-details">
              <summary>{t("ui.details")}</summary>
              <pre className="ui-details__raw">{`[${entry.code}] ${JSON.stringify(entry.params)}`}</pre>
            </details>
          )}
        </div>
        {time}
      </li>
    );
  }
  if (entry.type === "finished" && entry.status !== "failed") {
    const ok = entry.status === "succeeded";
    return (
      <li className={`job-entry job-entry--${ok ? "success" : "note"}`}>
        <Icon name={ok ? "check" : "stop"} size={16} className={ok ? "job-entry__icon" : "job-entry__icon job-entry__icon--filled"} />
        <div className="job-entry__body">
          <p>{t(ok ? "jobs.finished.succeeded" : "jobs.finished.cancelled", { job: job ?? "" })}</p>
        </div>
        {time}
      </li>
    );
  }
  const error = entry.error ?? { code: "internal" as const, params: {}, retryable: false, detail: "" };
  const text = errorText(t, error);
  return (
    <li className="job-entry job-entry--error">
      <div className="job-entry__body">
        <ErrorNotice
          cause={job ? t("jobs.finished.failed", { job, cause: text.cause }) : text.cause}
          next={text.next}
          detail={error.detail}
          code={error.code}
        />
      </div>
      {time}
    </li>
  );
}

/** The tray wired to the session's ledger. */
export function ConnectedJobTray({ defaultOpen }: { defaultOpen?: boolean | undefined }) {
  const api = useJobs();
  return <JobTray api={api} defaultOpen={defaultOpen} />;
}
