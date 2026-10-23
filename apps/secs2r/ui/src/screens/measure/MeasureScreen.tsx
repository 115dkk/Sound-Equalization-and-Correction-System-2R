import { HelpTip } from "../../ui/HelpTip";
// 측정·가져오기 (P31a; upstream step 5, src/secs/wizard/step5_measure.py).
//
// One position at a time: the map says where the mic goes, the step's
// primary action measures it, the curves show what was measured. Importing
// REW .mdat files is the other way in and sits on the same screen. Long work
// (measuring, importing, the demo data) runs as a job: the screen shows the
// running position's progress and its last warning, the job tray keeps the
// rest (U8). The screen draws the view and reports what the user did; P33
// wires the actions to the service.
import { useMemo, useState, type ReactNode } from "react";
import { JOB_NAME } from "../../app/jobs/jobText";
import { usePrefs } from "../../app/prefs";
import type { PrimaryAction } from "../../app/StepHeader";
import { useNumber, useT, type Translate } from "../../i18n/react";
import type { Notice } from "../../ipc/bindings/Notice";
import {
  Advanced,
  ArrangementMap,
  Button,
  Card,
  EmptyState,
  ErrorNotice,
  Graph,
  Icon,
  ProgressBar,
  Segmented,
  Select,
  StatusPill,
  Tooltip,
  useConfirmAndUndo,
  type Curve,
  type LegendChannel,
} from "../../ui";
import { isSubRole, orderRoles, roleCode } from "../../ui/maps/geometry";
import { baseName, estimateText, micPoints, pad2, pointText, recordSources, sourceSummary, sweepName } from "./format";
import { MdatAssignDialog, MdatFilesDialog } from "./MdatDialogs";
import {
  OWN_JOBS,
  REPETITIONS,
  START_DELAYS,
  currentPosition,
  donePositions,
  isRunning,
  type DeviceJob,
  type MeasureActions,
  type MeasureView,
  type PositionRecord,
  type StartDelay,
} from "./types";
import "./measure.css";

/* ── Header ─────────────────────────────────────────────────────────────── */

/** What the running device job is doing, for the header pill. */
function jobLine(t: Translate, job: DeviceJob): string {
  if (job.kind === "sample_load") return t("measure.status.sample");
  if (job.kind !== "measure") return t("measure.status.job", { job: t(JOB_NAME[job.kind]) });
  if (job.status === "cancel_requested") return t("measure.status.cancelling");
  const n = job.position ?? 0;
  if (job.progress?.stage === "countdown") return t("measure.status.countdown", { n, seconds: Math.max(0, job.progress.total - job.progress.done) });
  return t("measure.run.title", { n });
}

/**
 * The step header: the one primary action and, while a device job runs, a
 * pill naming it. On the import path the primary imports; once every
 * position is measured (and none is picked) it opens 보정. The primary is
 * busy while one of this screen's own jobs runs; another step's device job
 * blocks the step instead (the frame turns the primary off and says why).
 */
export function measureHeader(view: MeasureView, actions: MeasureActions, t: Translate): { primary?: PrimaryAction; status?: ReactNode } {
  const job = isRunning(view.job) ? view.job : null;
  const own = job !== null && OWN_JOBS.includes(job.kind);
  const status = job ? (
    <StatusPill tone="busy" data-ui="measure.status">
      {jobLine(t, job)}
    </StatusPill>
  ) : undefined;
  if (view.path === "import") {
    return { primary: { id: "measure.mdat.load", label: t("measure.mdat.load"), onClick: () => void actions.pickMdat(null), busy: own }, status };
  }
  const current = currentPosition(view);
  if (current === null) return { primary: { id: "measure.next", label: t("measure.primary.next"), onClick: actions.openCalibrate }, status };
  const again = donePositions(view).includes(current);
  return {
    primary: {
      id: "measure.position.start",
      label: t(again ? "measure.primary.remeasure" : "frame.step.measure.primary"),
      onClick: () => void actions.measurePosition(current),
      busy: own,
    },
    status,
  };
}

/* ── Screen ─────────────────────────────────────────────────────────────── */

