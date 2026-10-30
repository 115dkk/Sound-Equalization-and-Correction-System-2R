// What the correction achieved, first (ARCHITECTURE §7): the verdict card
// (is it good, is it current) and the per-channel scorecard whose rows pick
// the channel the graph shows.
import { useEffect, useRef, useState, type ReactNode } from "react";
import { errorText } from "../../app/jobs/jobText";
import { useNumber, useT } from "../../i18n/react";
import { Button, Card, DataTable, ErrorNotice, Spinner, StatusPill, Toggle, type Column } from "../../ui";
import { roleCode } from "../../ui/maps/geometry";
import { Stat } from "./controls";
import { availableTabs } from "./ResultGraph";
import { scorecardModel, type ChangeValues, type LatePositions, type ScoreRow } from "./scorecard";
import { MIMO_MIN_POSITIONS, mimoIncluded } from "./settingsView";
import { FAILED_TEXT, LOW_PROTECT_NEXT, groupOf, lowProtectFailures, modeName, noteLines, noticeText } from "./text";
import type { CalibrateActions, CalibrateView, StaleReason } from "./types";

const STALE_TEXT: Record<StaleReason | "unknown", "calibrate.stale.settings" | "calibrate.stale.target" | "calibrate.stale.measurements" | "calibrate.stale.config" | "calibrate.stale.unknown"> = {
  settings: "calibrate.stale.settings",
  target: "calibrate.stale.target",
  measurements: "calibrate.stale.measurements",
  config: "calibrate.stale.config",
  unknown: "calibrate.stale.unknown",
};

/**
 * A number and its unit from a `{value} …` catalog template, as two spans,
 * so the card's one display-size figure can keep its digits large and its
 * unit at title size (DESIGN.md §4: the big number is the digits, not the
 * unit). The template decides which side the unit is on.
 */
function Figure({ template, value }: { template: "calibrate.unit.percent" | "calibrate.unit.ms"; value: string }) {
  const t = useT();
  const marker = "\uE000";
  const [lead = "", trail = ""] = t(template, { value: marker }).split(marker);
  return (
    <>
      {lead.trim() && <span className="calibrate-change__unit">{lead.trim()}</span>}
      <span className="calibrate-change__number">{value}</span>
      {trail.trim() && <span className="calibrate-change__unit">{trail.trim()}</span>}
    </>
  );
}

/** A before → after pair; the screen reader hears "보정 전 …, 보정 후 …". `figure` draws `after` as digits + unit. */
function Change({ before, after, figure }: { before: string; after: string; figure?: ReactNode }) {
  const t = useT();
  return (
    <span className="calibrate-change" aria-label={t("calibrate.change.aria", { before, after })}>
      <span className="calibrate-change__before" aria-hidden="true">
        {before}
      </span>
      <span className="calibrate-change__arrow" aria-hidden="true">
        →
      </span>
      <span className="calibrate-change__after" aria-hidden="true">
        {figure ?? after}
      </span>
    </span>
  );
}

/**
 * MIMO 함께 계산 (P142): whether 보정 계산 also computes MIMO, right before
 * the button it changes. It is the MIMO card's `mimo_on`, so the two
 * switches move together. Below three measured positions it is off and says
 * why at the end of its own line, so the row keeps its height.
 */
function MimoChoice({ view, actions }: { view: CalibrateView; actions: CalibrateActions }) {
  const t = useT();
  const available = view.positions.length >= MIMO_MIN_POSITIONS;
  return (
    <div className="calibrate-run__mimo">
      <Toggle
        id="calibrate.run.mimo"
        label={t("calibrate.run.mimo")}
        help={t("calibrate.run.mimo.help")}
        hint={available ? undefined : t("calibrate.run.mimo.unavailable", { min: MIMO_MIN_POSITIONS })}
        checked={available && view.settings.mimo_on}
        disabled={!available}
        onChange={(on) => void actions.changeSetting({ kind: "mimo_on", on })}
      />
    </div>
  );
}

