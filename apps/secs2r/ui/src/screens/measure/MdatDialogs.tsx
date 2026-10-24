// The two mdat dialogs of 측정·가져오기 (upstream MdatAssignDialog and
// MdatFilesDialog, step5_measure.py:247-437).
//
// Assignment: every measurement of the scanned file with what it holds
// (unreadable, low band only, full range), the position it goes to (when
// the arrangement has several) and the channel (or 건너뜀), pre-filled from
// `mdat_scan`. The same channel twice at one position, or nothing chosen,
// disables 가져오기 (upstream `problems()`, the service's
// `mdat_duplicate_roles` and `mdat_nothing_selected`).
//
// Files: per position, the file its measurements came from (or 측정함, or
// none), the channels it has, 가져오기… (the whole file into that position)
// and 지우기 (that position only; confirm and undo, U5).
import { useId, useMemo, useState } from "react";
import { useT, type Translate } from "../../i18n/react";
import type { MdatAssignment } from "../../ipc/bindings/MdatAssignment";
import type { MdatImportParams } from "../../ipc/bindings/MdatImportParams";
import type { Notice } from "../../ipc/bindings/Notice";
import { Button, CellSelect, DataTable, Dialog, ErrorNotice, type Column } from "../../ui";
import { roleCode } from "../../ui/maps/geometry";
import { baseName, recordSources, roleText } from "./format";
import type { MdatPending, PositionRecord } from "./types";

const SKIP = "";

type Row = { role: string; position: number };

/** The problems that block the import (upstream `MdatAssignDialog.problems`). */
export function assignmentProblems(
  rows: readonly Row[],
  readable: readonly boolean[],
  count: number,
): { notice: Notice | null; invalid: Set<number> } {
  const chosen = rows
    .map((row, index) => ({ ...row, index }))
    .filter((row) => row.role !== SKIP && readable[row.index] && row.position >= 1 && row.position <= count);
  const seen = new Map<string, number[]>();
  for (const row of chosen) {
    const key = `${row.position}\u0000${row.role}`;
    seen.set(key, [...(seen.get(key) ?? []), row.index]);
  }
  const doubles = [...seen.values()].filter((indices) => indices.length > 1);
  if (doubles.length > 0) {
    const several = new Set(chosen.map((row) => row.position)).size > 1;
    const names = doubles
      .map((indices) => rows[indices[0] ?? 0]!)
      .sort((a, b) => a.position - b.position || a.role.localeCompare(b.role))
      .map((row) => (several ? `${roleCode(row.role)} P${row.position}` : roleCode(row.role)));
    return { notice: { code: "mdat_duplicate_roles", params: { roles: names.join(", ") } }, invalid: new Set(doubles.flat()) };
  }
  if (chosen.length === 0) return { notice: { code: "mdat_nothing_selected", params: {} }, invalid: new Set() };
  return { notice: null, invalid: new Set() };
}