export function MeasureScreen({ view, actions }: { view: MeasureView; actions: MeasureActions }) {
  const t = useT();
  const tutorial = usePrefs().accent === "tutorial";
  const busy = isRunning(view.job);
  const count = view.layout.count;
  const current = currentPosition(view);
  const done = donePositions(view);
  const lastDone = done[done.length - 1] ?? null;
  const records = useMemo(() => new Map(view.records.map((record) => [record.position, record])), [view.records]);
  const [filesOpen, setFilesOpen] = useState(false);
  const [clearTarget, setClearTarget] = useState<number | null>(null);

  const clearing = clearTarget ?? 0;
  const clear = useConfirmAndUndo({
    id: "measure.clear",
    title: t("measure.clear.title", { n: clearing }),
    body: t("measure.clear.body", { count: records.get(clearing)?.roles.length ?? 0 }),
    confirmLabel: t("measure.clear.confirm"),
    done: t("measure.clear.done", { n: clearing }),
    run: () => actions.clearPosition(clearing),
    undo: actions.undo,
  });
  const askClear = (position: number) => {
    setClearTarget(position);
    clear.request();
  };
  const select = (position: number) => {
    if (!busy) actions.selectPosition(position);
  };

  const classes = [
    "measure-screen",
    view.graphHidden && "measure-screen--graph-hidden",
    view.path === "import" && "measure-screen--import",
  ]
    .filter(Boolean)
    .join(" ");

  return (
    <div className={classes} data-ui="measure">
      <div className="measure-layout">
        <div className="measure-stack measure-stack--lead">
          <WhereCard view={view} actions={actions} current={current} onSelect={select} />
          <PositionsCard
            view={view}
            current={current}
            records={records}
            busy={busy}
            lastDone={lastDone}
            onSelect={select}
            onClear={askClear}
          />
        </div>
        <div className="measure-stack measure-stack--side">
          <GraphCard view={view} actions={actions} />
          <Card dense className="measure-import" data-ui="measure.import" title={t("measure.import.title")} help={t("measure.import.description")}>
            <div className="measure-buttons">
              {view.path !== "import" && (
                <Button id="measure.mdat.load" data-ui="measure.mdat.load" icon="file" disabled={busy} onClick={() => void actions.pickMdat(null)}>
                  {t("measure.mdat.load")}
                </Button>
              )}
              <Button id="measure.files.open" data-ui="measure.files.open" icon="list" onClick={() => setFilesOpen(true)}>
                {t("measure.files.open")}
              </Button>
            </div>
            {tutorial && (
              <div className="measure-sample">
                <Button id="measure.sample.load" data-ui="measure.sample.load" icon="play" disabled={busy} onClick={() => void actions.loadSample()} aria-describedby="measure.sample.help-content">
                  {t("measure.sample.load")}
                </Button>
                <HelpTip id="measure.sample.help" label={t("measure.sample.load")} content={t("measure.sample.hint")} />
              </div>
            )}
          </Card>
          {view.path !== "import" && <MeasureOptions view={view} actions={actions} busy={busy} />}
        </div>
      </div>

      {view.mdat && (
        <MdatAssignDialog
          key={`${view.mdat.path}#${view.mdat.position ?? 0}#${view.mdat.scan.measurements.length}`}
          pending={view.mdat}
          roles={view.config.roles}
          count={count}
          fallbackPosition={current ?? 1}
          onImport={(params) => void actions.importMdat(params)}
          onClose={actions.closeMdat}
        />
      )}
      <MdatFilesDialog
        open={filesOpen}
        count={count}
        records={view.records}
        roleCount={view.config.roles.length}
        busy={busy}
        onLoad={(position) => void actions.pickMdat(position)}
        onClear={askClear}
        onClose={() => setFilesOpen(false)}
      />
      {clear.dialog}
    </div>
  );
}

/* ── Where the mic goes ─────────────────────────────────────────────────── */

