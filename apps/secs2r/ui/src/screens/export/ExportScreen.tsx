import { HelpTip } from "../../ui/HelpTip";
// 내보내기: write what the DSP, the player and REW need. What the correction
// is (mode, delay, channels, MIMO, RC range), where the files go, and the
// exports in two groups: the filters for the DSP or player (the design name
// once above them, F21; the CamillaDSP configuration, the step's primary
// action in the header, with its devices folded in its row, F20; the filter
// values as text; the Equalizer APO configuration; the convolution IR) and
// the responses for REW (the REW mdat with what it carries, F19, and the
// staged mdat under 고급, U3). A filter format that cannot carry the result
// says why in its row (upstream 1.3.0 `_filter_card`).
// The CamillaDSP and Equalizer APO rows choose 파일만, 적용 or 관리 (ADR 0018,
// apply.tsx): with 적용 the header's primary applies to CamillaDSP and the
// row keeps its export as a secondary button; Equalizer APO's row applies
// with its own button and keeps its export beside it, quiet. With 관리 the
// same actions apply and then hand the design to the SECS 2R Host service.
// No 완료 button (U4): the step is done once the current result is exported.
import { useEffect, useId, useRef, type ReactNode } from "react";
import type { PrimaryAction } from "../../app/StepHeader";
import { errorText } from "../../app/jobs/jobText";
import type { MessageKey } from "../../i18n";
import { useNumber, useT, type Translate } from "../../i18n/react";
import type { CalibrationSummary } from "../../ipc/bindings/CalibrationSummary";
import type { ExportBlock } from "../../ipc/bindings/ExportBlock";
import type { ExportKind } from "../../ipc/bindings/ExportKind";
import type { MdatScope } from "../../ipc/bindings/MdatScope";
import type { Mode } from "../../ipc/bindings/Mode";
import type { Notice } from "../../ipc/bindings/Notice";
import { Advanced, Button, Card, EmptyState, ErrorNotice, Icon, Segmented, StatusPill, type PillTone } from "../../ui";
import { roleCode } from "../../ui/maps/geometry";
import { ApplyBlock, eqapoReason } from "./apply";
import { CamillaDevicesDisclosure, DESIGN_NAME_ID, DesignNameField } from "./fields";
import {
  EXPORT_KINDS,
  MDAT_SCOPES,
  applyLevel,
  applyProblem,
  canApply,
  canExport,
  designNameFor,
  designNameRejection,
  exportBlock,
  exportGroupBusy,
  exportStepState,
  filterModeName,
  inputProblem,
  isFilterKind,
  mdatFileName,
  mdatScopeFor,
  mimoInclusion,
  type ApplyProblem,
  type ApplyTarget,
  type ExportActions,
  type ExportRun,
  type ExportView,
  type HostProblem,
  type InputProblem,
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
  convolution: {
    title: "export.kind.convolution",
    body: "export.kind.convolution.body",
    run: "export.kind.convolution.run",
    exporting: "export.status.exporting.convolution",
  },
};

/** What a done export says next about its folder: where the files go (Equalizer APO) or how to hang them (the convolution IR). */
const OUTCOME_HINT: Partial<Record<ExportKind, MessageKey>> = {
  equalizer_apo: "export.outcome.equalizer_apo.hint",
  convolution: "export.outcome.convolution.hint",
};

/** The header's pill when the primary is off for something on this screen: a field to change, or 관리's Windows or service. */
const PROBLEM_STATUS: Record<Exclude<InputProblem["field"] | ApplyProblem["field"], "eqapo" | "host">, MessageKey> = {
  design_name: "export.status.design_name",
  capture: "export.status.asio_device",
  playback: "export.status.asio_device",
  address: "export.status.address",
  exe_path: "export.status.exe_path",
  manage: "export.status.windows_only",
};

const HOST_STATUS: Record<HostProblem, { text: MessageKey; tone: PillTone }> = {
  checking: { text: "export.status.host.checking", tone: "busy" },
  not_installed: { text: "export.status.host.not_installed", tone: "warning" },
  stopped: { text: "export.status.host.stopped", tone: "warning" },
  unreachable: { text: "export.status.host.unreachable", tone: "warning" },
};

