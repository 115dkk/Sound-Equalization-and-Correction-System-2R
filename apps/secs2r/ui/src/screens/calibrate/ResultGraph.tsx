// The result graph (upstream step_calibration.py result card): tabs SPL /
// GD / XO / BA, the channel, the curves exactly as `result_curves` gives
// them, a legend per tab, the RC band on SPL and GD, target editing on SPL
// (drag, double-click adds, right-click or Delete removes, right-click on
// empty space opens the target menu) and the XO line on XO. The BA tab adds
// the alignment lines and table under the graph.
import { useEffect, useMemo, useRef, useState, type MouseEvent, type ReactNode } from "react";
import { useNumber, useT, type Translate } from "../../i18n/react";
import type { ResultCurvesResult } from "../../ipc/bindings/ResultCurvesResult";
import type { ResultTab } from "../../ipc/bindings/ResultTab";
import type { SubResultLines } from "../../ipc/bindings/SubResultLines";
import { AxisRangeButton, Button, Card, DataTable, LogPlot, Menu, Segmented, StatusPill, TargetHandles, XoLine, formatHz, target as targetEdit, type AxisRangeValue, type Column, type Curve, type MenuItem, type PlotScale } from "../../ui";
import { Icon } from "../../ui/Icon";
import { roleCode } from "../../ui/maps/geometry";
import { colorTokens } from "../../ui/plot/series";
import { InlineSelect } from "./controls";
import { filterSpec, groupName, groupOf, noticeText, signedNumber, subModeText, subRoles, xoGroups, type FormatNumber } from "./text";
import type { CalibrateActions, CalibrateView } from "./types";

const PLOT_HEIGHT = 300;
export const TAB_LABEL: Record<ResultTab, "calibrate.tab.spl" | "calibrate.tab.gd" | "calibrate.tab.xo" | "calibrate.tab.ba"> = {
  spl: "calibrate.tab.spl",
  gd: "calibrate.tab.gd",
  xo: "calibrate.tab.xo",
  ba: "calibrate.tab.ba",
};

/** Tabs the project has: BA only with two subs or more (upstream R9). */
export function availableTabs(view: CalibrateView): ResultTab[] {
  const subs = view.lines.ba ? view.lines.ba.rows.length : subRoles(view.config).length;
  return subs >= 2 ? ["spl", "gd", "xo", "ba"] : ["spl", "gd", "xo"];
}

/* ── Curves and legend ──────────────────────────────────────────────────── */

type LegendKey = "before" | "after" | "measured" | "target" | "mimo" | "main" | "sub" | "sum" | "align_before" | "align_after" | "sub_sum";
type LegendItem = { key: LegendKey; color: string; style: "solid" | "dashed" | "dotted" | "bold" };

const LEGEND_TEXT: Record<LegendKey, Parameters<Translate>[0]> = {
  before: "calibrate.legend.before",
  after: "calibrate.legend.after",
  measured: "calibrate.legend.measured",
  target: "calibrate.legend.target",
  mimo: "calibrate.legend.mimo",
  main: "calibrate.legend.main",
  sub: "calibrate.legend.sub",
  sum: "calibrate.legend.sum",
  align_before: "calibrate.legend.align_before",
  align_after: "calibrate.legend.align_after",
  sub_sum: "calibrate.legend.sub_sum",
};

// Colour slots: on SPL and GD "after" takes --ch-1 and "before" --ch-2
// (blue and orange, told apart in every colour-vision test), MIMO --ch-4; on
// XO the main takes --ch-1 and the sub --ch-2; sums are the neutral ink.
const SPL_ORDER = ["after", "before", "_", "mimo"];
const XO_ORDER = ["main", "sub"];
const INK = "var(--plot-text)";

type Drawn = { curves: Curve[]; roleOrder: string[]; isSub: (role: string) => boolean; legend: LegendItem[]; target: boolean };

