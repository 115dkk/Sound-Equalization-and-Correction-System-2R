// Axis range popover (upstream axis_range.py): frequency and level limits,
// 자동 returns to the graph's own range, 적용 sets the typed one. A lower
// limit must sit below its upper limit; the popover says so and stays open.
// Enter applies. The chosen range is kept by the screen for the session.
import { useEffect, useRef, useState } from "react";
import { useT } from "../../i18n/react";
import { Button, IconButton } from "../Button";
import { NumberField } from "../NumberField";
import { Popover } from "../Popover";
import { Tooltip } from "../Tooltip";

export type AxisRangeValue = { x: readonly [number, number] | null; y: readonly [number, number] | null };

function validateRange(x: readonly [number, number], y: readonly [number, number]): "ok" | "x" | "y" {
  if (!(x[0] < x[1])) return "x";
  if (!(y[0] < y[1])) return "y";
  return "ok";
}

export function AxisRangeButton({
  id,
  value,
  defaults,
  onChange,
  yUnit = "dB",
  yLimits = [-120, 60],
  initiallyOpen = false,
  inline = false,
  preset,
}: {
  id: string;
  value: AxisRangeValue;
  /** The graph's own range, shown when a limit is automatic. */
  defaults: { x: readonly [number, number]; y: readonly [number, number] };
  onChange: (value: AxisRangeValue) => void;
  yUnit?: string;
  yLimits?: readonly [number, number];
  /** Gallery: show the popover open, in place. */
  initiallyOpen?: boolean;
  inline?: boolean;
  /** Gallery: a typed draft (and its error) to show on first open. */
  preset?: { x: readonly [number, number]; y: readonly [number, number]; error?: "x" | "y" };
}) {
  const t = useT();
  const anchor = useRef<HTMLButtonElement>(null);
  const [open, setOpen] = useState(initiallyOpen);
  const [draft, setDraft] = useState(() => (preset ? { x: preset.x, y: preset.y } : { x: value.x ?? defaults.x, y: value.y ?? defaults.y }));
  const [error, setError] = useState<"x" | "y" | null>(preset?.error ?? null);
  const latest = useRef(draft);
  latest.current = draft;
  const firstOpen = useRef(true);

  // Each time the popover opens, the draft starts from the graph's range.
  useEffect(() => {
    if (!open) return;
    if (firstOpen.current && preset) {
      firstOpen.current = false;
      return;
    }
    firstOpen.current = false;
    setDraft({ x: value.x ?? defaults.x, y: value.y ?? defaults.y });
    setError(null);
  }, [open]);

  const apply = () => {
    const { x, y } = latest.current;
    const check = validateRange(x, y);
    if (check !== "ok") {
      setError(check);
      return;
    }
    setError(null);
    onChange({ x: [x[0], x[1]], y: [y[0], y[1]] });
    setOpen(false);
  };

  const custom = value.x !== null || value.y !== null;
  const set = (axis: "x" | "y", side: 0 | 1, v: number) =>
    setDraft((d) => {
      const pair: [number, number] = [d[axis][0], d[axis][1]];
      pair[side] = v;
      return { ...d, [axis]: pair };
    });

  return (
    <>
      <Tooltip content={custom ? t("plot.axes.tooltip_custom") : t("plot.axes.tooltip")}>
        <IconButton
          ref={anchor}
          icon="axes"
          size="sm"
          label={t("plot.axes.button")}
          className={custom ? "plot-axes-button plot-axes-button--custom" : "plot-axes-button"}
          aria-expanded={open}
          aria-haspopup="dialog"
          data-ui={id}
          onClick={() => setOpen((o) => !o)}
        />
      </Tooltip>
      <Popover open={open} onClose={() => setOpen(false)} anchor={anchor} label={t("plot.axes.title")} placement="bottom-end" id={`${id}.popover`} inline={inline}>
        <form
          className="plot-axes"
          onSubmit={(event) => {
            event.preventDefault();
            apply();
          }}
          onKeyDown={(event) => {
            if (event.key === "Enter") {
              event.preventDefault();
              // NumberField commits on Enter first; apply after that update lands.
              window.setTimeout(apply, 0);
            }
          }}
        >
          <h3 className="plot-axes__title">{t("plot.axes.title")}</h3>
          <fieldset className="plot-axes__pair">
            <legend>{t("plot.axes.frequency")}</legend>
            <NumberField id={`${id}.x0`} label={t("plot.axes.lower")} value={draft.x[0]} onChange={(v) => set("x", 0, v)} min={1} max={24000} step={1} unit="Hz" size={6} />
            <NumberField id={`${id}.x1`} label={t("plot.axes.upper")} value={draft.x[1]} onChange={(v) => set("x", 1, v)} min={1} max={24000} step={1} unit="Hz" size={6} />
          </fieldset>
          <fieldset className="plot-axes__pair">
            <legend>{t("plot.axes.level")}</legend>
            <NumberField id={`${id}.y0`} label={t("plot.axes.lower")} value={draft.y[0]} onChange={(v) => set("y", 0, v)} min={yLimits[0]} max={yLimits[1]} step={1} unit={yUnit} digits={0} size={5} />
            <NumberField id={`${id}.y1`} label={t("plot.axes.upper")} value={draft.y[1]} onChange={(v) => set("y", 1, v)} min={yLimits[0]} max={yLimits[1]} step={1} unit={yUnit} digits={0} size={5} />
          </fieldset>
          {error && (
            <p className="plot-axes__error" role="alert">
              {t(error === "x" ? "plot.axes.error_frequency" : "plot.axes.error_level")}
            </p>
          )}
          <div className="plot-axes__actions">
            <Button
              variant="quiet"
              data-ui={`${id}.auto`}
              onClick={() => {
                onChange({ x: null, y: null });
                setOpen(false);
              }}
            >
              {t("plot.axes.auto")}
            </Button>
            <Button variant="primary" type="submit" data-ui={`${id}.apply`}>
              {t("plot.axes.apply")}
            </Button>
          </div>
        </form>
      </Popover>
    </>
  );
}
