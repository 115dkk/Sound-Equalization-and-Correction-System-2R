import { HelpTip } from "../../ui/HelpTip";
// 내보내기: write what the DSP and REW need. What the correction is (mode,
// delay, channels, MIMO, RC range), where the files go, and the exports in
// two groups: the filters for the DSP (the CamillaDSP configuration, the
// step's primary action in the header; the filter values as text; the
// Equalizer APO configuration) and the responses for REW (the before/after
// mdat, and the staged mdat under 고급, U3). A filter format that cannot
// carry the result says why in its row (upstream 1.3.0 `_filter_card`).
// No 완료 button (U4): the step is done once the current result is exported.
import { useEffect, useId, useRef, type ReactNode } from "react";
import type { PrimaryAction } from "../../app/StepHeader";
import { errorText } from "../../app/jobs/jobText";
import type { MessageKey } from "../../i18n";
import { useNumber, useT, type Translate } from "../../i18n/react";
import type { CalibrationSummary } from "../../ipc/bindings/CalibrationSummary";
import type { ExportBlock } from "../../ipc/bindings/ExportBlock";
import type { ExportKind } from "../../ipc/bindings/ExportKind";
import type { Mode } from "../../ipc/bindings/Mode";
import type { Notice } from "../../ipc/bindings/Notice";
import { Advanced, Button, Card, EmptyState, ErrorNotice, Icon, StatusPill } from "../../ui";
import { roleCode } from "../../ui/maps/geometry";
import {
  EXPORT_KINDS,
  canExport,
  exportBlock,
  exportStepState,
  filterModeName,
  mimoInclusion,
  type ExportActions,
  type ExportRun,
  type ExportView,
  type MimoInclusion,
} from "./types";
import "./export.css";

const MODE_TEXT: Record<Exclude<Mode, "ultra">, MessageKey> = {
  zero: "export.mode.zero",
  mixed: "export.mode.mixed",
  linear: "export.mode.linear",
};

const KIND_TEXT: Record<ExportKind, { title: MessageKey; body: MessageKey; run?: MessageKey; exporting: MessageKey }> = {
  camilla_dsp: { title: "export.kind.camilla_dsp", body: "export.kind.camilla_dsp.body", exporting: "export.status.exporting.camilla_dsp" },
  before_after_mdat: {
    title: "export.kind.before_after_mdat",
    body: "export.kind.before_after_mdat.body",
    run: "export.kind.before_after_mdat.run",
    exporting: "export.status.exporting.before_after_mdat",
  },
  staged_mdat: { title: "export.kind.staged_mdat", body: "export.kind.staged_mdat.body", run: "export.kind.staged_mdat.run", exporting: "export.status.exporting.staged_mdat" },
  filter_text: { title: "export.kind.filter_text", body: "export.kind.filter_text.body", run: "export.kind.filter_text.run", exporting: "export.status.exporting.filter_text" },
  equalizer_apo: {
    title: "export.kind.equalizer_apo",
    body: "export.kind.equalizer_apo.body",
    run: "export.kind.equalizer_apo.run",
    exporting: "export.status.exporting.equalizer_apo",
  },
};

/** Why a filter format cannot carry this result: only these pairs occur (`exportBlock`). */
const BLOCK_TEXT: Partial<Record<ExportKind, Partial<Record<ExportBlock, MessageKey>>>> = {
  filter_text: { subs: "export.blocked.filter_text.subs", mimo: "export.blocked.filter_text.mimo" },
  equalizer_apo: { mimo: "export.blocked.equalizer_apo.mimo" },
};

const MIMO_REASON: Record<Extract<MimoInclusion, { included: false }>["reason"], MessageKey> = {
  off: "export.result.mimo.off",
  positions: "export.result.mimo.positions",
  none: "export.result.mimo.none",
  stale: "export.result.mimo.stale",
};

