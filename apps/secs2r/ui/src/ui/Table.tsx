// Data table with optional group header rows (프론트, 센터, 서브 …).
// Numbers are right-aligned and tabular; a caption names the table for
// screen readers; selectable rows answer to click, Enter and Space.
import { Fragment, type KeyboardEvent, type ReactNode } from "react";

export type Column<R> = {
  key: string;
  header: ReactNode;
  cell: (row: R) => ReactNode;
  numeric?: boolean;
  /** CSS width (e.g. "6rem"); columns without one share the rest. */
  width?: string;
};

export type RowGroup<R> = { id: string; label: ReactNode; rows: readonly R[] };

type TableProps<R> = {
  id?: string;
  caption: ReactNode;
  /** Show the caption on screen (otherwise it is for screen readers only). */
  showCaption?: boolean;
  columns: readonly Column<R>[];
  groups: readonly RowGroup<R>[];
  rowKey: (row: R) => string;
  selectedKey?: string | null;
  onSelect?: (row: R) => void;
  /** Row content shown when every group is empty. */
  empty?: ReactNode;
};

export function DataTable<R>({ id, caption, showCaption, columns, groups, rowKey, selectedKey, onSelect, empty }: TableProps<R>) {
  const total = groups.reduce((sum, group) => sum + group.rows.length, 0);
  const onRowKey = (event: KeyboardEvent<HTMLTableRowElement>, row: R) => {
    if (onSelect && (event.key === "Enter" || event.key === " ")) {
      event.preventDefault();
      onSelect(row);
    }
  };
  const grouped = groups.length > 1 || (groups[0] !== undefined && groups[0].label !== "");
  return (
    <div className="ui-table-wrap">
      <table id={id} data-ui={id} className={["ui-table", onSelect && "ui-table--selectable"].filter(Boolean).join(" ")}>
        <caption className={showCaption ? "ui-table__caption" : "visually-hidden"}>{caption}</caption>
        <colgroup>
          {columns.map((column) => (
            <col key={column.key} style={column.width ? { width: column.width } : undefined} />
          ))}
        </colgroup>
        <thead>
          <tr>
            {columns.map((column) => (
              <th key={column.key} scope="col" className={column.numeric ? "ui-table__num" : undefined}>
                {column.header}
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          {total === 0 && empty !== undefined && (
            <tr className="ui-table__empty">
              <td colSpan={columns.length}>{empty}</td>
            </tr>
          )}
          {groups.map((group) => (
            <Fragment key={group.id}>
              {grouped && group.rows.length > 0 && (
                <tr className="ui-table__group">
                  <th scope="colgroup" colSpan={columns.length}>
                    {group.label}
                  </th>
                </tr>
              )}
              {group.rows.map((row) => {
                const key = rowKey(row);
                const selected = selectedKey === key;
                return (
                  <tr
                    key={key}
                    data-row={key}
                    className={selected ? "ui-table__row--selected" : undefined}
                    aria-current={onSelect && selected ? "true" : undefined}
                    tabIndex={onSelect ? 0 : undefined}
                    onClick={onSelect ? () => onSelect(row) : undefined}
                    onKeyDown={onSelect ? (event) => onRowKey(event, row) : undefined}
                  >
                    {columns.map((column) => (
                      <td key={column.key} className={column.numeric ? "ui-table__num tabular" : undefined}>
                        {column.cell(row)}
                      </td>
                    ))}
                  </tr>
                );
              })}
            </Fragment>
          ))}
        </tbody>
      </table>
    </div>
  );
}
