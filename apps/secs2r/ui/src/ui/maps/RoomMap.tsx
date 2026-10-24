// Room map (upstream RoomLayoutView, layout_map.py): the screen, the
// listening seat and one marker per speaker: a filled circle for a main
// speaker, a dashed circle for top and height speakers, a square for a
// subwoofer. Each marker carries its code and outputs. Speakers whose role
// has no place in the room sit in the 역할 미지정 row under the map. One
// tab stop; arrow keys move the selection in speaker order. Each marker is
// pressed through an invisible 40 px circle; where two speakers sit closer
// than that (C and SW5), each keeps the half nearer to it.
import type { KeyboardEvent } from "react";
import { useT } from "../../i18n/react";
import type { MessageKey } from "../../i18n";
import { ROOM_POSITIONS, roleCode, roleKind, type RoleKind } from "./geometry";

const SIZE = 420;
const MARGIN = 20;
const INNER = SIZE - MARGIN * 2;
/** Half the 40 px hit area every control reaches (docs/DESIGN.md §5). */
export const HIT_R = 20;

type Point = readonly [number, number];

/**
 * The part of a speaker's hit circle that is nearer to it than to any other
 * speaker, as a polygon to clip the circle with: the square around the
 * circle, cut along the line halfway to each speaker closer than two radii.
 * Null when none is that close (the whole circle is its own).
 */
export function hitCell(at: Point, others: readonly Point[], r: number = HIT_R): Point[] | null {
  const near = others.filter(([x, y]) => (x !== at[0] || y !== at[1]) && Math.hypot(x - at[0], y - at[1]) < 2 * r);
  if (near.length === 0) return null;
  let cell: Point[] = [
    [at[0] - r, at[1] - r],
    [at[0] + r, at[1] - r],
    [at[0] + r, at[1] + r],
    [at[0] - r, at[1] + r],
  ];
  for (const [ox, oy] of near) {
    // Keep the side of the halfway line that holds `at` (Sutherland–Hodgman).
    const side = ([px, py]: Point) => (px - (ox + at[0]) / 2) * (ox - at[0]) + (py - (oy + at[1]) / 2) * (oy - at[1]);
    const kept: Point[] = [];
    cell.forEach((a, i) => {
      const b = cell[(i + 1) % cell.length] ?? a;
      const sa = side(a);
      const sb = side(b);
      if (sa <= 0) kept.push(a);
      if (sa <= 0 !== sb <= 0) {
        const t = sa / (sa - sb);
        kept.push([a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]);
      }
    });
    cell = kept;
  }
  return cell;
}

type RoomSpeaker = { role: string; outputs: readonly number[] };

const KIND_KEY: Record<RoleKind, MessageKey> = { main: "map.kind.main", sub: "map.kind.sub", top: "map.kind.top", height: "map.kind.height" };