export function MdatAssignDialog({
  pending,
  roles,
  count,
  fallbackPosition,
  onImport,
  onClose,
  inline,
}: {
  pending: MdatPending;
  /** Roles of the measurement plan, in plan order. */
  roles: readonly string[];
  /** Positions in the arrangement. */
  count: number;
  /** Where rows go when the import names none (the screen's current position). */
  fallbackPosition: number;
  onImport: (params: MdatImportParams) => void;
  onClose: () => void;
  inline?: boolean;
}) {
  const t = useT();
  const problemId = useId();
  // A Dirac Live project carries every position; 파일 관리 brings in only the row's own position (ADR 0013).
  const measurements = useMemo(
    () =>
      pending.position !== null && pending.path.toLowerCase().endsWith(".liveproject")
        ? pending.scan.measurements.filter((m) => m.position === pending.position)
        : pending.scan.measurements,
    [pending],
  );
  const readable = useMemo(() => measurements.map((m) => m.looks_like_sub !== null), [measurements]);
  const [rows, setRows] = useState<Row[]>(() =>
    measurements.map((m) => ({
      role: m.looks_like_sub !== null && m.role && roles.includes(m.role) ? m.role : SKIP,
      // The scan guesses position 1 for a title that names none; those rows go where the hint says (the screen's current position).
      position: pending.position ?? (m.position_tagged ? m.position : fallbackPosition),
    })),
  );
  const showPositions = count > 1 || pending.position !== null || measurements.some((m) => m.position !== 1);
  const { notice, invalid } = assignmentProblems(rows, readable, count);
  const assignments: MdatAssignment[] = rows.flatMap((row, index) => {
    const m = measurements[index];
    if (!m || row.role === SKIP || !readable[index] || row.position < 1 || row.position > count) return [];
    return [{ measurement: m.title, role: row.role, index: m.index, position: row.position }];
  });

  const set = (index: number, patch: Partial<Row>) => setRows((list) => list.map((row, i) => (i === index ? { ...row, ...patch } : row)));
  const roleOptions = [{ value: SKIP, label: t("measure.mdat.skip") }, ...roles.map((role) => ({ value: role, label: roleText(t, role) }))];
  const positionOptions = (current: number) => [
    ...Array.from({ length: count }, (_, i) => ({ value: String(i + 1), label: t("measure.mdat.position", { n: i + 1 }) })),
    ...(current > count ? [{ value: String(current), label: t("measure.mdat.position.outside", { n: current }) }] : []),
  ];
  const detail = (index: number) => {
    const flag = measurements[index]?.looks_like_sub;
    return flag === null || flag === undefined ? t("measure.mdat.detail.unreadable") : flag ? t("measure.mdat.detail.sub") : t("measure.mdat.detail.full");
  };

  type Item = { index: number };
  const columns: Column<Item>[] = [
    {
      key: "title",
      header: t("measure.mdat.col.measurement"),
      cell: ({ index }) => {
        const title = measurements[index]?.title || t("measure.mdat.untitled", { n: index + 1 });
        return <span className="measure-cell-text">{title}</span>;
      },
    },
    {
      key: "detail",
      header: t("measure.mdat.col.content"),
      width: "9.5rem",
      cell: ({ index }) => <span className={readable[index] ? "measure-cell-muted" : "measure-cell-faint"}>{detail(index)}</span>,
    },
    ...(showPositions
      ? [
          {
            key: "position",
            header: t("measure.mdat.col.position"),
            width: "8.5rem",
            cell: ({ index }: Item) => {
              const row = rows[index]!;
              return (
                <CellSelect
                  id={`measure.mdat.row.${index + 1}.position`}
                  label={t("measure.mdat.position.label", { title: measurements[index]?.title ?? "" })}
                  value={String(row.position)}
                  options={positionOptions(row.position)}
                  disabled={!readable[index]}
                  onChange={(value) => set(index, { position: Number(value) })}
                />
              );
            },
          },
        ]
      : []),
    {
      key: "role",
      header: t("measure.mdat.col.role"),
      width: "16.5rem",
      cell: ({ index }) => (
        <CellSelect
          id={`measure.mdat.row.${index + 1}.role`}
          label={t("measure.mdat.role.label", { title: measurements[index]?.title ?? "" })}
          value={rows[index]!.role}
          options={roleOptions}
          disabled={!readable[index]}
          invalid={invalid.has(index)}
          describedBy={problemId}
          onChange={(value) => set(index, { role: value })}
        />
      ),
    },
  ];

  const file = baseName(pending.path);
  return (
    <Dialog
      open
      inline={inline}
      id="measure.mdat.dialog"
      help={t(showPositions ? "measure.mdat.dialog.intro.positions" : "measure.mdat.dialog.intro")}
      title={t("measure.mdat.dialog.title", { file })}
      onClose={onClose}
      footer={
        <>
          <Button variant="secondary" onClick={onClose} data-ui="measure.mdat.cancel">
            {t("ui.dialog.cancel")}
          </Button>
          <Button
            variant="primary"
            data-ui="measure.mdat.import"
            disabled={notice !== null}
            aria-describedby={notice ? problemId : undefined}
            onClick={() => onImport({ path: pending.path, assignments, position: pending.position ?? fallbackPosition })}
          >
            {t("measure.mdat.import", { count: assignments.length })}
          </Button>
        </>
      }
    >
      <div className="measure-dialog">
        <div className="measure-dialog__table">
          <DataTable
            id="measure.mdat.rows"
            caption={t("measure.mdat.dialog.caption", { file })}
            columns={columns}
            groups={[{ id: "rows", label: "", rows: measurements.map((_, index) => ({ index })) }]}
            rowKey={(item) => String(item.index)}
          />
        </div>
        {notice && (
          <div id={problemId}>
            <ErrorNotice id="measure.mdat.problem" tone="warning" cause={noticeText(t, notice)} />
          </div>
        )}
      </div>
    </Dialog>
  );
}