export function modeText(t: Translate, mode: Mode, ultraMs: number, number: (value: number, digits?: number) => string): string {
  return mode === "ultra" ? t("export.mode.ultra", { ms: number(ultraMs) }) : t(MODE_TEXT[mode]);
}

/** One RC notice from `result_lines().rc` as a phrase; numbers arrive as strings and are rounded to whole Hz and m³. */
export function rcText(t: Translate, notice: Notice, number: (value: number, digits?: number) => string): string {
  const p = notice.params;
  const whole = (value: string | undefined) => (value === undefined || Number.isNaN(Number(value)) ? value ?? "" : number(Number(value)));
  switch (notice.code) {
    case "rc_limit":
      if (p.auto !== "true") return t("export.rc.rc_limit.manual", { hz: whole(p.hz) });
      return p.schroeder_hz !== undefined
        ? t("export.rc.rc_limit.schroeder", { hz: whole(p.hz), schroeder_hz: whole(p.schroeder_hz) })
        : t("export.rc.rc_limit.auto", { hz: whole(p.hz) });
    case "rc_room_volume":
      return t("export.rc.rc_room_volume", { volume_m3: whole(p.volume_m3) });
    case "rc_no_room_volume":
      return t("export.rc.rc_no_room_volume");
    case "rc_auto_cap":
      return t("export.rc.rc_auto_cap", { hz: whole(p.hz) });
    default:
      return t("jobs.log.unknown");
  }
}

export function ExportScreen({
  view,
  actions,
  initialAdvanced = false,
}: {
  view: ExportView;
  actions: ExportActions;
  /** Gallery: draw 고급 unfolded. The fold is the screen's own state. */
  initialAdvanced?: boolean;
}) {
  const rootRef = useRef<HTMLDivElement>(null);
  const runsRef = useRef(view.runs);
  // A run that ends (the header's primary starts one far above its row) brings
  // its outcome into view inside the content's scroll area; the first draw does not scroll.
  useEffect(() => {
    const before = runsRef.current;
    runsRef.current = view.runs;
    const changed = EXPORT_KINDS.find((kind) => view.runs[kind] && view.runs[kind] !== before[kind]);
    const el = changed ? rootRef.current?.querySelector<HTMLElement>(`[data-ui="export.outcome.${changed}"]`) : null;
    if (el) revealInside(el);
  }, [view.runs]);
  return (
    <div ref={rootRef} className="export-screen" data-ui="export">
      <ResultCard view={view} actions={actions} />
      <FilesCard view={view} actions={actions} initialAdvanced={initialAdvanced} />
    </div>
  );
}

/** Scrolls the nearest scrolling ancestor (the frame's content) so `el` shows, never the page. */
function revealInside(el: HTMLElement) {
  for (let node = el.parentElement; node; node = node.parentElement) {
    if (!/(auto|scroll)/.test(getComputedStyle(node).overflowY) || node.scrollHeight <= node.clientHeight) continue;
    const top = el.getBoundingClientRect().top - node.getBoundingClientRect().top + node.scrollTop;
    const bottom = top + el.offsetHeight;
    if (top < node.scrollTop + 16) node.scrollTop = Math.max(0, top - 16);
    else if (bottom > node.scrollTop + node.clientHeight - 16) node.scrollTop = bottom - node.clientHeight + 16;
    return;
  }
}

/**
 * The step header: the primary exports the CamillaDSP configuration; the
 * pill says what runs, that an export failed, that the result is stale, or
 * that the step is done.
 */