export function RoomMap({
  id,
  speakers,
  selected,
  onSelect,
}: {
  id: string;
  speakers: readonly RoomSpeaker[];
  selected: number | null;
  onSelect?: ((index: number) => void) | undefined;
}) {
  const t = useT();
  const placed = speakers
    .map((speaker, index) => {
      const at = ROOM_POSITIONS[speaker.role];
      return { speaker, index, at: at ? ([MARGIN + at[0] * INNER, MARGIN + at[1] * INNER] as const) : undefined };
    })
    .filter((x): x is { speaker: RoomSpeaker; index: number; at: readonly [number, number] } => x.at !== undefined);
  const cells = placed.map(({ at }) => hitCell(at, placed.map((other) => other.at)));
  const unplaced = speakers.map((speaker, index) => ({ speaker, index })).filter(({ speaker }) => !ROOM_POSITIONS[speaker.role]);
  const optionId = (index: number) => `${id}-spk-${index}`;
  const outputs = (speaker: RoomSpeaker) => speaker.outputs.join("/");
  const describe = (speaker: RoomSpeaker) =>
    t("map.speaker.aria", {
      code: roleCode(speaker.role),
      kind: t(KIND_KEY[roleKind(speaker.role)]),
      outputs: outputs(speaker) || "—",
    });

  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (!onSelect || speakers.length === 0) return;
    const step = event.key === "ArrowLeft" || event.key === "ArrowUp" ? -1 : event.key === "ArrowRight" || event.key === "ArrowDown" ? 1 : 0;
    if (!step) return;
    event.preventDefault();
    const next = selected === null ? (step > 0 ? 0 : speakers.length - 1) : Math.min(speakers.length - 1, Math.max(0, selected + step));
    onSelect(next);
  };

  return (
    <div className="map map--room">
      <div
        className="map__frame"
        id={id}
        data-ui={id}
        role={onSelect ? "listbox" : "img"}
        aria-label={t("map.room.label", { count: speakers.length })}
        aria-activedescendant={onSelect && selected !== null && ROOM_POSITIONS[speakers[selected]?.role ?? ""] ? optionId(selected) : undefined}
        tabIndex={onSelect ? 0 : undefined}
        onKeyDown={onKeyDown}
      >
        <svg viewBox={`0 0 ${SIZE} ${SIZE}`} width={SIZE} height={SIZE} className="map__svg">
          {cells.some(Boolean) && (
            <defs>
              {placed.map(({ index }, i) => {
                const cell = cells[i];
                return cell ? (
                  <clipPath key={index} id={`${optionId(index)}-hit`}>
                    <polygon points={cell.map(([px, py]) => `${px},${py}`).join(" ")} />
                  </clipPath>
                ) : null;
              })}
            </defs>
          )}
          <rect className="map__screen" x={MARGIN + INNER * 0.22} y={10} width={INNER * 0.56} height={7} rx={2} />
          <text className="map__caption map__caption--screen" x={SIZE / 2} y={34} textAnchor="middle">
            {t("map.screen")}
          </text>
          {/* The caption's meaning without words (see ArrangementMap). */}
          <g className="map__front" aria-hidden="true">
            <line className="map__front-shaft" x1={SIZE / 2} y1={27} x2={SIZE / 2} y2={40} />
            <path className="map__front-head" d={`M${SIZE / 2},19 L${SIZE / 2 + 4},27 L${SIZE / 2 - 4},27 Z`} />
          </g>
          <g className="map__furniture">
            <rect x={MARGIN + INNER * 0.36} y={MARGIN + INNER * 0.62} width={INNER * 0.28} height={INNER * 0.11} rx={6} />
            <rect className="map__furniture-dark" x={MARGIN + INNER * 0.38} y={MARGIN + INNER * 0.61} width={INNER * 0.24} height={INNER * 0.05} rx={3} />
          </g>
          <text className="map__caption map__caption--seat" x={SIZE / 2} y={MARGIN + INNER * 0.705} textAnchor="middle" dominantBaseline="central">
            {t("map.seat")}
          </text>
          {placed.map(({ speaker, index, at }, i) => {
            const [x, y] = at;
            const kind = roleKind(speaker.role);
            const isSelected = index === selected;
            return (
              <g
                key={index}
                id={optionId(index)}
                className={`map-speaker map-speaker--${kind}${isSelected ? " map-speaker--selected" : ""}`}
                role={onSelect ? "option" : undefined}
                aria-selected={onSelect ? isSelected : undefined}
                aria-label={describe(speaker)}
                data-ui={`${id}.speaker.${speaker.role}`}
                onClick={onSelect ? () => onSelect(index) : undefined}
              >
                {isSelected && <circle className="map-speaker__halo" cx={x} cy={y} r={19} />}
                <circle className="map-speaker__hit" cx={x} cy={y} r={HIT_R} clipPath={cells[i] ? `url(#${optionId(index)}-hit)` : undefined} />
                {kind === "sub" ? (
                  <rect className="map-speaker__body" x={x - 11} y={y - 11} width={22} height={22} rx={3} />
                ) : (
                  <circle className="map-speaker__body" cx={x} cy={y} r={10} />
                )}
                <text className="map-speaker__code" x={x} y={y + 25} textAnchor="middle">
                  {roleCode(speaker.role)}
                  <tspan className="map-speaker__outputs" dx={4}>
                    {outputs(speaker)}
                  </tspan>
                </text>
              </g>
            );
          })}
        </svg>
      </div>
      <ul className="map__legend" aria-hidden="true">
        <li>
          <svg width="14" height="14" viewBox="0 0 14 14" className="map__legend-mark map-speaker--main">
            <circle cx="7" cy="7" r="6" className="map-speaker__body" />
          </svg>
          {t("map.kind.main")}
        </li>
        <li>
          <svg width="14" height="14" viewBox="0 0 14 14" className="map__legend-mark map-speaker--top">
            <circle cx="7" cy="7" r="5.5" className="map-speaker__body" />
          </svg>
          {t("map.kind.overhead")}
        </li>
        <li>
          <svg width="14" height="14" viewBox="0 0 14 14" className="map__legend-mark map-speaker--sub">
            <rect x="1" y="1" width="12" height="12" rx="2" className="map-speaker__body" />
          </svg>
          {t("map.kind.sub")}
        </li>
      </ul>
      {unplaced.length > 0 && (
        <div className="map__unassigned" role="group" aria-label={t("map.unassigned")}>
          <span className="map__unassigned-label">{t("map.unassigned")}</span>
          {unplaced.map(({ speaker, index }) => (
            <button
              key={index}
              type="button"
              className="map__unassigned-chip tabular"
              aria-pressed={index === selected}
              aria-label={t("map.unassigned.aria", { outputs: outputs(speaker) || "—" })}
              data-ui={`${id}.unassigned.${index}`}
              onClick={() => onSelect?.(index)}
            >
              {t("map.output_short", { outputs: outputs(speaker) || "—" })}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}
