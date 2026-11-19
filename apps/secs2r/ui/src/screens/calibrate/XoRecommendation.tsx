// The crossover card's recommendation: right after 'XO 추천 계산', the
// comparison it ran (one row per group, one column per candidate, each
// score as a number and as a bar scaled within its row); after the project
// is reopened, only the recommended value of each group is left, a button
// where the crossover differs and a check where it is already set. The
// recommended cell carries the accent and the word 추천, the current one is
// pressed and carries a check (with 현재 when it is not the recommended one),
// so a group already on its recommendation shows one cell, not two marks.
// Every cell sets its group's crossover to that candidate, and 추천값 적용
// sets every group that differs at once (both through the manual `xo` edit).
import type { ReactNode } from "react";
import { useNumber, useT } from "../../i18n/react";
import type { XoCandidateScore } from "../../ipc/bindings/XoCandidateScore";
import { Button } from "../../ui";
import { Icon } from "../../ui/Icon";
import { Tooltip } from "../../ui/Tooltip";
import { xoCurrent, xoDiffering } from "./settingsView";
import { groupName } from "./text";
import type { CalibrateActions, CalibrateView } from "./types";

type Props = {
  view: CalibrateView;
  actions: CalibrateActions;
  /** The crossover groups in configuration order. */
  groups: readonly string[];
  /** The group chosen in the editor above (its row is highlighted). */
  selected: string;
};

/** Where a bar ends: the row's lowest score still draws a sliver, its highest the full width. */
function fill(score: number, low: number, high: number): number {
  return high > low ? 12 + (88 * (score - low)) / (high - low) : 100;
}

function Mark({ best, current }: { best: boolean; current: boolean }) {
  const t = useT();
  if (!best && !current) return null;
  return (
    <span className="calibrate-xo-rec__mark">
      {current && <Icon name="check" size={14} />}
      {best ? t("calibrate.xo.rec.best") : t("calibrate.xo.rec.current")}
    </span>
  );
}

export function XoRecommendation({ view, actions, groups, selected }: Props) {
  const t = useT();
  const s = view.settings;
  const shown = groups.filter((group) => s.xo_rec[group] !== undefined);
  if (shown.length === 0 || view.running === "recommend_xo") return null;
  const differing = xoDiffering(s, groups);
  const busy = view.running !== null;
  const recommendation = view.xoRecommendation;
  return (
    <div className={["calibrate-xo-rec", view.results.xo_rec_stale && "calibrate-xo-rec--stale"].filter(Boolean).join(" ")}>
      {recommendation && recommendation.table.length > 0 ? (
        <ScoreTable view={view} actions={actions} groups={shown} selected={selected} busy={busy} />
      ) : (
        <ValueTable view={view} actions={actions} groups={shown} selected={selected} busy={busy} />
      )}
      {differing.length > 0 && (
        <div className="calibrate-actions">
          <Button data-ui="calibrate.xo.rec.apply" disabled={busy} onClick={() => void actions.applyXoRecommendation()}>
            {t("calibrate.xo.rec.apply")}
          </Button>
        </div>
      )}
    </div>
  );
}

type TableProps = { view: CalibrateView; actions: CalibrateActions; groups: readonly string[]; selected: string; busy: boolean };

function Row({ group, selected, children }: { group: string; selected: boolean; children: ReactNode }) {
  const t = useT();
  return (
    <tr className={selected ? "calibrate-xo-rec__row--selected" : undefined} data-group={group}>
      <th scope="row">{groupName(t, group)}</th>
      {children}
    </tr>
  );
}