export function exportHeader(view: ExportView, actions: ExportActions, t: Translate): { primary?: PrimaryAction; status?: ReactNode } {
  const state = exportStepState(view);
  // A blocked step already says why under the title; the pill adds what runs,
  // that an export failed (its row may be below the fold), that the result
  // no longer matches the inputs (it still exports), or that it is exported.
  const failed = EXPORT_KINDS.some((kind) => view.runs[kind]?.status === "failed");
  const status = view.exporting ? (
    <StatusPill tone="busy">{t(KIND_TEXT[view.exporting].exporting)}</StatusPill>
  ) : failed ? (
    <StatusPill tone="danger">{t("export.status.failed")}</StatusPill>
  ) : view.results.calibration?.stale ? (
    <StatusPill tone="warning" data-ui="export.status.stale">
      {t("frame.status.calibration.stale")}
    </StatusPill>
  ) : state.status === "done" ? (
    <StatusPill tone="success">{t("export.status.done")}</StatusPill>
  ) : undefined;
  return {
    primary: {
      id: "export.run.camilla_dsp",
      label: t("frame.step.export.primary"),
      onClick: () => actions.exportFiles("camilla_dsp"),
      // Export jobs run one at a time: busy while this one runs, off while another does.
      busy: view.exporting === "camilla_dsp",
      disabled: view.exporting !== null && view.exporting !== "camilla_dsp",
    },
    status,
  };
}

/* ── The result ─────────────────────────────────────────────────────────── */

function ResultCard({ view, actions }: { view: ExportView; actions: ExportActions }) {
  const t = useT();
  const result = view.results.calibration;
  if (!result) {
    // Show the running job or the action that creates an exportable result.
    return (
      <Card data-ui="export.result" aria-label={t("export.result.title")}>
        {view.computing ? (
          <div className="export-computing" data-ui="export.result.computing">
            <StatusPill tone="busy">{t("frame.status.calibration.computing")}</StatusPill>
          </div>
        ) : (
          <EmptyState
            id="export.result.empty"
            title={t("export.result.empty")}
            action={
              <Button variant="secondary" iconEnd="arrowRight" data-ui="export.go_calibrate" onClick={actions.goToCalibrate}>
                {t("export.go_calibrate")}
              </Button>
            }
          />
        )}
      </Card>
    );
  }
  return (
    <Card
      title={t("export.result.title")}
      data-ui="export.result"
      stale={result.stale}
      actions={
        view.computing ? (
          <StatusPill tone="busy" data-ui="export.result.computing">
            {t("frame.status.calibration.computing")}
          </StatusPill>
        ) : null
      }
    >
      {result.stale && (
        <ErrorNotice
          id="export.result.stale"
          tone="warning"
          cause={t("export.result.stale")}
          next={t("export.result.stale.next")}
          action={
            <Button variant="secondary" size="sm" iconEnd="arrowRight" data-ui="export.go_calibrate" onClick={actions.goToCalibrate}>
              {t("export.go_calibrate")}
            </Button>
          }
        />
      )}
      <ResultFacts view={view} result={result} />
    </Card>
  );
}

function ResultFacts({ view, result }: { view: ExportView; result: CalibrationSummary }) {
  const t = useT();
  const number = useNumber();
  const mimo = mimoInclusion(view);
  const roles = [...result.main_roles, ...result.sub_roles].map(roleCode);
  const rc = view.rc.map((notice) => rcText(t, notice, number));
  return (
    <div className="export-facts">
      <dl className="export-facts__grid" data-ui="export.result.summary">
        <Fact label={t("export.result.mode")}>{modeText(t, view.settings.mode, view.settings.ultra_ms, number)}</Fact>
        <Fact label={t("export.result.latency")}>
          <span className="tabular">{t("export.result.latency.value", { ms: number(result.latency_ms, 1) })}</span>
        </Fact>
        <Fact label={t("export.result.channels")}>
          <span className="tabular">{t("export.result.channels.value", { count: result.main_roles.length })}</span>
        </Fact>
        <Fact label={t("export.result.subs")}>
          {result.sub_roles.length > 0 ? <span className="tabular">{t("export.result.subs.value", { count: result.sub_roles.length })}</span> : t("export.result.subs.none")}
        </Fact>
      </dl>
      <dl className="export-facts__lines">
        <div className="export-line" data-ui="export.result.mimo">
          <dt>{t("export.result.mimo")}</dt>
          <dd className={mimo.included ? undefined : "muted"}>
            {mimo.included ? (
              <span className="tabular">
                {t("export.result.mimo.included", { channels: mimo.channels, kind: mimo.kind.toUpperCase(), supports: mimo.supports })}
              </span>
            ) : (
              t(MIMO_REASON[mimo.reason], { count: view.positions })
            )}
          </dd>
        </div>
        {rc.length > 0 && (
          <div className="export-line" data-ui="export.result.rc">
            <dt>{t("export.result.rc")}</dt>
            <dd className="tabular">{rc.join(" · ")}</dd>
          </div>
        )}
        <div className="export-line" data-ui="export.result.roles">
          <dt>{t("export.result.roles")}</dt>
          <dd className="export-roles">{roles.join(" · ")}</dd>
        </div>
      </dl>
    </div>
  );
}