function problemStatus(problem: InputProblem | ApplyProblem): { text: MessageKey; tone: PillTone } | null {
  if (problem.field === "eqapo") return null;
  if (problem.field === "host") return HOST_STATUS[problem.problem];
  return { text: PROBLEM_STATUS[problem.field], tone: "warning" };
}

const APPLYING_TEXT: Record<ApplyTarget, MessageKey> = { camilla: "export.status.applying.camilla", eqapo: "export.status.applying.eqapo" };
const MANAGING_TEXT: Record<ApplyTarget, MessageKey> = { camilla: "export.status.managing.camilla", eqapo: "export.status.managing.eqapo" };

/** Why a filter format cannot carry this result: only these pairs occur (`exportBlock`). */
const BLOCK_TEXT: Partial<Record<ExportKind, Partial<Record<ExportBlock, MessageKey>>>> = {
  filter_text: { subs: "export.blocked.filter_text.subs", mimo: "export.blocked.filter_text.mimo" },
  staged_mdat: { stale_cache: "export.blocked.mdat.stale_cache" },
  before_after_mdat: { stale_cache: "export.blocked.mdat.stale_cache" },
};

const SCOPE_TEXT: Record<MdatScope, MessageKey> = {
  all: "export.mdat.scope.all",
  measurements: "export.mdat.scope.measurements",
  results: "export.mdat.scope.results",
};

const MIMO_REASON: Record<Extract<MimoInclusion, { included: false }>["reason"], MessageKey> = {
  off: "export.result.mimo.off",
  positions: "export.result.mimo.positions",
  none: "export.result.mimo.none",
  stale: "export.result.mimo.stale",
};