/**
 * The correction run lives in this card (DESIGN.md §1: the button beside
 * what it acts on). Before a result: the plan sentence, MIMO 함께 계산 and
 * 보정 계산 after it; while the correction runs: a spinner and what it
 * computes, in the controls' height; with a result: the figures and, after
 * them, 다시 측정하기, MIMO 함께 계산 and 다시 보정하기, and on a current
 * result 보정값 내보내기 last. The same inputs give the same result here
 * (upstream's runs could differ), so measuring again is the one that can
 * change it and carries the emphasis; the way on to 내보내기 is in the go
 * colour, apart from it (the rail is not where users look for it).
 * `blocked` is the step's blocked state (the header says why).
 */
function RunControls({ view, actions, blocked, again, current = false }: { view: CalibrateView; actions: CalibrateActions; blocked: boolean; again: boolean; current?: boolean }) {
  const t = useT();
  const count = view.positions.length;
  if (view.running === "calibrate") {
    return (
      <p className="calibrate-run__busy" role="status" data-ui="calibrate.verdict.computing">
        <Spinner size={18} />
        <span>{t(view.runningMimo ? "calibrate.verdict.computing.mimo" : "calibrate.verdict.computing", { count })}</span>
      </p>
    );
  }
  // Another of this screen's jobs runs: the header pill names it, as the step's blocked reason names a block.
  const off = blocked || view.running !== null || count === 0;
  const reason = blocked ? "frame.header.calibrate.blocked" : view.running !== null ? "frame.header.calibrate.status" : undefined;
  // MIMO 함께 계산 and the button it changes wrap as one.
  const compute = (
    <div className="calibrate-run__compute">
      <MimoChoice view={view} actions={actions} />
      <Button variant={again ? "secondary" : "primary"} id="calibrate.compute" data-ui="calibrate.compute" disabled={off} aria-describedby={reason} onClick={actions.calibrate}>
        {t(again ? "calibrate.verdict.recompute" : "frame.step.calibrate.primary")}
      </Button>
    </div>
  );
  return again ? (
    <div className="calibrate-run__actions">
      <Button variant="primary" data-ui="calibrate.remeasure" onClick={actions.goToMeasurements}>
        {t("calibrate.verdict.remeasure")}
      </Button>
      {compute}
      {current && (
        <Button variant="go" data-ui="calibrate.export" onClick={actions.goToExport}>
          {t("calibrate.verdict.export")}
        </Button>
      )}
    </div>
  ) : compute;
}

/** The plan sentence before the first run: how many positions it corrects from, and MIMO when 보정 계산 computes it too. */
function planKey(view: CalibrateView) {
  if (mimoIncluded(view.settings, view.positions.length)) return "calibrate.verdict.plan.mimo";
  return view.positions.length === 1 ? "calibrate.verdict.plan.one" : "calibrate.verdict.plan";
}

