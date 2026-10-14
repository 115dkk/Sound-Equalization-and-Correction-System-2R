// Graphs and maps: measurement curves with legend and position chips, the
// highlight, target editing with the RC band, the XO line, the axis range
// popover, a dense stress case, and the three maps.
import { useLayoutEffect, useMemo, useRef, useState } from "react";
import { useNumber, useT } from "../i18n/react";
import { SAMPLE_ROLES } from "../ipc/fixtures";
import {
  ArrangementMap,
  ArrangementThumb,
  AxisRangeButton,
  Card,
  Graph,
  LogPlot,
  RoomMap,
  TargetHandles,
  XoLine,
  formatHz,
  target as targetEdit,
  type Curve,
} from "../ui";
import { PRESET_ROLES, isSubRole, roleCode } from "../ui/maps/geometry";
import { logGrid, measurementCurves, response } from "./data";
import type { SpecimenDef } from "./Specimen";

const ROLES = [...SAMPLE_ROLES];
const CHANNELS = ROLES.map((role) => ({ role, label: roleCode(role), sub: isSubRole(role) }));

function MeasurementGraph({ positions = 3, highlight }: { positions?: number; highlight?: { role?: string; position?: number } }) {
  const t = useT();
  const freqs = useMemo(() => logGrid(5, 22000, 280), []);
  const curves = useMemo(() => measurementCurves(freqs, ROLES, positions, isSubRole), [freqs, positions]);
  return (
    <Graph
      id={highlight ? "gallery.plot.highlight" : "gallery.plot.measure"}
      label={t("plot.measurement.title")}
      freqs={freqs}
      curves={curves}
      channels={CHANNELS}
      roleOrder={ROLES}
      positions={Array.from({ length: positions }, (_, i) => i + 1)}
      yRange={[-35, 20]}
      initialHighlight={highlight ?? null}
    />
  );
}

function TargetGraph() {
  const t = useT();
  const number = useNumber();
  const freqs = useMemo(() => logGrid(5, 20000, 400), []);
  const [points, setPoints] = useState<targetEdit.TargetPoint[]>([
    { hz: 10, db: 6 },
    { hz: 30, db: 6 },
    { hz: 120, db: 2 },
    { hz: 1000, db: 0 },
    { hz: 20000, db: -4 },
  ]);
  const curves: Curve[] = useMemo(() => [{ id: "L-before", role: "L", kind: "before", position: 1, values: response(freqs, "L", 1, false) }], [freqs]);
  const band = { lo: 20, hi: 310 };
  return (
    <div className="gal-stack">
      <LogPlot
        id="gallery.plot.target"
        label={t("plot.target.title")}
        freqs={freqs}
        curves={curves}
        roleOrder={ROLES}
        isSub={isSubRole}
        xRange={[5, 20000]}
        yRange={[-30, 15]}
        band={band}
        height={300}
        onDoubleClick={({ hz, value }) => setPoints((p) => targetEdit.addPoint(p, { hz, db: value }).points)}
        overlay={(scale) => (
          <>
            <span className="plot-band-label" style={{ left: scale.xToPx(band.lo) + 4 }}>
              {t("plot.band.lower", { hz: formatHz(band.lo) })}
            </span>
            <span className="plot-band-label" style={{ left: scale.xToPx(band.hi) + 4 }}>
              {t("plot.band.upper", { hz: formatHz(band.hi) })}
            </span>
            <TargetHandles id="gallery.target" points={points} onChange={setPoints} scale={scale} />
          </>
        )}
      />
      <p className="muted">
        {t("plot.target.help")} · {points.map((p) => `${formatHz(p.hz)} Hz ${number(p.db, 1)} dB`).join(", ")}
      </p>
    </div>
  );
}

function XoGraph() {
  const t = useT();
  const freqs = useMemo(() => logGrid(5, 500, 300), []);
  const [xo, setXo] = useState(80);
  const curves: Curve[] = useMemo(
    () => [
      { id: "L", role: "L", kind: "main", position: 1, values: response(freqs, "L", 1, false) },
      { id: "SW1", role: "SW1", kind: "sub", position: 1, values: response(freqs, "SW1", 1, true) },
    ],
    [freqs],
  );
  return (
    <LogPlot
      id="gallery.plot.xo"
      label={t("plot.xo.label")}
      freqs={freqs}
      curves={curves}
      roleOrder={ROLES}
      isSub={isSubRole}
      xRange={[5, 500]}
      yRange={[-30, 12]}
      height={280}
      overlay={(scale) => <XoLine id="gallery.xo" hz={xo} onChange={setXo} scale={scale} />}
    />
  );
}

function AxesPopover() {
  const [value, setValue] = useState<{ x: readonly [number, number] | null; y: readonly [number, number] | null }>({ x: null, y: null });
  return (
    <div className="gal-axes-stage">
      <AxisRangeButton
        id="gallery.axes"
        value={value}
        defaults={{ x: [5, 22000], y: [-35, 20] }}
        onChange={setValue}
        initiallyOpen
        inline
        preset={{ x: [5, 22000], y: [10, -20], error: "y" }}
      />
    </div>
  );
}