function Fact({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="export-fact">
      <dt>{label}</dt>
      <dd>{children}</dd>
    </div>
  );
}

/* ── The files ──────────────────────────────────────────────────────────── */

function FilesCard({ view, actions, initialAdvanced }: { view: ExportView; actions: ExportActions; initialAdvanced: boolean }) {
  const t = useT();
  return (
    <Card title={t("export.files.title")} data-ui="export.files">
      <FolderRow view={view} actions={actions} />
      <div className="export-groups">
        <ExportGroup title={t("export.group.filters")}>
          <ExportItem kind="camilla_dsp" view={view} actions={actions} />
          <ExportItem kind="filter_text" view={view} actions={actions} />
          <ExportItem kind="equalizer_apo" view={view} actions={actions} />
        </ExportGroup>
        <ExportGroup title={t("export.group.rew")}>
          <ExportItem kind="before_after_mdat" view={view} actions={actions} />
          <Advanced id="export.advanced" count={1} defaultOpen={initialAdvanced}>
            <ExportItem kind="staged_mdat" view={view} actions={actions} />
          </Advanced>
        </ExportGroup>
      </div>
    </Card>
  );
}

/** One group of exports: its heading sits between the card's title (h2) and the rows' titles (h4). */
function ExportGroup({ title, children }: { title: string; children: ReactNode }) {
  const titleId = useId();
  return (
    <section className="export-group" aria-labelledby={titleId}>
      <h3 className="export-group__title" id={titleId}>
        {title}
      </h3>
      {children}
    </section>
  );
}

function FolderRow({ view, actions }: { view: ExportView; actions: ExportActions }) {
  const t = useT();
  if (view.tutorial) {
    return (
      <div className="export-folder" data-ui="export.folder">
        <div className="export-folder__text">
          <p className="export-folder__label">{t("export.folder")}</p>
          <p className="export-tutorial" data-ui="export.tutorial">
            <Icon name="info" size={18} />
            <span>{t("export.tutorial")}</span>
          </p>
        </div>
      </div>
    );
  }
  const folder = view.folder;
  return (
    <div className="export-folder" data-ui="export.folder">
      <div className="export-folder__text">
        <p className="export-folder__label">{t("export.folder")}</p>
        <p className="export-path" data-ui="export.folder.path">
          {folder ? folder.folder : <span className="muted">{t("export.folder.loading")}</span>}
        </p>
        {folder && <p className="export-folder__hint">{t(folder.stored ? "export.folder.stored" : "export.folder.default")}</p>}
      </div>
      <div className="export-folder__actions">
        <Button size="sm" data-ui="export.folder.change" disabled={view.exporting !== null} onClick={actions.changeFolder}>
          {t("export.folder.change")}
        </Button>
        <Button size="sm" variant="quiet" icon="folder" data-ui="export.folder.open" disabled={!folder} onClick={() => folder && actions.openFolder(folder.folder)}>
          {t("export.folder.open")}
        </Button>
      </div>
    </div>
  );
}