function drawCurves(result: ResultCurvesResult, tab: ResultTab, computed: boolean, subs: readonly string[]): Drawn {
  const curves: Curve[] = [];
  const kinds = new Set(result.curves.map((c) => c.kind));
  const legend: LegendItem[] = [];
  if (tab === "spl" || tab === "gd") {
    for (const c of result.curves) {
      // The SPL target is drawn by the target handles (it moves while dragged).
      if (c.kind === "target") continue;
      if (c.kind === "before") curves.push({ id: "before", role: "before", kind: computed ? "before" : "main", values: c.values });
      else if (c.kind === "after" || c.kind === "mimo") curves.push({ id: c.kind, role: c.kind, kind: "main", values: c.values });
    }
    if (computed) {
      if (kinds.has("before")) legend.push({ key: "before", color: "var(--ch-2)", style: "dotted" });
      if (kinds.has("after")) legend.push({ key: "after", color: "var(--ch-1)", style: "solid" });
      if (kinds.has("mimo")) legend.push({ key: "mimo", color: "var(--ch-4)", style: "solid" });
    } else if (kinds.has("before")) legend.push({ key: "measured", color: "var(--ch-2)", style: "solid" });
    if (tab === "spl") legend.push({ key: "target", color: "var(--plot-target)", style: "dashed" });
    return { curves, roleOrder: SPL_ORDER, isSub: () => false, legend, target: false };
  }
  if (tab === "xo") {
    for (const c of result.curves) {
      if (c.kind === "target") curves.push({ id: "target", role: "target", kind: "target", values: c.values });
      else if (c.kind === "main" || c.kind === "sub") curves.push({ id: c.kind, role: c.kind, kind: "main", values: c.values });
      else if (c.kind === "sum") curves.push({ id: "sum", role: "sum", kind: "sub", values: c.values });
    }
    if (curves.length) {
      legend.push(
        { key: "main", color: "var(--ch-1)", style: "solid" },
        { key: "sub", color: "var(--ch-2)", style: "solid" },
        { key: "sum", color: INK, style: "bold" },
        { key: "target", color: "var(--plot-target)", style: "dashed" },
      );
    }
    return { curves, roleOrder: XO_ORDER, isSub: () => false, legend, target: false };
  }
  // BA: per sub before (thin, dotted) and after, the sub sum before and after, the target.
  for (const c of result.curves) {
    if (c.kind === "target") curves.push({ id: "target", role: "target", kind: "target", values: c.values });
    else if (c.kind === "before" && c.role) curves.push({ id: `before.${c.role}`, role: c.role, kind: "before", position: 2, values: c.values });
    else if (c.kind === "after" && c.role) curves.push({ id: `after.${c.role}`, role: c.role, kind: "main", values: c.values });
    else if (c.kind === "sum_before") curves.push({ id: "sum_before", role: "sum", kind: "before", values: c.values });
    else if (c.kind === "sum_after") curves.push({ id: "sum_after", role: "sum", kind: "sub", values: c.values });
  }
  if (curves.length) {
    legend.push(
      { key: "align_before", color: INK, style: "dotted" },
      { key: "align_after", color: INK, style: "solid" },
      { key: "sub_sum", color: INK, style: "bold" },
      { key: "target", color: "var(--plot-target)", style: "dashed" },
    );
  }
  const subSet = new Set(subs);
  return { curves, roleOrder: [...subs], isSub: (role) => subSet.has(role), legend, target: true };
}

function ResultLegend({ items }: { items: readonly LegendItem[] }) {
  const t = useT();
  if (!items.length) return null;
  return (
    <ul className="calibrate-legend" data-ui="calibrate.plot.legend" aria-label={t("calibrate.legend.label")}>
      {items.map((item) => (
        <li key={item.key} className="calibrate-legend__item" data-ui={`calibrate.plot.legend.${item.key}`}>
          <span className={`calibrate-swatch calibrate-swatch--${item.style}`} style={{ borderTopColor: item.color }} aria-hidden="true" />
          {t(LEGEND_TEXT[item.key])}
        </li>
      ))}
    </ul>
  );
}

/* ── Target menu ────────────────────────────────────────────────────────── */

const PRESET_TEXT: Readonly<Record<string, "calibrate.target.preset.flat" | "calibrate.target.preset.harman" | "calibrate.target.preset.cinema">> = {
  flat: "calibrate.target.preset.flat",
  harman: "calibrate.target.preset.harman",
  cinema: "calibrate.target.preset.cinema",
};

export function presetName(t: Translate, preset: string | null): string {
  if (preset === null) return t("calibrate.target.custom");
  const key = PRESET_TEXT[preset];
  return key ? t(key) : preset;
}