function WhereCard({
  view,
  actions,
  current,
  onSelect,
}: {
  view: MeasureView;
  actions: MeasureActions;
  current: number | null;
  onSelect: (position: number) => void;
}) {
  const t = useT();
  const count = view.layout.count;
  const job = isRunning(view.job) ? view.job : null;
  const measuring = job?.kind === "measure" ? job : null;
  const title = current !== null ? t("measure.where.title", { n: current, count }) : t("measure.where.all_done", { count });
  const description =
    current === null ? (
      t("measure.where.all_done.hint")
    ) : (
      <>
        <span className="measure-where__place">{pointText(t, current)}</span>
        {view.path !== "import" && view.estimate && <span className="measure-where__estimate tabular">{estimateText(t, view.estimate.per_position_seconds)}</span>}
      </>
    );
  const warnings = current !== null ? (view.warnings.find((item) => item.position === current)?.notices ?? []) : [];
  return (
    <Card
      dense
      className="measure-where"
      data-ui="measure.where"
      title={<span className="tabular">{title}</span>}
      description={description}
      actions={
        <Button size="sm" variant="quiet" data-ui="measure.graph.toggle" icon="axes" onClick={() => actions.setGraphHidden(!view.graphHidden)}>
          {t(view.graphHidden ? "measure.graph.show" : "measure.graph.hide")}
        </Button>
      }
    >
      <div className="measure-map">
        <ArrangementMap id="measure.map" count={count} points={micPoints(view.layout)} done={donePositions(view)} current={current} onSelect={onSelect} />
      </div>
      {measuring && <RunProgress job={measuring} roleCount={view.config.roles.length} onCancel={() => void actions.cancelJob(measuring.job)} />}
      {!measuring && warnings.length > 0 && <SnrWarning notices={warnings} />}
      {view.path === "import" && (
        <p className="measure-note" data-ui="measure.import.note">
          <Icon name="info" size={16} />
          <span>{t("measure.where.import")}</span>
        </p>
      )}
    </Card>
  );
}

/** Progress across the plan's roles for the position being measured, and its cancel. */
function RunProgress({ job, roleCount, onCancel }: { job: DeviceJob; roleCount: number; onCancel: () => void }) {
  const t = useT();
  const n = job.position ?? 0;
  const progress = job.progress;
  const countdown = progress?.stage === "countdown" ? progress : null;
  const cancelling = job.status === "cancel_requested";
  const total = countdown ? countdown.total : (progress?.total ?? roleCount);
  const doneCount = countdown ? countdown.done : (progress?.done ?? 0);
  const line = countdown
    ? t("measure.run.countdown", { seconds: Math.max(0, countdown.total - countdown.done) })
    : progress
      ? t("measure.run.channels", { done: progress.done, total: progress.total })
      : t("measure.run.starting");
  const last = !countdown && progress ? roleCode(progress.stage) : null;
  const hint = cancelling ? t("measure.run.cancelling") : countdown ? t("measure.run.countdown.hint", { n }) : null;
  return (
    <div className="measure-run" data-ui="measure.run">
      <div className="measure-run__head">
        <p className="measure-run__line">
          <span className="measure-run__count tabular">{line}</span>
          {last && <span className="measure-run__last">{t("measure.run.last", { code: last })}</span>}
        </p>
        <Tooltip content={t("measure.cancel.hint")}>
          <Button size="sm" id="measure.cancel" data-ui="measure.cancel" icon="stop" static disabled={cancelling} onClick={onCancel}>
            {t("measure.cancel")}
          </Button>
        </Tooltip>
      </div>
      <ProgressBar
        id="measure.run.progress"
        label={t("measure.run.label", { n })}
        done={doneCount}
        total={total}
        indeterminate={!progress}
        valueText={line}
      />
      {hint && <p className="measure-hint">{hint}</p>}
    </div>
  );
}

/**
 * The low S/N notices of the current position (U6: the threshold comes with
 * the notice, 20 dB). One notice reads as its sentence; several as one
 * sentence with the channels listed.
 */
function SnrWarning({ notices }: { notices: readonly Notice[] }) {
  const t = useT();
  const number = useNumber();
  const value = (text: string | undefined, digits: number) => {
    const n = Number(text);
    return Number.isFinite(n) ? number(n, digits) : (text ?? "");
  };
  const params = (notice: Notice) => ({
    role: roleCode(notice.params.role ?? ""),
    snr_db: value(notice.params.snr_db, 1),
    threshold_db: value(notice.params.threshold_db, 0),
  });
  const [first] = notices;
  if (!first) return null;
  const cause =
    notices.length === 1
      ? t("notice.measurement_low_snr", params(first))
      : t("measure.warning.low_snr", {
          threshold_db: params(first).threshold_db,
          // Each "SW2 17.2 dB" stays on one line.
          list: notices.map((notice) => t("measure.warning.low_snr.item", params(notice)).replace(/ /g, "\u00a0")).join(", "),
        });
  return <ErrorNotice id="measure.warning" tone="warning" cause={cause} next={t("notice.measurement_low_snr.next")} />;
}

