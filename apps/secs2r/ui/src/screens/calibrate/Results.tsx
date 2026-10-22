// What the correction achieved, first (ARCHITECTURE §7): the verdict card
// (is it good, is it current) and the per-channel scorecard whose rows pick
// the channel the graph shows.
import { useState, type ReactNode } from "react";
import { errorText } from "../../app/jobs/jobText";
import { useNumber, useT } from "../../i18n/react";
import type { ChannelResultLine } from "../../ipc/bindings/ChannelResultLine";
import { Button, Card, DataTable, ErrorNotice, StatusPill, type Column } from "../../ui";
import { roleCode } from "../../ui/maps/geometry";
import { Stat } from "./controls";
import { availableTabs } from "./ResultGraph";
import { FAILED_TEXT, groupOf, modeName, noteLines } from "./text";
import type { CalibrateActions, CalibrateView, StaleReason } from "./types";

const STALE_TEXT: Record<StaleReason | "unknown", "calibrate.stale.settings" | "calibrate.stale.target" | "calibrate.stale.measurements" | "calibrate.stale.unknown"> = {
  settings: "calibrate.stale.settings",
  target: "calibrate.stale.target",
  measurements: "calibrate.stale.measurements",
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

export function Verdict({ view, actions, onSampleRateSettings }: { view: CalibrateView; actions: CalibrateActions; onSampleRateSettings: () => void }) {
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
        <div className="calibrate-plan" data-ui="calibrate.verdict.plan">
          <p>
            {view.positions.length > 0
              ? t(view.positions.length === 1 ? "calibrate.verdict.plan.one" : "calibrate.verdict.plan", { count: view.positions.length })
              : t("calibrate.verdict.plan.no_measurements")}
          </p>
        </div>
      </Card>
    );
  }

  const v = result.verdict;
  const pct = (value: number) => t("calibrate.unit.percent", { value: number(value) });
  const ms = (value: number) => t("calibrate.unit.ms", { value: number(value, 1) });
  const mimoText = !view.settings.mimo_on
    ? t("calibrate.verdict.mimo.off")
    : !mimo
      ? t("calibrate.verdict.mimo.none")
      : mimo.stale
        ? t("frame.status.calibration.stale")
        : t("calibrate.verdict.mimo.done", { kind: mimo.kind.toUpperCase(), count: mimo.support_count });
  // The engine notes as sentences, for everyone, as upstream shows them; rc_limit is the RC card's line.
  const notes = noteLines(t, number, result.notes.filter((note) => note.code !== "rc_limit"));

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
      <div className="calibrate-figures">
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
        {/* With the figures, as upstream shows them beside the total latency; collapsed. */}
        {notes.length > 0 && (
          <details className="ui-details calibrate-notes" data-ui="calibrate.verdict.notes">
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

export function Scorecard({ view, actions }: { view: CalibrateView; actions: CalibrateActions }) {
  const t = useT();
  const number = useNumber();
  // The worst channels first, on request; the configuration order otherwise (it matches the channel list).
  const [worstFirst, setWorstFirst] = useState(false);
  const result = view.results.calibration;
  const tabs = availableTabs(view);
  const tab = tabs.includes(view.tab) ? view.tab : "spl";
  const rows = worstFirst ? [...view.lines.channels].sort((a, b) => a.within_1db_pct[1] - b.within_1db_pct[1]) : view.lines.channels;
  const channel = view.channel ?? view.lines.channels[0]?.role ?? null;
  const head = (label: string, band: string) => (
    <span className="calibrate-th">
      <span>{label}</span>
      <span className="calibrate-th__band">{band}</span>
    </span>
  );
  const columns: Column<ChannelResultLine>[] = [
    { key: "channel", header: t("calibrate.score.channel"), cell: (r) => <strong className="calibrate-code">{roleCode(r.role)}</strong>, width: "3.5rem" },
    {
      key: "within",
      header: head(t("calibrate.score.within"), t("calibrate.verdict.within.band")),
      numeric: true,
      cell: (r) => t("calibrate.score.change", { before: number(r.within_1db_pct[0]), after: number(r.within_1db_pct[1]) }),
    },
    {
      key: "gd",
      header: head(t("calibrate.score.gd"), t("calibrate.verdict.gd.band")),
      numeric: true,
      cell: (r) => t("calibrate.score.change", { before: number(r.gd_spread_ms[0], 1), after: number(r.gd_spread_ms[1], 1) }),
    },
  ];
  return (
    <Card
      title={t("calibrate.score.title")}
      dense
      stale={!!result?.stale}
      className="calibrate-scorecard"
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
      <DataTable
        id="calibrate.scorecard"
        caption={t("calibrate.score.caption")}
        columns={columns}
        groups={[{ id: "channels", label: "", rows }]}
        rowKey={(r) => r.role}
        selectedKey={tab === "ba" ? null : channel}
        {...(tab === "ba" ? {} : { onSelect: (r: ChannelResultLine) => actions.selectChannel(r.role, tab === "xo" ? groupOf(view.config, r.role) : null) })}
      />
    </Card>
  );
}