export function targetMenuItems(t: Translate, view: Pick<CalibrateView, "target">, actions: Pick<CalibrateActions, "applyTargetPreset" | "saveTarget" | "loadTarget">): MenuItem[] {
  const current = view.target.preset;
  return [
    ...view.target.presets.map(([name]) => ({
      id: `calibrate.target.preset.${name}`,
      label: presetName(t, name),
      ...(current === name ? { icon: "check" as const } : {}),
      onSelect: () => actions.applyTargetPreset(name),
    })),
    // An indicator, not a command: the target becomes 사용자 지정 by editing it.
    { id: "calibrate.target.custom", label: t("calibrate.target.custom"), ...(current === null ? { icon: "check" as const } : {}), disabled: true, onSelect: () => {} },
    { id: "calibrate.target.save", label: t("calibrate.target.save"), icon: "save", onSelect: actions.saveTarget },
    { id: "calibrate.target.load", label: t("calibrate.target.load"), icon: "folder", onSelect: actions.loadTarget },
  ];
}

/* ── BA lines ───────────────────────────────────────────────────────────── */

function BaLines({ ba, subs, number }: { ba: SubResultLines; subs: readonly string[]; number: FormatNumber }) {
  const t = useT();
  const tokens = colorTokens(subs, () => true);
  const columns: Column<SubResultLines["rows"][number]>[] = [
    {
      key: "sub",
      header: t("calibrate.ba.col.sub"),
      cell: (r) => (
        <span className="calibrate-sub-name">
          <span className="calibrate-swatch calibrate-swatch--solid" style={{ borderTopColor: `var(${tokens.get(r.role) ?? "--text-muted"})` }} aria-hidden="true" />
          {roleCode(r.role)}
        </span>
      ),
    },
    { key: "mode", header: t("calibrate.ba.col.mode"), cell: (r) => subModeText(t, r.mode) },
    { key: "gain", header: t("calibrate.ba.col.gain"), numeric: true, cell: (r) => t("calibrate.unit.db", { value: signedNumber(number, r.gain_db, 1) }) },
    { key: "delay", header: t("calibrate.ba.col.delay"), numeric: true, cell: (r) => t("calibrate.unit.ms", { value: number(r.delay_ms, 2) }) },
    { key: "polarity", header: t("calibrate.ba.col.polarity"), cell: (r) => t(r.polarity < 0 ? "calibrate.polarity.inverted" : "calibrate.polarity.normal") },
    {
      key: "allpass",
      header: t("calibrate.ba.col.allpass"),
      cell: (r) =>
        r.allpass.length ? r.allpass.map(([hz, q]) => t("calibrate.spec.allpass", { hz: number(hz), q: number(q, 1) })).join(", ") : t("calibrate.ba.allpass.none"),
    },
  ];
  const fixed = ba.rows
    .map((r) => ({ role: r.role, spec: filterSpec(t, number, r.high_pass, r.low_pass, r.peq) }))
    .filter((r) => r.spec);
  const layouts = ba.layouts.map((l) =>
    l.kind === "dba"
      ? t(l.auto_ms === null ? "calibrate.ba.layout_line.dba_manual" : "calibrate.ba.layout_line.dba", {
          bundle: l.bundle,
          front: l.front.map(roleCode).join("·"),
          rear: l.rear.map(roleCode).join("·"),
          rear_ms: signedNumber(number, l.rear_ms, 2),
          auto_ms: number(l.auto_ms ?? 0, 2),
          trim_ms: signedNumber(number, l.trim_ms, 2),
          peq: l.peq.length,
        })
      : t("calibrate.ba.layout_line.multi", { bundle: l.bundle, members: l.members.map(roleCode).join("·"), peq: l.peq.length }),
  );
  const warnings = ba.warnings.map((w) => noticeText(t, number, w) ?? `${w.code} ${JSON.stringify(w.params)}`);
  return (
    <div className="calibrate-ba" data-ui="calibrate.ba.result">
      <p className="calibrate-line tabular" data-ui="calibrate.ba.metrics">
        {t("calibrate.ba.flatness", {
          lo: number(ba.band_hz[0]),
          hi: number(ba.band_hz[1]),
          before: number(ba.flatness_db[0], 1),
          after: number(ba.flatness_db[1], 1),
        })}
        <span className="calibrate-line__sep" aria-hidden="true">
          ·
        </span>
        {ba.seat_spread_db
          ? t("calibrate.ba.seat_spread", { before: number(ba.seat_spread_db[0], 1), after: number(ba.seat_spread_db[1], 1) })
          : t("calibrate.ba.seat_spread.one")}
      </p>
      <DataTable
        id="calibrate.ba.table"
        caption={t("calibrate.ba.table")}
        columns={columns}
        groups={[{ id: "subs", label: "", rows: ba.rows }]}
        rowKey={(r) => r.role}
      />
      {(layouts.length > 0 || fixed.length > 0) && (
        <div className="calibrate-lines" data-ui="calibrate.ba.filters">
          {layouts.map((line) => (
            <p key={line} className="calibrate-line tabular">
              {line}
            </p>
          ))}
          {fixed.length > 0 && (
            <p className="calibrate-line tabular">{t("calibrate.ba.fixed_filters", { list: fixed.map((f) => `${roleCode(f.role)} ${f.spec}`).join(" · ") })}</p>
          )}
        </div>
      )}
      {warnings.length > 0 && (
        <ul className="calibrate-warnings" data-ui="calibrate.ba.warnings">
          {warnings.map((w) => (
            <li key={w}>
              <Icon name="warning" size={16} />
              <span>{w}</span>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}

/* ── The graph card ─────────────────────────────────────────────────────── */

type TargetPoint = targetEdit.TargetPoint;

export function ResultGraph({ view, actions, menuOpen = false }: { view: CalibrateView; actions: CalibrateActions; menuOpen?: boolean | undefined }) {
  const t = useT();
  const number = useNumber();
  const tabs = availableTabs(view);
  const tab: ResultTab = tabs.includes(view.tab) ? view.tab : "spl";
  const computed = view.results.calibration !== null;
  const subs = useMemo(() => subRoles(view.config), [view.config]);
  const groups = useMemo(() => xoGroups(view.config), [view.config]);
  const curves = view.curves && view.curves.tab === tab ? view.curves : null;
  const drawn = useMemo(() => (curves ? drawCurves(curves, tab, computed, subs) : null), [curves, tab, computed, subs]);

  // Channel list: the result's channels, or before a run the roles measured at position 1.
  const channels = computed ? view.lines.channels.map((c) => c.role) : view.measuredRoles;
  const channel = view.channel && channels.includes(view.channel) ? view.channel : (channels[0] ?? null);
  const xoGroup = view.xoGroup && groups.includes(view.xoGroup) ? view.xoGroup : (groups[0] ?? null);

  // Axis ranges the user picked, per tab, for this visit.
  const [ranges, setRanges] = useState<Partial<Record<ResultTab, AxisRangeValue>>>({});
  const range = ranges[tab] ?? { x: null, y: null };
  const xRange = curves?.axes.frequency_hz ?? ([5, 20000] as [number, number]);
  const yRange = curves?.axes.value ?? ([-30, 15] as [number, number]);
  const unit = curves?.axes.unit === "ms" ? "ms" : "dB";

  // Target points while dragging stay local; a release (or a key, a removal, a double-click) stores them.
  const [draft, setDraft] = useState<TargetPoint[] | null>(null);
  const dragging = useRef(false);
  useEffect(() => setDraft(null), [view.target]);
  const points: TargetPoint[] = draft ?? view.target.points.map(([hz, db]) => ({ hz, db }));
  const commit = (next: TargetPoint[]) => {
    setDraft(next);
    actions.setTargetPoints(next.map((p) => [p.hz, p.db]));
  };

  // The XO line follows the pointer; curves follow through previewXo; release stores the value.
  const [dragHz, setDragHz] = useState<number | null>(null);
  const xoHz = dragHz ?? curves?.xo_hz ?? (xoGroup ? (view.settings.xo[xoGroup] ?? null) : null);
  useEffect(() => setDragHz(null), [view.settings.xo]);

  // Target menu: from the toolbar button or a right-click on empty graph space (SPL only).
  const [menu, setMenu] = useState<"button" | "context" | null>(menuOpen ? "button" : null);
  const [at, setAt] = useState({ x: 0, y: 0 });
  const buttonRef = useRef<HTMLButtonElement>(null);
  const contextRef = useRef<HTMLSpanElement>(null);
  const stageRef = useRef<HTMLDivElement>(null);
  const onContextMenu = (event: MouseEvent<HTMLDivElement>) => {
    if (tab !== "spl") return;
    const target = event.target as Element;
    if (!target.closest(".u-over") || target.closest(".plot-handle")) return;
    event.preventDefault();
    const box = stageRef.current?.getBoundingClientRect();
    setAt({ x: event.clientX - (box?.left ?? 0), y: event.clientY - (box?.top ?? 0) });
    setMenu("context");
  };

  const selected = computed ? view.lines.channels.find((c) => c.role === channel) : undefined;
  const band = curves?.rc_band_hz ?? null;

  const overlay = (scale: PlotScale): ReactNode => (
    <>
      {band && (tab === "spl" || tab === "gd") && (
        <>
          {band[1] === null && (
            <span className="calibrate-band-shade" style={{ width: Math.max(0, scale.xToPx(band[0])) }} aria-hidden="true" />
          )}
          <span className="plot-band-label" data-ui="calibrate.plot.band.lower" style={{ left: scale.xToPx(band[0]) + 4 }}>
            {t("plot.band.lower", { hz: formatHz(band[0]) })}
          </span>
          {band[1] !== null && (
            <span className="plot-band-label calibrate-band-label--upper" data-ui="calibrate.plot.band.upper" style={{ left: scale.xToPx(band[1]) + 4 }}>
              {t("plot.band.upper", { hz: formatHz(band[1]) })}
            </span>
          )}
        </>
      )}
      {tab === "spl" && (
        <div
          className="calibrate-layer"
          onPointerDownCapture={(event) => {
            if (event.button === 0 && (event.target as Element).closest(".plot-handle")) dragging.current = true;
          }}
          onPointerUp={() => {
            if (!dragging.current) return;
            dragging.current = false;
            if (draft) commit(draft);
          }}
        >
          <TargetHandles
            id="calibrate.target"
            points={points}
            scale={scale}
            onChange={(next) => (dragging.current ? setDraft(next) : commit(next))}
          />
        </div>
      )}
      {tab === "xo" && xoGroup && xoHz !== null && curves && curves.curves.length > 0 && (
        <div
          className="calibrate-layer"
          onPointerUp={() => {
            if (dragHz !== null) actions.changeSetting({ kind: "xo", group: xoGroup, hz: dragHz });
          }}
          onKeyUp={() => {
            if (dragHz !== null) actions.changeSetting({ kind: "xo", group: xoGroup, hz: dragHz });
          }}
        >
          <XoLine
            id="calibrate.xo.line"
            hz={xoHz}
            scale={scale}
            onChange={(hz) => {
              setDragHz(hz);
              actions.previewXo(xoGroup, hz);
            }}
          />
        </div>
      )}
    </>
  );

  const toolbar = (
    <>
      {tab === "spl" && (
        <Button
          ref={buttonRef}
          variant="quiet"
          size="sm"
          iconEnd="chevronDown"
          data-ui="calibrate.target.menu"
          aria-haspopup="menu"
          aria-expanded={menu !== null}
          onClick={() => setMenu((m) => (m ? null : "button"))}
        >
          {t("calibrate.target.button", { name: presetName(t, view.target.preset) })}
        </Button>
      )}
      <AxisRangeButton
        id="calibrate.plot.axes"
        value={range}
        defaults={{ x: xRange, y: yRange }}
        yUnit={unit}
        onChange={(value) => setRanges((r) => ({ ...r, [tab]: value }))}
      />
    </>
  );

  const hint = (): string | null => {
    if (computed || !curves) return null;
    if (tab === "spl") return curves.curves.length ? t("calibrate.plot.preview") : t("calibrate.plot.preview.none");
    return t(tab === "gd" ? "calibrate.plot.pending.gd" : tab === "xo" ? "calibrate.plot.pending.xo" : "calibrate.plot.pending.ba");
  };

  return (
    <Card dense stale={!!curves?.stale} className="calibrate-graph" aria-label={t("calibrate.graph.label")} data-ui="calibrate.graph">
      <div className="calibrate-graph__bar">
        <Segmented
          id="calibrate.tab"
          label={t("calibrate.graph.tabs")}
          hideLabel
          value={tab}
          options={tabs.map((value) => ({ value, label: t(TAB_LABEL[value]) }))}
          onChange={actions.selectTab}
        />
        <InlineSelect
          id="calibrate.channel"
          label={t("calibrate.graph.channel")}
          value={channel ?? ""}
          disabled={tab === "ba" || channels.length === 0}
          options={channels.map((role) => ({ value: role, label: roleCode(role) }))}
          onChange={(role) => actions.selectChannel(role, tab === "xo" ? groupOf(view.config, role) : null)}
        />
        {tab === "xo" && xoGroup && (
          <span className="calibrate-graph__group muted" data-ui="calibrate.graph.group">
            {t("calibrate.graph.group", { group: groupName(t, xoGroup) })}
          </span>
        )}
        {curves?.subs_only && tab === "ba" && (
          <StatusPill tone="info" data-ui="calibrate.ba.subs_only">
            {t("calibrate.ba.subs_only")}
          </StatusPill>
        )}
      </div>
      <div className="calibrate-graph__stage" ref={stageRef} onContextMenu={onContextMenu}>
        {curves && drawn ? (
          <LogPlot
            id="calibrate.plot"
            label={t("calibrate.graph.plot", { tab: t(TAB_LABEL[tab]) })}
            freqs={curves.grid}
            curves={drawn.curves}
            roleOrder={drawn.roleOrder}
            isSub={drawn.isSub}
            xRange={range.x ?? xRange}
            yRange={range.y ?? yRange}
            yUnit={unit}
            height={PLOT_HEIGHT}
            band={band && band[1] !== null && (tab === "spl" || tab === "gd") ? { lo: band[0], hi: band[1] } : null}
            stale={curves.stale}
            onDoubleClick={tab === "spl" ? ({ hz, value }) => commit(targetEdit.addPoint(points, { hz, db: value }).points) : undefined}
            overlay={overlay}
            toolbar={toolbar}
          />
        ) : (
          <div className="calibrate-graph__loading" style={{ height: PLOT_HEIGHT + 28 }} aria-hidden="true" />
        )}
        <span ref={contextRef} className="calibrate-context-anchor" style={{ left: at.x, top: at.y }} aria-hidden="true" />
        <div className={menuOpen ? "calibrate-menu-inline" : undefined}>
          <Menu
            open={menu !== null}
            onClose={() => setMenu(null)}
            anchor={menu === "context" ? contextRef : buttonRef}
            placement={menu === "context" ? "bottom-start" : "bottom-end"}
            label={t("calibrate.target.menu")}
            id="calibrate.target.menu.list"
            items={targetMenuItems(t, view, actions)}
            inline={menuOpen}
          />
        </div>
      </div>
      {drawn && <ResultLegend items={drawn.legend} />}
      {hint() && (
        <p className="calibrate-hint" data-ui="calibrate.plot.preview">
          {hint()}
        </p>
      )}
      {selected && (tab === "spl" || tab === "gd") && (
        <p className="calibrate-line tabular" data-ui="calibrate.plot.filters">
          {[
            t("calibrate.filters.channel", { code: roleCode(selected.role) }),
            t("calibrate.filters.peq", { count: selected.peq_count }),
            selected.fir_taps ? t("calibrate.filters.fir", { taps: number(selected.fir_taps) }) : t("calibrate.filters.no_fir"),
            selected.xo_hz ? t("calibrate.filters.xo", { hz: number(selected.xo_hz) }) : null,
            selected.mimo_support_count !== null ? t("calibrate.filters.mimo", { count: selected.mimo_support_count }) : null,
          ]
            .filter(Boolean)
            .join(" · ")}
        </p>
      )}
      {computed && tab === "xo" && xoGroup && xoHz !== null && (
        <p className="calibrate-line tabular" data-ui="calibrate.plot.xo">
          {t("calibrate.graph.xo_line", { group: groupName(t, xoGroup), hz: number(xoHz) })}
        </p>
      )}
      {tab === "ba" && view.lines.ba && <BaLines ba={view.lines.ba} subs={subs} number={number} />}
    </Card>
  );
}