export function Verdict({
  view,
  actions,
  blocked,
  onSampleRateSettings,
  notesOpen = false,
}: {
  view: CalibrateView;
  actions: CalibrateActions;
  blocked: boolean;
  onSampleRateSettings: () => void;
  /** Gallery: start with the calculation notes open. */
  notesOpen?: boolean | undefined;
}) {
  const t = useT();
  const number = useNumber();
  const result = view.results.calibration;
  const mimo = view.results.mimo;
  const failure = view.failure?.job === "calibrate" ? view.failure : null;
  const resultMode = view.resultMode ?? (result && !result.stale ? { mode: view.settings.mode, ultra_ms: view.settings.ultra_ms } : null);

  // A stale result is named by the header pill and explained by the notice below; no third mark here.
  const pill = !result ? (
    <StatusPill tone="neutral">{t("calibrate.verdict.none")}</StatusPill>
  ) : result.stale ? undefined : (
    <StatusPill tone="success">{t("calibrate.verdict.current")}</StatusPill>
  );

  const failureText = failure ? errorText(t, failure.error) : null;
  const sampleRateFailure = failure?.error.params.reason === "measurement_sample_rate";
  const failed = failure && failureText && (
    <ErrorNotice
      id="calibrate.verdict.failed"
      cause={failureText.generic ? t(FAILED_TEXT[failure.job]) : `${t(FAILED_TEXT[failure.job])} ${failureText.cause}`}
      next={failureText.next}
      detail={failure.error.detail}
      code={failure.error.code}
      action={failure.error.code === "engine_input" && !failureText.generic ? (
        <Button variant="secondary" data-ui={`calibrate.verdict.failed.${sampleRateFailure ? "sample_rate" : "measurements"}`} onClick={sampleRateFailure ? onSampleRateSettings : actions.goToMeasurements}>
          {t(sampleRateFailure ? "calibrate.failed.sample_rate_settings" : "calibrate.failed.inspect_measurements")}
        </Button>
      ) : undefined}
    />
  );

  if (!result) {
    return (
      <Card
        title={t("calibrate.verdict.title")}
        actions={pill}
        className="calibrate-verdict"
        data-ui="calibrate.verdict"
      >
        {failed}
        <div className="calibrate-run" data-ui="calibrate.verdict.plan">
          {view.positions.length === 0 ? (
            <p>{t("calibrate.verdict.plan.no_measurements")}</p>
          ) : (
            <>
              {view.running !== "calibrate" && <p>{t(planKey(view), { count: view.positions.length })}</p>}
              <RunControls view={view} actions={actions} blocked={blocked} again={false} />
            </>
          )}
        </div>
      </Card>
    );
  }

  const v = result.verdict;
  const pct = (value: number) => t("calibrate.unit.percent", { value: number(value) });
  const ms = (value: number) => t("calibrate.unit.ms", { value: number(value, 1) });
  // A failed MIMO run says so here too: its notice is in the MIMO card, far below.
  const mimoText = !view.settings.mimo_on
    ? t("calibrate.verdict.mimo.off")
    : view.failure?.job === "mimo"
      ? t("calibrate.verdict.mimo.failed")
      : !mimo
        ? t("calibrate.verdict.mimo.none")
        : mimo.stale
          ? t("frame.status.calibration.stale")
          : t("calibrate.verdict.mimo.done", { kind: mimo.kind.toUpperCase(), count: mimo.support_count });
  // 극저역 보호 that could not hold its boost is a warning with what to do (ADR 0022), not a note.
  const unprotected = lowProtectFailures(result.notes);
  // The engine notes as sentences, for everyone, as upstream shows them; rc_limit is the RC card's line.
  const notes = noteLines(t, number, result.notes.filter((note) => note.code !== "rc_limit" && !unprotected.includes(note)));

  return (
    <Card
      title={t("calibrate.verdict.title")}
      description={resultMode ? t("calibrate.verdict.mode", { mode: modeName(t, number, resultMode.mode, resultMode.ultra_ms) }) : undefined}
      actions={pill}
      stale={result.stale}
      className="calibrate-verdict"
      data-ui="calibrate.verdict"
    >
      {failed}
      {result.stale && (
        <ErrorNotice
          id="calibrate.verdict.stale"
          tone="warning"
          cause={t(STALE_TEXT[view.staleReason ?? "unknown"])}
          next={t("calibrate.stale.next")}
        />
      )}
      {unprotected.length > 0 && (
        <ErrorNotice
          id="calibrate.verdict.low_protect"
          tone="warning"
          cause={unprotected.map((note) => noticeText(t, number, note)).join(" ")}
          next={t(LOW_PROTECT_NEXT)}
        />
      )}
      <div className="calibrate-figures">
        <div className="calibrate-run">
          <dl className="calibrate-stats">
            <Stat id="calibrate.verdict.within" lead label={t("calibrate.verdict.within")} detail={t("calibrate.verdict.within.band")}>
              <Change before={pct(v.within_1db_before_pct)} after={pct(v.within_1db_after_pct)} figure={<Figure template="calibrate.unit.percent" value={number(v.within_1db_after_pct)} />} />
            </Stat>
            <Stat id="calibrate.verdict.gd" label={t("calibrate.verdict.gd")} detail={t("calibrate.verdict.gd.band")}>
              <Change before={number(v.low_band_time_spread_before_ms, 1)} after={ms(v.low_band_time_spread_after_ms)} />
            </Stat>
            <Stat id="calibrate.verdict.spread" label={t("calibrate.verdict.spread")}>
              {view.positions.length > 1 ? t("calibrate.unit.db", { value: number(v.position_spread_db, 1) }) : t("calibrate.verdict.spread.one")}
            </Stat>
            <Stat id="calibrate.verdict.delay" label={t("calibrate.verdict.delay")}>
              {ms(result.latency_ms)}
            </Stat>
            <Stat id="calibrate.verdict.mimo" label={t("calibrate.verdict.mimo")}>
              {mimoText}
            </Stat>
          </dl>
          <RunControls view={view} actions={actions} blocked={blocked} again current={!result.stale} />
        </div>
        {/* With the figures, as upstream shows them beside the total latency; collapsed. */}
        {notes.length > 0 && (
          <details className="ui-details calibrate-notes" data-ui="calibrate.verdict.notes" open={notesOpen || undefined}>
            <summary>{t("calibrate.verdict.notes", { count: notes.length })}</summary>
            <ul>
              {notes.map((line, i) => (
                <li key={i}>{line}</li>
              ))}
            </ul>
          </details>
        )}
      </div>
    </Card>
  );
}