/* ── Positions ──────────────────────────────────────────────────────────── */

type RowState = "measuring" | "redo" | "done" | "next" | "partial" | "waiting";

function PositionsCard({
  view,
  current,
  records,
  busy,
  lastDone,
  onSelect,
  onClear,
}: {
  view: MeasureView;
  current: number | null;
  records: ReadonlyMap<number, PositionRecord>;
  busy: boolean;
  lastDone: number | null;
  onSelect: (position: number) => void;
  onClear: (position: number) => void;
}) {
  const t = useT();
  const count = view.layout.count;
  const roleCount = view.config.roles.length;
  const done = new Set(donePositions(view));
  const measuringAt = isRunning(view.job) && view.job.kind === "measure" ? view.job.position : null;
  const summary = sourceSummary(t, view.records);
  const stateOf = (n: number): RowState => {
    if (n === measuringAt) return "measuring";
    if (done.has(n)) return n === current ? "redo" : "done";
    if (n === current) return "next";
    return (records.get(n)?.roles.length ?? 0) > 0 ? "partial" : "waiting";
  };
  const stateText = (n: number, state: RowState) => {
    switch (state) {
      case "measuring":
        return t("measure.positions.measuring");
      case "redo":
        return t("map.position.redo");
      case "done":
        return t("map.position.done");
      case "next":
        return t("map.position.next");
      case "partial":
        return t("measure.positions.partial", { have: records.get(n)?.roles.length ?? 0, total: roleCount });
      default:
        return t("map.position.waiting");
    }
  };
  const sourceText = (record: PositionRecord | undefined) => {
    if (!record || record.roles.length === 0) return t("measure.positions.source.none");
    const files = recordSources(record).filter((name) => name !== "");
    const here = recordSources(record).includes("");
    if (files.length === 0) return t("measure.positions.source.measured");
    const fileLine = t("measure.positions.source.file", { file: files.join(" + ") });
    return here ? `${fileLine} · ${t("measure.positions.source.measured")}` : fileLine;
  };
  return (
    <Card
      dense
      className="measure-positions"
      data-ui="measure.positions.card"
      title={t("measure.positions.title")}
      actions={
        lastDone !== null ? (
          <Button size="sm" variant="quiet" data-ui="measure.redo" disabled={busy} onClick={() => onClear(lastDone)}>
            {t("measure.redo", { n: lastDone })}
          </Button>
        ) : undefined
      }
    >
      <ol className="measure-list" data-ui="measure.positions" aria-label={t("measure.positions.label", { count })}>
        {Array.from({ length: count }, (_, i) => {
          const n = i + 1;
          const state = stateOf(n);
          const label = stateText(n, state);
          return (
            <li key={n}>
              <Tooltip content={sourceText(records.get(n))}>
                <button
                  type="button"
                  className={`measure-row measure-row--${state}${n === current ? " measure-row--current" : ""}`}
                  data-ui={`measure.positions.${n}`}
                  aria-current={n === current ? "true" : undefined}
                  aria-disabled={busy || undefined}
                  onClick={() => onSelect(n)}
                >
                  <span className="measure-row__num tabular">{pad2(n)}</span>
                  <span className="measure-row__text">{pointText(t, n)}</span>
                  <span className="measure-row__state">
                    <span className="measure-row__dot" aria-hidden="true" />
                    <span className="tabular">{label}</span>
                  </span>
                </button>
              </Tooltip>
            </li>
          );
        })}
      </ol>
      {summary && (
        <p className="measure-sources" data-ui="measure.sources">
          {t("measure.sources", { list: summary })}
        </p>
      )}
    </Card>
  );
}

/* ── What was measured ──────────────────────────────────────────────────── */