/** Right after the job: every candidate's score, each cell a button that sets that value. */
function ScoreTable({ view, actions, groups, selected, busy }: TableProps) {
  const t = useT();
  const number = useNumber();
  const s = view.settings;
  const recommendation = view.xoRecommendation!;
  const candidates = recommendation.candidates;
  const best = new Map(recommendation.best);
  const rows = groups.flatMap((group) => {
    const scores = recommendation.table.find((entry) => entry.group === group)?.rows;
    return scores ? [{ group, scores }] : [];
  });
  // A crossover typed in between the candidates (90 Hz) is named at the row's end.
  const offGrid = rows.some(({ group }) => !candidates.includes(xoCurrent(s, group)));
  const cell = (group: string, row: XoCandidateScore, low: number, high: number) => {
    const isBest = (best.get(group) ?? s.xo_rec[group]) === row.hz;
    const isCurrent = xoCurrent(s, group) === row.hz;
    const name = [
      t("calibrate.xo.rec.cell", { group: groupName(t, group), hz: number(row.hz), score: number(row.score, 1) }),
      isBest && t("calibrate.xo.rec.best"),
      isCurrent && t("calibrate.xo.rec.current"),
    ].filter(Boolean).join(", ");
    const metrics = (
      <span className="calibrate-xo-rec__metrics tabular">
        <span>{t("calibrate.xo.rec.flat", { value: number(row.flat_pct, 1) })}</span>
        <span>{t("calibrate.xo.rec.gd", { value: number(row.gd_ms, 1) })}</span>
        <span>{t("calibrate.xo.rec.late", { value: number(row.late_db, 1) })}</span>
      </span>
    );
    return (
      <td key={row.hz}>
        <Tooltip content={metrics}>
          <button
            type="button"
            className={["calibrate-xo-rec__cell", isBest && "calibrate-xo-rec__cell--best"].filter(Boolean).join(" ")}
            data-ui={`calibrate.xo.rec.${group}.${row.hz}`}
            aria-label={name}
            aria-pressed={isCurrent}
            disabled={busy}
            onClick={() => void actions.changeSetting({ kind: "xo", group, hz: row.hz })}
          >
            <span className="calibrate-xo-rec__score tabular">{number(row.score, 1)}</span>
            <Mark best={isBest} current={isCurrent} />
            <span className="calibrate-xo-rec__bar" aria-hidden="true">
              <span className="calibrate-xo-rec__fill" style={{ width: `${fill(row.score, low, high)}%` }} />
            </span>
          </button>
        </Tooltip>
      </td>
    );
  };
  return (
    <table className="calibrate-xo-rec__table" data-ui="calibrate.xo.rec">
      <caption>
        <span className="calibrate-xo-rec__caption">{t("calibrate.xo.rec.caption")}</span>
        <span className="calibrate-xo-rec__hint">{t("calibrate.xo.rec.hint")}</span>
      </caption>
      <thead>
        <tr>
          <th scope="col">{t("calibrate.xo.group")}</th>
          {candidates.map((hz) => (
            <th key={hz} scope="col" className="tabular">
              {t("calibrate.xo.rec.hz", { hz: number(hz) })}
            </th>
          ))}
          {offGrid && <th scope="col">{t("calibrate.xo.rec.current")}</th>}
        </tr>
      </thead>
      <tbody>
        {rows.map(({ group, scores }) => {
          const values = scores.map((row) => row.score);
          const low = Math.min(...values);
          const high = Math.max(...values);
          const current = xoCurrent(s, group);
          return (
            <Row key={group} group={group} selected={group === selected}>
              {candidates.map((hz) => {
                const row = scores.find((entry) => entry.hz === hz);
                return row ? cell(group, row, low, high) : <td key={hz} />;
              })}
              {offGrid && (
                <td className="calibrate-xo-rec__off tabular">
                  {candidates.includes(current) ? null : (
                    <span className="calibrate-xo-rec__mark">
                      <Icon name="check" size={14} />
                      {t("calibrate.xo.rec.hz", { hz: number(current) })}
                    </span>
                  )}
                </td>
              )}
            </Row>
          );
        })}
      </tbody>
    </table>
  );
}

/** After reopening a project: the recommended value per group, a button where the crossover differs. */
function ValueTable({ view, actions, groups, selected, busy }: TableProps) {
  const t = useT();
  const number = useNumber();
  const s = view.settings;
  const differs = xoDiffering(s, groups);
  return (
    <table className="calibrate-xo-rec__table calibrate-xo-rec__table--values" data-ui="calibrate.xo.rec">
      <caption>
        <span className="calibrate-xo-rec__caption">{t("calibrate.xo.rec.caption.values")}</span>
      </caption>
      <thead>
        <tr>
          <th scope="col">{t("calibrate.xo.group")}</th>
          <th scope="col">{t("calibrate.xo.rec.best")}</th>
          {differs.length > 0 && <th scope="col">{t("calibrate.xo.rec.current")}</th>}
        </tr>
      </thead>
      <tbody>
        {groups.map((group) => {
          const rec = s.xo_rec[group]!;
          const current = xoCurrent(s, group);
          const applied = current === rec;
          const value = t("calibrate.xo.rec.hz", { hz: number(rec) });
          return (
            <Row key={group} group={group} selected={group === selected}>
              <td>
                {applied ? (
                  <span className="calibrate-xo-rec__cell calibrate-xo-rec__cell--best calibrate-xo-rec__cell--static">
                    <span className="calibrate-xo-rec__score tabular">{value}</span>
                    <Mark best={false} current />
                  </span>
                ) : (
                  <button
                    type="button"
                    className="calibrate-xo-rec__cell calibrate-xo-rec__cell--best"
                    data-ui={`calibrate.xo.rec.${group}.${rec}`}
                    aria-label={[t("calibrate.xo.rec.value", { group: groupName(t, group), hz: number(rec) }), t("calibrate.xo.rec.best")].join(", ")}
                    aria-pressed={false}
                    disabled={busy}
                    onClick={() => void actions.changeSetting({ kind: "xo", group, hz: rec })}
                  >
                    <span className="calibrate-xo-rec__score tabular">{value}</span>
                  </button>
                )}
              </td>
              {differs.length > 0 && (
                <td className="calibrate-xo-rec__off tabular">
                  {applied ? null : (
                    <span className="calibrate-xo-rec__mark">
                      <Icon name="check" size={14} />
                      {t("calibrate.xo.rec.hz", { hz: number(current) })}
                    </span>
                  )}
                </td>
              )}
            </Row>
          );
        })}
      </tbody>
    </table>
  );
}