/** A column head: the measure, and its band on a second, smaller line. */
function Head({ label, band }: { label: string; band: string }) {
  return (
    <span className="calibrate-th">
      <span>{label}</span>
      <span className="calibrate-th__band">{band}</span>
    </span>
  );
}

/**
 * One scorecard figure: before → after, then → the MIMO value when the
 * screen shows one. The MIMO value is underlined in the colour of the
 * graph's MIMO curve, and the scorecard's key names it. The screen reader
 * hears "보정 전 …, 보정 후 …, MIMO 후 …" instead of the arrows.
 */
function ScoreChange({ values, digits = 0 }: { values: ChangeValues; digits?: number }) {
  const t = useT();
  const number = useNumber();
  const before = number(values.before, digits);
  const after = number(values.after, digits);
  const mimo = values.mimo === null ? null : number(values.mimo, digits);
  return (
    <>
      <span className="visually-hidden">{mimo === null ? t("calibrate.change.aria", { before, after }) : t("calibrate.change.aria.mimo", { before, after, mimo })}</span>
      <span className="calibrate-score-change" aria-hidden="true">
        <span>{t("calibrate.score.change", { before, after })}</span>
        {mimo !== null && (
          <span className="calibrate-score-change__mimo">
            → <span className="calibrate-mimo">{mimo}</span>
          </span>
        )}
      </span>
    </>
  );
}

type PositionRow = LatePositions["rows"][number];

/**
 * The late bass at every measured position, collapsed at the scorecard's
 * end: does the ringing go down at the other seats too. Columns are the
 * positions in ascending order; the main position, whose figures the
 * scorecard shows, is marked as the reference.
 */
function LateByPosition({ table, main, defaultOpen, onOpen }: { table: LatePositions; main: number | null; defaultOpen: boolean; onOpen: () => void }) {
  const t = useT();
  const details = useRef<HTMLDetailsElement>(null);
  // Opened from the start (gallery, tests): the way a click opens it, once the graph beside the card has
  // its height (it decides the card's), so the layout is the one a user gets. Only on mount.
  useEffect(() => {
    if (!defaultOpen) return;
    const frame = requestAnimationFrame(() => {
      if (!details.current || details.current.open) return;
      onOpen();
      details.current.open = true;
    });
    return () => cancelAnimationFrame(frame);
  }, []);
  const columns: Column<PositionRow>[] = [
    { key: "channel", header: t("calibrate.score.channel"), cell: (r) => <strong className="calibrate-code">{roleCode(r.role)}</strong>, width: "3rem" },
    ...table.positions.map((position, i): Column<PositionRow> => ({
      key: `position.${position}`,
      // The reference mark is read, not a band: on its own line at the table's size.
      header:
        position === main ? (
          <span className="calibrate-th">
            <span>{t("calibrate.score.position", { n: position })}</span>
            <span className="calibrate-th__mark">{t("calibrate.score.position.main")}</span>
          </span>
        ) : (
          t("calibrate.score.position", { n: position })
        ),
      numeric: true,
      cell: (r) => {
        const values = r.cells[i];
        return values ? <ScoreChange values={values} digits={1} /> : null;
      },
    })),
  ];
  return (
    <details ref={details} className="ui-details calibrate-positions" data-ui="calibrate.scorecard.positions">
      {/* A click opens it after this handler: the scorecard keeps its table's height first. */}
      <summary onClick={() => !details.current?.open && onOpen()}>{t("calibrate.score.positions")}</summary>
      <DataTable
        id="calibrate.scorecard.positions.table"
        caption={t("calibrate.score.positions.caption")}
        columns={columns}
        groups={[{ id: "roles", label: "", rows: table.rows }]}
        rowKey={(r) => r.role}
      />
    </details>
  );
}