/** A service notice code as a sentence (`notice.<code>`). */
export function noticeText(t: Translate, notice: Notice): string {
  switch (notice.code) {
    case "mdat_duplicate_roles":
      return t("notice.mdat_duplicate_roles", { roles: duplicateRoles(notice.params.roles ?? "") });
    case "mdat_nothing_selected":
      return t("notice.mdat_nothing_selected");
    default:
      return t("jobs.log.unknown");
  }
}

/**
 * The service sends the doubled pairs as JSON (`[[pos, "role"], …]`); the
 * dialog's own check sends them already written. Either way: "L P1, R P1".
 */
function duplicateRoles(value: string): string {
  try {
    const pairs = JSON.parse(value) as unknown;
    if (Array.isArray(pairs)) {
      return pairs
        .filter((pair): pair is [number, string] => Array.isArray(pair) && pair.length === 2)
        .map(([position, role]) => `${roleCode(String(role))} P${position}`)
        .join(", ");
    }
  } catch {
    // Already written out.
  }
  return value;
}

export function MdatFilesDialog({
  open,
  count,
  records,
  roleCount,
  busy,
  onLoad,
  onClear,
  onClose,
  inline,
}: {
  open: boolean;
  count: number;
  records: readonly PositionRecord[];
  /** Roles in the measurement plan (channels have / total). */
  roleCount: number;
  /** A device job runs: nothing can start. */
  busy: boolean;
  onLoad: (position: number) => void;
  onClear: (position: number) => void;
  onClose: () => void;
  inline?: boolean;
}) {
  const t = useT();
  const byPosition = new Map(records.map((record) => [record.position, record]));
  type Item = { position: number; record: PositionRecord | undefined };
  const items: Item[] = Array.from({ length: count }, (_, i) => ({ position: i + 1, record: byPosition.get(i + 1) }));
  const fileText = (record: PositionRecord | undefined) => {
    if (!record || record.roles.length === 0) return null;
    return recordSources(record)
      .map((name) => name || t("measure.files.measured"))
      .join(" + ");
  };
  const columns: Column<Item>[] = [
    {
      key: "position",
      header: t("measure.files.col.position"),
      width: "7.5rem",
      cell: ({ position }) => <span className="measure-cell-nowrap">{t("measure.mdat.position", { n: position })}</span>,
    },
    {
      key: "file",
      header: t("measure.files.col.file"),
      cell: ({ record }) => {
        const text = fileText(record);
        if (!text) return <span className="measure-cell-faint">{t("measure.files.none")}</span>;
        return <span className="measure-cell-text">{text}</span>;
      },
    },
    {
      key: "channels",
      header: t("measure.files.col.channels"),
      numeric: true,
      width: "5rem",
      cell: ({ record }) => t("measure.files.channels", { have: record?.roles.length ?? 0, total: roleCount }),
    },
    {
      key: "actions",
      header: <span className="visually-hidden">{t("measure.files.col.actions")}</span>,
      width: "12.5rem",
      cell: ({ position, record }) => (
        <div className="measure-row-actions">
          <Button
            size="sm"
            data-ui={`measure.files.${position}.load`}
            aria-label={t("measure.files.load.label", { n: position })}
            disabled={busy}
            onClick={() => onLoad(position)}
          >
            {t("measure.files.load")}
          </Button>
          <Button
            size="sm"
            variant="quiet"
            data-ui={`measure.files.${position}.clear`}
            aria-label={t("measure.clear.title", { n: position })}
            disabled={busy || !record || record.roles.length === 0}
            onClick={() => onClear(position)}
          >
            {t("measure.files.clear")}
          </Button>
        </div>
      ),
    },
  ];
  return (
    <Dialog
      open={open}
      inline={inline}
      id="measure.files"
      help={t("measure.files.intro")}
      title={t("measure.files.title")}
      onClose={onClose}
      footer={
        <Button variant="secondary" onClick={onClose} data-ui="measure.files.close">
          {t("ui.close")}
        </Button>
      }
    >
      <div className="measure-dialog">
        <div className="measure-dialog__table">
          <DataTable
            id="measure.files.rows"
            caption={t("measure.files.title")}
            columns={columns}
            groups={[{ id: "positions", label: "", rows: items }]}
            rowKey={(item) => String(item.position)}
          />
        </div>
      </div>
    </Dialog>
  );
}
