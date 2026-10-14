// Presentation helpers for 측정·가져오기: names, spans and labels built from
// view data. No engine values are computed here.
import { isMessageKey, type MessageKey } from "../../i18n";
import type { Translate } from "../../i18n/react";
import type { LayoutResult } from "../../ipc/bindings/LayoutResult";
import type { MicPoint } from "../../ui/maps/geometry";
import { arrangementPoints, roleCode } from "../../ui/maps/geometry";
import type { PositionRecord } from "./types";

/** [1, 2, 3, 5] → "1–3, 5" (upstream `position_spans`). */
export function positionSpans(numbers: readonly number[]): string {
  const sorted = [...new Set(numbers)].sort((a, b) => a - b);
  const spans: [number, number][] = [];
  for (const n of sorted) {
    const last = spans[spans.length - 1];
    if (last && n === last[1] + 1) last[1] = n;
    else spans.push([n, n]);
  }
  return spans.map(([a, b]) => (a === b ? String(a) : `${a}–${b}`)).join(", ");
}

/** 262144 → "256k", 1048576 → "1M" (the names REW and the project use). */
export function sweepName(samples: number): string {
  if (samples >= 1048576 && samples % 1048576 === 0) return `${samples / 1048576}M`;
  return `${Math.round(samples / 1024)}k`;
}

/** Two digits for the list ("04"), as upstream. */
export const pad2 = (n: number) => String(n).padStart(2, "0");

/** The arrangement's points from `layout_get`, or the upstream geometry when it is missing. */
export function micPoints(layout: LayoutResult): MicPoint[] {
  const arrangement = layout.arrangements.find((item) => item.count === layout.count);
  if (!arrangement || arrangement.points.length === 0) return arrangementPoints(layout.count);
  return [...arrangement.points]
    .sort((a, b) => a.position - b.position)
    .map((point) => ({ x: point.x_cm, y: point.y_cm, h: point.height > 0 ? 1 : point.height < 0 ? -1 : 0 }));
}

/** Where position n is (upstream `arr_points` text: "청취 위치 · 귀 높이", "뒤 30 cm" …). */
export function pointText(t: Translate, n: number): string {
  const key = `measure.point.${n}`;
  return isMessageKey(key) ? t(key) : t("measure.point.other", { n });
}

/**
 * "Ltf · 왼쪽 탑 프론트" (upstream `role_text`: Dolby code and channel name;
 * a multi-amp band such as `L:LF` keeps its band after the name).
 */
export function roleText(t: Translate, role: string): string {
  const [base = role, band] = role.split(":");
  const sub = /^SW([1-8])$/.exec(base);
  const key = `measure.role.${base}`;
  const name = sub ? t("measure.role.sub", { n: sub[1] ?? "" }) : isMessageKey(key) ? t(key as MessageKey) : t("measure.role.custom");
  const text = t("measure.role.option", { code: roleCode(base), name });
  return band ? `${text} · ${band}` : text;
}

/** File names a position's roles came from; "" stands for roles measured here. */
export function recordSources(record: PositionRecord): string[] {
  const names: string[] = [];
  for (const role of record.roles) {
    const name = record.sources[role] ?? "";
    if (!names.includes(name)) names.push(name);
  }
  return names;
}

/** The source note: "a.mdat (위치 1–9) · 측정함 (위치 10)"; null when nothing came from a file. */
export function sourceSummary(t: Translate, records: readonly PositionRecord[]): string | null {
  const recorded = records.filter((record) => record.roles.length > 0);
  if (!recorded.some((record) => recordSources(record).some((name) => name !== ""))) return null;
  const groups = new Map<string, number[]>();
  for (const record of [...recorded].sort((a, b) => a.position - b.position)) {
    const name = recordSources(record)
      .map((source) => source || t("measure.sources.measured"))
      .join(" + ");
    groups.set(name, [...(groups.get(name) ?? []), record.position]);
  }
  return [...groups].map(([name, positions]) => t("measure.sources.group", { name, positions: positionSpans(positions) })).join(" · ");
}

/** The file name of a path, either separator. */
export const baseName = (path: string) => path.split(/[\\/]/).pop() ?? path;

/** "약 1분 53초" / "약 45초" for the per-position estimate (upstream `format_estimate`). */
export function estimateText(t: Translate, seconds: number): string {
  const total = Math.max(0, Math.round(seconds));
  const m = Math.floor(total / 60);
  const s = total % 60;
  return m > 0 ? t("measure.where.estimate.min", { m, s }) : t("measure.where.estimate.sec", { s });
}