function DensePlot() {
  const t = useT();
  const freqs = useMemo(() => logGrid(5, 22000, 3000), []);
  const roles = useMemo(() => [...PRESET_ROLES["9.1.6"]!, "SW2", "SW3", "SW4"], []);
  const curves = useMemo(() => measurementCurves(freqs, roles, 3, isSubRole), [freqs, roles]);
  const started = useRef(performance.now());
  const [ms, setMs] = useState<number | null>(null);
  useLayoutEffect(() => {
    requestAnimationFrame(() => setMs(performance.now() - started.current));
  }, []);
  return (
    <div className="gal-stack">
      <Graph
        id="gallery.plot.dense"
        label={t("plot.measurement.title")}
        freqs={freqs}
        curves={curves}
        channels={roles.map((role) => ({ role, label: roleCode(role), sub: isSubRole(role) }))}
        roleOrder={roles}
        positions={[1, 2, 3]}
        yRange={[-40, 20]}
      />
      <p className="gal-note tabular" data-gallery-perf={ms === null ? "" : Math.round(ms)}>
        {`${curves.length} curves × ${freqs.length} points · first paint ${ms === null ? "…" : `${Math.round(ms)} ms`}`}
      </p>
    </div>
  );
}

function StalePlot() {
  const t = useT();
  const freqs = useMemo(() => logGrid(5, 22000, 280), []);
  const curves = useMemo(() => measurementCurves(freqs, ["L", "R", "SW1"], 1, isSubRole), [freqs]);
  return (
    <Card title={t("plot.measurement.title")} stale dense>
      <Graph id="gallery.plot.stale" label={t("plot.measurement.title")} freqs={freqs} curves={curves} channels={CHANNELS.filter((c) => ["L", "R", "SW1"].includes(c.role))} roleOrder={ROLES} yRange={[-35, 20]} stale height={240} />
    </Card>
  );
}

const SPEAKERS = [...ROLES.map((role, i) => ({ role, outputs: [i + 1] }))];

function RoomSpecimen({ unassigned = false }: { unassigned?: boolean }) {
  const list = unassigned ? [...SPEAKERS.slice(0, 6), { role: "custom", outputs: [14] }, { role: "custom2", outputs: [15, 16] }] : SPEAKERS;
  const [selected, setSelected] = useState<number | null>(unassigned ? 6 : 2);
  return <RoomMap id={unassigned ? "gallery.room.unassigned" : "gallery.room"} speakers={list} selected={selected} onSelect={setSelected} />;
}

function ArrangementSpecimen({ count, done, current, selectable = true }: { count: number; done: number[]; current: number | null; selectable?: boolean }) {
  const [now, setNow] = useState(current);
  return <ArrangementMap id={`gallery.arrangement.${count}`} count={count} done={done} current={now} onSelect={selectable ? setNow : undefined} />;
}

function Thumbs() {
  const t = useT();
  const cards = [
    { count: 1, key: "map.arrangement.single" },
    { count: 9, key: "map.arrangement.one_chair" },
    { count: 13, key: "map.arrangement.couple" },
    { count: 17, key: "map.arrangement.family" },
  ] as const;
  return (
    <div className="gal-thumbs">
      {cards.map((card) => (
        <div key={card.count} className="gal-thumb-card" aria-current={card.count === 9 ? "true" : undefined}>
          <ArrangementThumb count={card.count} />
          <span className="gal-thumb-card__name">{t(card.key)}</span>
          <span className="muted tabular">{t("map.arrangement.count", { count: card.count })}</span>
        </div>
      ))}
    </div>
  );
}

export const plotSpecimens: SpecimenDef[] = [
  { id: "plot.measure", title: "LogPlot · 13 channels × 3 positions, legend and position chips", width: 960, render: () => <MeasurementGraph /> },
  { id: "plot.highlight", title: "LogPlot · SW1 highlighted (hover a chip)", width: 960, render: () => <MeasurementGraph highlight={{ role: "SW1" }} /> },
  { id: "plot.target", title: "Target editing · drag, double-click adds, right-click or Delete removes; RC band shaded", width: 960, render: () => <TargetGraph /> },
  { id: "plot.xo", title: "XO line · drag or arrow keys, 40–200 Hz", width: 760, render: () => <XoGraph /> },
  { id: "plot.axes", title: "Axis range popover · lower above upper", width: 420, render: () => <AxesPopover /> },
  { id: "plot.stale", title: "LogPlot · stale result", width: 760, render: () => <StalePlot /> },
  { id: "plot.dense", title: "Stress · 57 curves × 3000 points", width: 1100, render: () => <DensePlot /> },
];

export const mapSpecimens: SpecimenDef[] = [
  { id: "map.room", title: "Room map · 7.1.4 with two subs, C selected", render: () => <RoomSpecimen /> },
  { id: "map.room.unassigned", title: "Room map · speakers without a place", render: () => <RoomSpecimen unassigned /> },
  { id: "map.arrangement.9", title: "Arrangement · 9 positions, 1–3 done, 4 current (arrow keys move)", render: () => <ArrangementSpecimen count={9} done={[1, 2, 3]} current={4} /> },
  { id: "map.arrangement.17", title: "Arrangement · 17 positions on two sofas, 11 current", render: () => <ArrangementSpecimen count={17} done={[1, 2, 3, 4, 5, 6, 7, 8, 9, 10]} current={11} /> },
  { id: "map.arrangement.13", title: "Arrangement · 13 positions, view only", render: () => <ArrangementSpecimen count={13} done={[]} current={null} selectable={false} /> },
  { id: "map.thumbs", title: "Arrangement thumbnails", render: () => <Thumbs /> },
];
