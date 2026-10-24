// Project warnings (`ProjectInfo.migration_warnings`, F5) as the status lists
// them: one sentence per code, then one line per measurement position with
// the roles it lost, or the entry as the service wrote it. A code is
// translated by `project.warning.<code>`; one without a key reads as an
// unknown warning with its raw code and entry.
import { isMessageKey } from "../i18n";
import type { Translate } from "../i18n/react";
import { roleCode } from "../ui/maps/geometry";
import type { ProjectWarning } from "./model";

/** Codes the app clears by measuring the position again (the new IR lands in the project's own folder). */
const REMEASURE: ReadonlySet<string> = new Set(["measurement.ir_path.outside"]);

export type WarningRow = { position: number; roles: string[] } | { raw: string };
export type WarningGroup = { code: string; text: string; known: boolean; remeasure: boolean; rows: WarningRow[] };

/** A warning's identity, for dismissing it. */
export const warningKey = (warning: ProjectWarning): string => `${warning.code}\n${warning.entry}`;

/** "position 3, role FL" (secs-project `ir_path_of`) as its position and role. */
export function warningTarget(entry: string): { position: number; role: string } | null {
  const match = /^position (\d+), role (.+)$/.exec(entry);
  return match?.[1] && match[2] ? { position: Number(match[1]), role: match[2] } : null;
}

function warningText(t: Translate, code: string): { text: string; known: boolean } {
  const key = `project.warning.${code}`;
  if (isMessageKey(key)) return { text: t(key), known: true };
  return { text: t("project.warning.unknown"), known: false };
}

/** Groups by code in the order the service raised them; positions keep one line each. */
export function groupWarnings(t: Translate, warnings: readonly ProjectWarning[]): WarningGroup[] {
  const groups = new Map<string, WarningGroup>();
  for (const warning of warnings) {
    let group = groups.get(warning.code);
    if (!group) {
      const { text, known } = warningText(t, warning.code);
      group = { code: warning.code, text, known, remeasure: REMEASURE.has(warning.code), rows: [] };
      groups.set(warning.code, group);
    }
    const target = warningTarget(warning.entry);
    const row = target && group.rows.find((r): r is { position: number; roles: string[] } => "position" in r && r.position === target.position);
    if (row && target) {
      if (!row.roles.includes(roleCode(target.role))) row.roles.push(roleCode(target.role));
    } else if (target && target.position >= 1) {
      group.rows.push({ position: target.position, roles: [roleCode(target.role)] });
    } else {
      group.rows.push({ raw: warning.entry });
    }
  }
  return [...groups.values()];
}