export function Scorecard({ view, actions, positionsOpen = false }: { view: CalibrateView; actions: CalibrateActions; positionsOpen?: boolean | undefined }) {
  const t = useT();
  // The worst channels first, on request; the configuration order otherwise (it matches the channel list).
  const [worstFirst, setWorstFirst] = useState(false);
  const main = useRef<HTMLDivElement>(null);
  const result = view.results.calibration;
  const model = scorecardModel(view.results, view.lines.channels);
  const tabs = availableTabs(view);
  const tab = tabs.includes(view.tab) ? view.tab : "spl";
  // By the share after correction, as before MIMO existed: MIMO is the comparison, not the ranking.
  const rows = worstFirst ? [...model.rows].sort((a, b) => a.within.after - b.within.after) : model.rows;
  const channel = view.channel ?? view.lines.channels[0]?.role ?? null;
  const columns: Column<ScoreRow>[] = [
    { key: "channel", header: t("calibrate.score.channel"), cell: (r) => <strong className="calibrate-code">{roleCode(r.role)}</strong>, width: "3rem" },
    { key: "within", header: <Head label={t("calibrate.score.within")} band={t("calibrate.verdict.within.band")} />, numeric: true, cell: (r) => <ScoreChange values={r.within} /> },
    { key: "gd", header: <Head label={t("calibrate.score.gd")} band={t("calibrate.verdict.gd.band")} />, numeric: true, cell: (r) => <ScoreChange values={r.gd} digits={1} /> },
  ];
  if (model.late) {
    columns.push({
      key: "late",
      header: <Head label={t("calibrate.score.late")} band={t("calibrate.score.late.band")} />,
      numeric: true,
      cell: (r) => (r.late ? <ScoreChange values={r.late} digits={1} /> : null),
    });
  }
  // The positions table can show MIMO values only for roles the scorecard shows them for, but the key covers both.
  const mimo = model.mimo || !!model.positions?.mimo;
  // Opening the positions lets the card grow below the graph; the channel table keeps the height it has now, so the summary stays under the pointer.
  const keepMainHeight = () => {
    const element = main.current;
    if (element) element.style.setProperty("--calibrate-score-main-h", `${element.getBoundingClientRect().height}px`);
  };
  return (
    <Card
      title={t("calibrate.score.title")}
      dense
      stale={!!result?.stale}
      className={["calibrate-scorecard", model.late && "calibrate-scorecard--late"].filter(Boolean).join(" ")}
      data-ui="calibrate.scorecard.card"
      actions={
        <Button
          variant="quiet"
          size="sm"
          className="calibrate-sort"
          data-ui="calibrate.scorecard.sort"
          aria-pressed={worstFirst}
          onClick={() => setWorstFirst((value) => !value)}
        >
          {t("calibrate.score.sort")}
        </Button>
      }
    >
      <div ref={main} className="calibrate-scorecard__main">
        <DataTable
          id="calibrate.scorecard"
          caption={t(model.mimo ? "calibrate.score.caption.mimo" : "calibrate.score.caption")}
          columns={columns}
          groups={[{ id: "channels", label: "", rows }]}
          rowKey={(r) => r.role}
          selectedKey={tab === "ba" ? null : channel}
          {...(tab === "ba" ? {} : { onSelect: (r: ScoreRow) => actions.selectChannel(r.role, tab === "xo" ? groupOf(view.config, r.role) : null) })}
        />
      </div>
      {(mimo || model.late) && (
        <div className="calibrate-score-key">
          {mimo && (
            <p data-ui="calibrate.scorecard.key">
              {t("calibrate.legend.before")} <span aria-hidden="true">→</span> {t("calibrate.legend.after")} <span aria-hidden="true">→</span>{" "}
              <span className="calibrate-mimo">{t("calibrate.score.mimo")}</span>
            </p>
          )}
          {model.late && <p data-ui="calibrate.scorecard.late">{t("calibrate.score.late.note")}</p>}
        </div>
      )}
      {model.positions && <LateByPosition table={model.positions} main={model.mainPosition} defaultOpen={positionsOpen} onOpen={keepMainHeight} />}
    </Card>
  );
}