function GraphCard({ view, actions }: { view: MeasureView; actions: MeasureActions }) {
  const t = useT();
  const data = view.curves;
  const roleOrder = useMemo(() => {
    const roles = [...view.config.roles];
    for (const curve of data?.curves ?? []) if (!roles.includes(curve.role)) roles.push(curve.role);
    return orderRoles(roles);
  }, [view.config.roles, data]);
  const channels: LegendChannel[] = useMemo(() => roleOrder.map((role) => ({ role, label: roleCode(role), sub: isSubRole(role) })), [roleOrder]);
  const curves: Curve[] = useMemo(
    () =>
      (data?.curves ?? []).map((curve) => ({
        id: `${curve.role}@${curve.position}`,
        role: curve.role,
        kind: curve.kind === "sub" || isSubRole(curve.role) ? "sub" : "main",
        position: curve.position,
        values: curve.values,
      })),
    [data],
  );
  const positions = useMemo(() => [...new Set(curves.map((curve) => curve.position ?? 1))].sort((a, b) => a - b), [curves]);
  return (
    <Card
      dense
      className="measure-graph"
      data-ui="measure.graph"
      hidden={view.graphHidden}
      title={t("plot.measurement.title")}
      help={curves.length > 0 ? t("measure.graph.description") : undefined}
    >
      {data && curves.length > 0 ? (
        <Graph
          id="measure.plot"
          label={t("plot.measurement.title")}
          freqs={data.grid}
          curves={curves}
          channels={channels}
          roleOrder={roleOrder}
          positions={positions}
          xRange={data.axes.frequency_hz}
          yRange={data.axes.value}
          range={view.plotRange}
          onRangeChange={actions.setPlotRange}
          height={300}
        />
      ) : (
        <EmptyState id="measure.graph.empty" title={t("measure.graph.empty.title")} description={t("measure.graph.empty.body")} />
      )}
    </Card>
  );
}

/* ── Options ────────────────────────────────────────────────────────────── */

/**
 * Start delay (shown), sweep length and repetitions (고급). Repetitions are
 * offered only where they can be used: REW with a loopback or no timing
 * reference. Native measurement records one sweep per channel, and REW's
 * acoustic reference allows one, so elsewhere the choice is not drawn.
 */
export function MeasureOptions({
  view,
  actions,
  busy,
  advancedOpen = false,
}: {
  view: MeasureView;
  actions: Pick<MeasureActions, "setStartDelay" | "setSweepLength" | "setRepetitions">;
  busy: boolean;
  /** Gallery: draw 고급 open. */
  advancedOpen?: boolean;
}) {
  const t = useT();
  const number = useNumber();
  const repeatable = view.config.method === "rew" && (view.layout.timing.reference === "loopback" || view.layout.timing.reference === "none");
  const delay = (START_DELAYS as readonly string[]).includes(view.layout.start_delay) ? (view.layout.start_delay as StartDelay) : START_DELAYS[0];
  const delayLabel = (value: StartDelay) => (value === START_DELAYS[0] ? t("measure.options.delay.off") : t("measure.options.delay.seconds", { n: Number.parseInt(value, 10) }));
  const repetitions = (REPETITIONS as readonly number[]).includes(view.repetitions) ? view.repetitions : 1;
  return (
    <Card dense className="measure-options" data-ui="measure.options" title={t("measure.options.title")}>
      <Select
        id="measure.options.delay"
        label={t("measure.options.delay")}
        value={delay}
        options={START_DELAYS.map((value) => ({ value, label: delayLabel(value) }))}
        onChange={(value) => void actions.setStartDelay(value)}
        help={t("measure.options.delay.hint")}
        disabled={busy}
      />
      <Advanced id="measure.options.advanced" count={repeatable ? 2 : 1} defaultOpen={advancedOpen}>
        <div className="measure-options__advanced">
          <Select
            id="measure.options.sweep"
            label={t("measure.options.sweep")}
            value={String(view.config.sweep_length)}
            options={view.sweepOptions.map((option) => ({
              value: String(option.samples),
              label: t("measure.options.sweep.option", { length: sweepName(option.samples), seconds: number(option.seconds, 1) }),
            }))}
            onChange={(value) => void actions.setSweepLength(Number(value))}
            help={t("measure.options.sweep.hint")}
            disabled={busy}
          />
          {repeatable && (
            <Segmented
              id="measure.options.repeats"
              label={t("measure.options.repeats")}
              help={t("measure.options.repeats.hint")}
              size="sm"
              value={String(repetitions)}
              options={REPETITIONS.map((n) => ({ value: String(n), label: t("measure.options.repeats.option", { n }), disabled: busy }))}
              onChange={(value) => void actions.setRepetitions(Number(value))}
            />
          )}
        </div>
      </Advanced>
    </Card>
  );
}

/** File name for the frame's status (`ProjectInfo.path` is "" until the project is saved). */
export const projectFileName = (view: MeasureView) => (view.project.path ? baseName(view.project.path) : null);
