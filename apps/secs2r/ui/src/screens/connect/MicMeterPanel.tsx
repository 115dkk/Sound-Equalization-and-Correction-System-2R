// The mic input meter (upstream mic_meter.py): level, held peak with a clip
// line, whether readings arrive (and why not, when the monitor job failed),
// and a control that clears the peak. It sits
// in the level check, and the frame can float it as a small panel (upstream
// Ctrl+M; P33 wires the shortcut). The held peak is the highest Meter peak
// since the panel appeared or since 피크 지우기. The floating panel is not
// modal: it takes the focus when it opens (its close button, or its title),
// Esc inside it closes it, and the focus goes back where it was.
import { useEffect, useLayoutEffect, useRef, useState, type KeyboardEvent } from "react";
import { errorText } from "../../app/jobs/jobText";
import type { MessageKey } from "../../i18n";
import { useNumber, useT } from "../../i18n/react";
import type { IpcError } from "../../ipc/bindings/IpcError";
import { Button, ErrorNotice, IconButton, LevelMeter, StatusPill, type PillTone } from "../../ui";
import type { ConnectPath, MeterReading, MonitorState } from "./types";
import "./connect.css";

export type MicMeterView = {
  /** "입력 1 · UMIK-1": the input channel and device the monitor reads; null when no input is chosen. */
  input: string | null;
  monitor: MonitorState;
  reading: MeterReading | null;
  /** Why the monitor job failed; shown in place of the offline hint. Undefined or null: no failure known. */
  error?: IpcError | null | undefined;
  /** The measurement path, for the offline hint: only REW's mentions REW. Undefined: not known (the hint names both). */
  path?: ConnectPath | null | undefined;
};

/** Upstream shows "클립" when the held peak passes −1 dBFS. */
const CLIP_DBFS = -1;

const MONITOR: Record<MonitorState, { tone: PillTone; key: MessageKey }> = {
  off: { tone: "neutral", key: "connect.mic.off" },
  starting: { tone: "busy", key: "connect.mic.starting" },
  receiving: { tone: "success", key: "connect.mic.receiving" },
  stalled: { tone: "warning", key: "connect.mic.stalled" },
  offline: { tone: "danger", key: "connect.mic.offline" },
};

/** An offline monitor whose job failed: it answered, with an error. */
const FAILED = { tone: "danger", key: "connect.mic.failed" } as const satisfies { tone: PillTone; key: MessageKey };

/** Why the monitor stopped, from its error code; the service's own text sits behind 자세히. */
function MonitorFailure({ id, error }: { id: string; error: IpcError }) {
  const t = useT();
  const text = errorText(t, error);
  return (
    <ErrorNotice
      id={id}
      cause={text.generic ? t("connect.mic.failed.cause") : text.cause}
      next={text.next}
      detail={error.detail}
      code={error.code}
    />
  );
}

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
  const failure = view.monitor === "offline" ? (view.error ?? null) : null;
  const state = failure ? FAILED : MONITOR[view.monitor];
  // The native and simulated monitors have no REW to start; only REW's hint (or an unknown path's) names it.
  const offlineHint: MessageKey = view.path === "rew" || view.path === undefined ? "connect.mic.offline.next" : "connect.mic.offline.next.native";
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
      {failure ? (
        <MonitorFailure id={`${id}.error`} error={failure} />
      ) : (
        view.monitor === "offline" && (
          <p className="connect-hint" data-ui={`${id}.hint`}>
            {t(offlineHint)}
          </p>
        )
      )}
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
