// The mic input meter (upstream mic_meter.py): level, held peak with a clip
// line, whether readings arrive, and a control that clears the peak. It sits
// in the level check, and the frame can float it as a small panel (upstream
// Ctrl+M; P33 wires the shortcut). The held peak is the highest Meter peak
// since the panel appeared or since 피크 지우기. The floating panel is not
// modal: it takes the focus when it opens (its close button, or its title),
// Esc inside it closes it, and the focus goes back where it was.
import { useEffect, useLayoutEffect, useRef, useState, type KeyboardEvent } from "react";
import { useNumber, useT } from "../../i18n/react";
import { Button, IconButton, LevelMeter, StatusPill, type PillTone } from "../../ui";
import type { MeterReading, MonitorState } from "./types";
import "./connect.css";

export type MicMeterView = {
  /** "입력 1 · UMIK-1": the input channel and device the monitor reads; null when no input is chosen. */
  input: string | null;
  monitor: MonitorState;
  reading: MeterReading | null;
};

/** Upstream shows "클립" when the held peak passes −1 dBFS. */
const CLIP_DBFS = -1;

const MONITOR: Record<MonitorState, { tone: PillTone; key: "connect.mic.off" | "connect.mic.starting" | "connect.mic.receiving" | "connect.mic.stalled" | "connect.mic.offline" }> = {
  off: { tone: "neutral", key: "connect.mic.off" },
  starting: { tone: "busy", key: "connect.mic.starting" },
  receiving: { tone: "success", key: "connect.mic.receiving" },
  stalled: { tone: "warning", key: "connect.mic.stalled" },
  offline: { tone: "danger", key: "connect.mic.offline" },
};

export function MicMeterPanel({
  view,
  id = "connect.mic",
  floating = false,
  onClose,
}: {
  view: MicMeterView;
  /** Prefix of the panel's ids (a floating copy uses another one). */
  id?: string;
  /** Drawn as a raised panel with a title (the frame's floating meter). */
  floating?: boolean;
  onClose?: (() => void) | undefined;
}) {
  const t = useT();
  const number = useNumber();
  const [peak, setPeak] = useState<number | null>(null);
  const panelRef = useRef<HTMLElement>(null);
  const closeRef = useRef<HTMLButtonElement>(null);
  const titleRef = useRef<HTMLHeadingElement>(null);
  const reading = view.reading;
  // Floating: focus in on opening, and back on closing unless the user has moved on.
  // A layout effect, so the cleanup runs while the panel still holds the focus.
  useLayoutEffect(() => {
    const panel = panelRef.current;
    if (!floating || !panel) return;
    const before = document.activeElement;
    (closeRef.current ?? titleRef.current)?.focus({ preventScroll: true });
    return () => {
      const active = document.activeElement;
      const inside = !active || active === document.body || panel.contains(active);
      if (inside && before instanceof HTMLElement && before !== document.body && before.isConnected) before.focus({ preventScroll: true });
    };
  }, [floating]);
  useEffect(() => {
    if (reading && Number.isFinite(reading.peak_dbfs) && reading.peak_dbfs > -200) setPeak((held) => (held === null ? reading.peak_dbfs : Math.max(held, reading.peak_dbfs)));
  }, [reading]);
  const clipped = (peak !== null && peak > CLIP_DBFS) || reading?.clipping === true;
  const state = MONITOR[view.monitor];
  const body = (
    <div className="connect-mic" data-ui={id}>
      <p className="connect-mic__input" data-ui={`${id}.input`}>
        {view.input ?? t("connect.mic.no_input")}
      </p>
      <LevelMeter
        id={`${id}.meter`}
        label={t("connect.mic.level")}
        value={reading ? Math.max(reading.rms_dbfs, -120) : null}
        peak={peak}
        clipping={reading?.clipping ?? false}
      />
      <div className="connect-mic__row">
        <span className={["connect-mic__peak", "tabular", clipped && "connect-mic__peak--clip"].filter(Boolean).join(" ")} data-ui={`${id}.peak`}>
          {peak === null ? t("connect.mic.peak.none") : t(clipped ? "connect.mic.peak.clip" : "connect.mic.peak", { value: number(peak, 1) })}
        </span>
        <Button variant="quiet" size="sm" data-ui={`${id}.clear_peak`} disabled={peak === null} onClick={() => setPeak(null)}>
          {t("connect.mic.clear_peak")}
        </Button>
      </div>
      <StatusPill tone={state.tone} data-ui={`${id}.state`}>
        {t(state.key)}
      </StatusPill>
      {view.monitor === "offline" && <p className="connect-hint">{t("connect.mic.offline.next")}</p>}
    </div>
  );
  if (!floating) return body;
  const onKeyDown = (event: KeyboardEvent<HTMLElement>) => {
    if (event.key !== "Escape" || !onClose) return;
    // Ours: the tutorial and the app do not see it.
    event.preventDefault();
    event.stopPropagation();
    onClose();
  };
  return (
    <section ref={panelRef} className="connect-mic-panel" aria-labelledby={`${id}.title`} onKeyDown={onKeyDown}>
      <header className="connect-mic-panel__head">
        <h2 ref={titleRef} className="connect-mic-panel__title" id={`${id}.title`} tabIndex={-1}>
          {t("connect.mic.title")}
        </h2>
        {onClose && <IconButton ref={closeRef} icon="close" size="sm" label={t("connect.mic.close")} data-ui={`${id}.close`} onClick={onClose} />}
      </header>
      {body}
    </section>
  );
}