/** Where a kind's files land below the export folder (the service's folder rules, `export_folder.rs`). */
function whereText(t: Translate, kind: ExportKind, view: ExportView): string {
  if (kind === "camilla_dsp") return t("export.kind.camilla_dsp.where");
  if (kind === "staged_mdat") return t("export.kind.staged_mdat.where");
  if (kind === "filter_text") return t("export.kind.filter_text.where", { mode: view.settings.mode });
  if (kind === "equalizer_apo") return t("export.kind.equalizer_apo.where", { mode: view.settings.mode });
  return t("export.kind.before_after_mdat.where", { what: mimoInclusion(view).included ? "RC_BA_MIMO" : "RC_BA", mode: view.settings.mode });
}

function ExportItem({ kind, view, actions }: { kind: ExportKind; view: ExportView; actions: ExportActions }) {
  const t = useT();
  const text = KIND_TEXT[kind];
  const run = view.runs[kind];
  const running = view.exporting === kind;
  const titleId = `export.kind.${kind}.title`;
  const block = exportBlock(view, kind);
  const blocked = block ? BLOCK_TEXT[kind]?.[block] : undefined;
  const blockedId = useId();
  return (
    <section className="export-item" data-ui={`export.kind.${kind}`} aria-labelledby={titleId}>
      <div className="export-item__text">
        <div className="ui-heading-help">
          <h4 className="export-item__title" id={titleId}>{t(text.title)}</h4>
          <HelpTip id={`export.kind.${kind}.help`} labelledBy={titleId} content={t(text.body)} />
        </div>
        <p className="export-item__where">
          <Icon name="folder" size={16} />
          <span>{whereText(t, kind, view)}</span>
        </p>
      </div>
      <div className="export-item__action">
        {text.run ? (
          <Button
            data-ui={`export.run.${kind}`}
            busy={running}
            disabled={!canExport(view) || block !== null}
            aria-describedby={blocked ? blockedId : undefined}
            onClick={() => actions.exportFiles(kind)}
          >
            {t(text.run)}
          </Button>
        ) : running ? (
          <StatusPill tone="busy">{t("export.run.busy")}</StatusPill>
        ) : null}
      </div>
      {blocked && (
        // The format's scope, not a fault: the info icon and the muted text, never the warning hue.
        <p className="export-item__blocked" id={blockedId} data-ui={`export.kind.${kind}.blocked`}>
          <Icon name="info" size={18} />
          <span>{t(blocked)}</span>
        </p>
      )}
      {run && <Outcome kind={kind} run={run} view={view} actions={actions} />}
    </section>
  );
}

function Outcome({ kind, run, view, actions }: { kind: ExportKind; run: ExportRun; view: ExportView; actions: ExportActions }) {
  const t = useT();
  const id = `export.outcome.${kind}`;
  if (run.status === "failed") {
    const text = errorText(t, run.error);
    return (
      <div className="export-item__outcome" data-ui={id}>
        <ErrorNotice cause={text.cause} next={text.next} detail={run.error.detail} code={run.error.code} />
      </div>
    );
  }
  if (run.status === "tutorial") {
    return (
      <p className="export-item__outcome export-outcome export-outcome--muted" data-ui={id} role="status">
        <Icon name="info" size={18} />
        <span>{t("export.run.tutorial")}</span>
      </p>
    );
  }
  return (
    <div className="export-item__outcome export-done" data-ui={id} role="status">
      <div className="export-outcome">
        <Icon name="check" size={18} weight="semibold" className="export-outcome__icon" />
        <p className="export-outcome__text">
          <span className="tabular">{t("export.run.done", { count: run.files.length, path: run.folder })}</span>
        </p>
        <Button size="sm" variant="quiet" icon="folder" data-ui={`${id}.open`} onClick={() => actions.openFolder(run.folder)}>
          {t("export.folder.open")}
        </Button>
      </div>
      {kind === "equalizer_apo" && (
        // What to do with the folder: the files go into Equalizer APO's own config folder.
        <p className="export-done__hint" data-ui={`${id}.hint`}>
          {t("export.outcome.equalizer_apo.hint", { mode: filterModeName(view.settings) })}
        </p>
      )}
    </div>
  );
}