function modeText(t: Translate, mode: Mode, ultraMs: number, number: (value: number, digits?: number) => string): string {
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
  initialDevices = false,
  initialRevertConfirm = false,
}: {
  view: ExportView;
  actions: ExportActions;
  /** Gallery: draw 고급 unfolded. The fold is the screen's own state. */
  initialAdvanced?: boolean;
  /** Gallery: draw the CamillaDSP devices unfolded. The fold is the screen's own state. */
  initialDevices?: boolean;
  /** Gallery and tests: open with 빼기's confirm showing. The confirm is the screen's own state. */
  initialRevertConfirm?: boolean;
}) {
  const rootRef = useRef<HTMLDivElement>(null);
  const runsRef = useRef(view.runs);
  const applyRunsRef = useRef(view.apply.runs);
  // A run that ends (the header's primary starts one far above its row) brings
  // its outcome into view inside the content's scroll area; the first draw does not scroll.
  useEffect(() => {
    const before = runsRef.current;
    runsRef.current = view.runs;
    const changed = EXPORT_KINDS.find((kind) => view.runs[kind] && view.runs[kind] !== before[kind]);
    const el = changed ? rootRef.current?.querySelector<HTMLElement>(`[data-ui="export.outcome.${changed}"]`) : null;
    if (el) revealInside(el);
  }, [view.runs]);
  // The same for an apply (CamillaDSP's starts in the header too).
  useEffect(() => {
    const before = applyRunsRef.current;
    applyRunsRef.current = view.apply.runs;
    const changed = (["camilla", "eqapo"] as const).find((target) => view.apply.runs[target] && view.apply.runs[target] !== before[target]);
    const el = changed ? rootRef.current?.querySelector<HTMLElement>(`[data-ui="export.apply.${changed}.outcome"]`) : null;
    if (el) revealInside(el);
  }, [view.apply.runs]);
  return (
    <div ref={rootRef} className="export-screen" data-ui="export">
      <ResultCard view={view} actions={actions} />
      <FilesCard view={view} actions={actions} initialAdvanced={initialAdvanced} initialDevices={initialDevices} initialRevertConfirm={initialRevertConfirm} />
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
 * The step header: the primary exports the CamillaDSP configuration, with
 * CamillaDSP set to 적용 applies it to the running CamillaDSP, and with 관리
 * also hands it to the service; the pill says what runs, what holds the
 * primary off, that an export or an apply failed, that the result is stale,
 * or that the step is done.
 */
export function exportHeader(view: ExportView, actions: ExportActions, t: Translate): { primary?: PrimaryAction; status?: ReactNode } {
  const state = exportStepState(view);
  const level = applyLevel(view, "camilla");
  const applying = level !== "files";
  // What holds the primary off makes the pill its reason (StepHeader
  // describes the primary by it) and the row below says what to change:
  // for the export a design name that cannot be a folder or an ASIO side
  // without a driver, for 적용 the address or that ASIO side, for 관리 also
  // Windows, the executable and the service.
  const problem = applying ? applyProblem(view, "camilla") : inputProblem(view, "camilla_dsp");
  const problemPill = problem ? problemStatus(problem) : null;
  const applyingTarget = view.apply.applying;
  // A blocked step already says why under the title; the pill adds what runs,
  // that an export or an apply failed (its row may be below the fold), that
  // the result no longer matches the inputs (it still exports), or that it
  // is exported or applied.
  const failed = EXPORT_KINDS.some((kind) => view.runs[kind]?.status === "failed");
  const applyFailed = Object.values(view.apply.runs).some((run) => run?.status === "failed" || run?.status === "rolled_back");
  const applied = Object.values(view.apply.runs).some((run) => run?.status === "applied");
  const status = view.exporting ? (
    <StatusPill tone="busy">{t(KIND_TEXT[view.exporting].exporting)}</StatusPill>
  ) : applyingTarget ? (
    <StatusPill tone="busy">{t((applyLevel(view, applyingTarget) === "manage" ? MANAGING_TEXT : APPLYING_TEXT)[applyingTarget])}</StatusPill>
  ) : problemPill ? (
    <StatusPill tone={problemPill.tone} data-ui="export.status.problem">
      {t(problemPill.text)}
    </StatusPill>
  ) : failed ? (
    <StatusPill tone="danger">{t("export.status.failed")}</StatusPill>
  ) : applyFailed ? (
    <StatusPill tone="danger">{t("export.status.apply_failed")}</StatusPill>
  ) : view.results.calibration?.stale ? (
    <StatusPill tone="warning" data-ui="export.status.stale">
      {t("frame.status.calibration.stale")}
    </StatusPill>
  ) : state.status === "done" ? (
    <StatusPill tone="success">{t(applied ? "export.status.applied" : "export.status.done")}</StatusPill>
  ) : undefined;
  // Export and apply jobs run one at a time: busy while this one runs, off
  // while another does, and off while a field on this screen needs changing.
  const busy = applying ? view.apply.applying === "camilla" : view.exporting === "camilla_dsp";
  const disabled = busy ? false : exportGroupBusy(view) || problem !== null;
  return {
    primary: applying
      ? { id: "export.apply.camilla", label: t(level === "manage" ? "export.manage.camilla.run" : "export.apply.camilla.run"), onClick: actions.applyCamilla, busy, disabled }
      : { id: "export.run.camilla_dsp", label: t("frame.step.export.primary"), onClick: () => actions.exportFiles("camilla_dsp"), busy, disabled },
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

function FilesCard({
  view,
  actions,
  initialAdvanced,
  initialDevices,
  initialRevertConfirm,
}: {
  view: ExportView;
  actions: ExportActions;
  initialAdvanced: boolean;
  initialDevices: boolean;
  initialRevertConfirm: boolean;
}) {
  const t = useT();
  return (
    <Card title={t("export.files.title")} data-ui="export.files">
      <FolderRow view={view} actions={actions} />
      <div className="export-groups">
        <ExportGroup title={t("export.group.filters")}>
          {/* One name for every filter export, above their rows; the tutorial has no folder to name. */}
          {!view.tutorial && <DesignNameField view={view} actions={actions} />}
          <ExportItem kind="camilla_dsp" view={view} actions={actions} initialDevices={initialDevices} />
          <ExportItem kind="filter_text" view={view} actions={actions} />
          <ExportItem kind="equalizer_apo" view={view} actions={actions} initialRevertConfirm={initialRevertConfirm} />
          <ExportItem kind="convolution" view={view} actions={actions} />
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
  // The tutorial has no export folder; the outcome rows say nothing was saved.
  if (view.tutorial) return null;
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

/**
 * Where a kind's files land below the export folder (the service's folder
 * rules, `export_folder.rs`; a named filter export goes to
 * `SECS_<kind>_<name>`, `design_export.rs` `named_folder`).
 */
function whereText(t: Translate, kind: ExportKind, view: ExportView): string {
  const name = isFilterKind(kind) ? designNameFor(view) : null;
  if (name !== null) return kind === "camilla_dsp" ? t("export.kind.camilla_dsp.where.named", { name }) : t("export.kind.where.named", { kind, name });
  if (kind === "camilla_dsp") return t("export.kind.camilla_dsp.where");
  if (kind === "staged_mdat") return t("export.kind.staged_mdat.where");
  if (kind === "filter_text") return t("export.kind.filter_text.where", { mode: view.settings.mode });
  if (kind === "before_after_mdat" && mdatScopeFor(view.results.calibration, view.mdatScope) === "measurements") {
    return t("export.kind.before_after_mdat.where.measurements");
  }
  const what = mimoInclusion(view).included ? "RC_BA_MIMO" : "RC_BA";
  if (kind === "equalizer_apo") return t("export.kind.equalizer_apo.where", { what, mode: view.settings.mode });
  if (kind === "convolution") return t("export.kind.convolution.where", { what, mode: view.settings.mode });
  return t("export.kind.before_after_mdat.where", { what, mode: view.settings.mode });
}

/**
 * What the REW mdat carries, chosen before its button is pressed. With no
 * result yet only the measurements can go, so the choice shows that and
 * says why.
 */
function MdatScopeField({ view, actions }: { view: ExportView; actions: ExportActions }) {
  const t = useT();
  const result = view.results.calibration;
  const scope = mdatScopeFor(result, view.mdatScope);
  const hint = result ? undefined : t(view.positions > 0 ? "export.mdat.scope.no_result" : "export.mdat.scope.no_measurements");
  return (
    <div className="export-item__scope">
      <Segmented<MdatScope>
        id="export.mdat.scope"
        label={t("export.mdat.scope")}
        size="sm"
        value={scope}
        options={MDAT_SCOPES.map((value) => ({ value, label: t(SCOPE_TEXT[value]), disabled: !result && value !== "measurements" }))}
        onChange={actions.setMdatScope}
        hint={hint}
      />
    </div>
  );
}

/** The rows that choose 파일만 or 적용 (ADR 0018), and the target each applies to. */
const APPLY_TARGET: Partial<Record<ExportKind, ApplyTarget>> = { camilla_dsp: "camilla", equalizer_apo: "eqapo" };

function ExportItem({
  kind,
  view,
  actions,
  initialDevices = false,
  initialRevertConfirm = false,
}: {
  kind: ExportKind;
  view: ExportView;
  actions: ExportActions;
  initialDevices?: boolean;
  initialRevertConfirm?: boolean;
}) {
  const t = useT();
  const text = KIND_TEXT[kind];
  const run = view.runs[kind];
  const running = view.exporting === kind;
  // The tutorial writes nothing, so it applies nothing either.
  const target = view.tutorial ? undefined : APPLY_TARGET[kind];
  const level = target === undefined ? "files" : applyLevel(view, target);
  const applying = level !== "files";
  const titleId = `export.kind.${kind}.title`;
  const block = exportBlock(view, kind);
  const blocked = block ? BLOCK_TEXT[kind]?.[block] : undefined;
  const blockedId = useId();
  const recalibrateId = useId();
  const scoped = kind === "before_after_mdat";
  // A design name that cannot be a folder holds the filter buttons off; the field's message is the reason.
  const problem = inputProblem(view, kind);
  const describedBy = blocked ? blockedId : problem?.field === "design_name" ? `${DESIGN_NAME_ID}-error` : undefined;
  return (
    <section className={scoped ? "export-item export-item--scoped" : "export-item"} data-ui={`export.kind.${kind}`} aria-labelledby={titleId}>
      <div className="export-item__text">
        <div className="ui-heading-help">
          <h4 className="export-item__title" id={titleId}>{t(text.title)}</h4>
          <HelpTip id={`export.kind.${kind}.help`} labelledBy={titleId} content={t(text.body)} />
        </div>
        {scoped && <MdatScopeField view={view} actions={actions} />}
        <p className="export-item__where">
          <Icon name="folder" size={16} />
          <span>{whereText(t, kind, view)}</span>
        </p>
        {scoped && (
          // The file's own name: it says whether the file holds measurements, results or both.
          <p className="export-item__where export-item__file" data-ui="export.mdat.file">
            <Icon name="file" size={16} />
            <span>{mdatFileName(mdatScopeFor(view.results.calibration, view.mdatScope), view.settings)}</span>
          </p>
        )}
        {kind === "camilla_dsp" && <CamillaDevicesDisclosure view={view} actions={actions} defaultOpen={initialDevices} />}
        {/* 파일만 or 적용, under the row's devices and folder: the setting before the results below. */}
        {target !== undefined && <ApplyBlock target={target} view={view} actions={actions} initialConfirm={initialRevertConfirm} />}
      </div>
      <div className="export-item__action">
        {applying && target === "eqapo" && (
          // The row's own primary: Equalizer APO's 적용 or 관리, off with the reason while it is not installed or the service cannot take it.
          <Button
            data-ui="export.apply.eqapo"
            busy={view.apply.applying === "eqapo"}
            disabled={!canApply(view, "eqapo")}
            aria-describedby={blocked ? blockedId : eqapoReason(view)}
            onClick={actions.applyEqapo}
          >
            {t(level === "manage" ? "export.manage.eqapo.run" : "export.apply.eqapo.run")}
          </Button>
        )}
        {text.run || applying ? (
          // With 적용 the export stays: CamillaDSP's moves from the header to its row; Equalizer APO's goes quiet beside 적용.
          <Button
            variant={applying && target === "eqapo" ? "quiet" : "secondary"}
            data-ui={`export.run.${kind}`}
            busy={running}
            disabled={!canExport(view, kind) || block !== null || problem !== null}
            aria-describedby={describedBy}
            onClick={() => actions.exportFiles(kind)}
          >
            {t(text.run ?? "export.kind.camilla_dsp.run")}
          </Button>
        ) : running ? (
          <StatusPill tone="busy">{t("export.run.busy")}</StatusPill>
        ) : null}
      </div>
      {blocked && (
        // The format's scope, not a fault: the info icon and the muted text, never the warning hue.
        // A result restored from an older cache lifts with a new correction: the action follows its sentence.
        <div className="export-item__blocked">
          <p className="export-item__blocked-text" id={blockedId} data-ui={`export.kind.${kind}.blocked`}>
            <Icon name="info" size={18} />
            <span>{t(blocked)}</span>
          </p>
          {block === "stale_cache" && (
            // Both mdat rows carry one: each is named for its format and described by its sentence.
            // Off while an export runs, so the result does not change under it.
            <Button
              size="sm"
              className="export-item__blocked-action"
              id={recalibrateId}
              data-ui={`export.kind.${kind}.recalibrate`}
              aria-labelledby={`${recalibrateId} ${titleId}`}
              aria-describedby={blockedId}
              busy={view.recalibrating}
              disabled={view.exporting !== null}
              onClick={actions.recalibrate}
            >
              {t("export.blocked.mdat.recalibrate")}
            </Button>
          )}
        </div>
      )}
      {run && <Outcome kind={kind} run={run} view={view} actions={actions} />}
    </section>
  );
}

function Outcome({ kind, run, view, actions }: { kind: ExportKind; run: ExportRun; view: ExportView; actions: ExportActions }) {
  const t = useT();
  const id = `export.outcome.${kind}`;
  if (run.status === "failed") {
    const problem = designNameRejection(run.error);
    if (problem) {
      return (
        <div className="export-item__outcome" data-ui={id}>
          <ErrorNotice cause={t(`export.design_name.problem.${problem}`)} code={run.error.code} />
        </div>
      );
    }
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
  const hint = OUTCOME_HINT[kind];
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
      {hint && (
        // What to do with the folder: Equalizer APO's files go into its own config folder; the convolution IR's guide says which file goes where.
        <p className="export-done__hint" data-ui={`${id}.hint`}>
          {t(hint, { mode: filterModeName(view.settings) })}
        </p>
      )}
    </div>
  );
}
