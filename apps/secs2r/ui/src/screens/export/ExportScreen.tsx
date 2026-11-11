import { HelpTip } from "../../ui/HelpTip";
// 내보내기: write what the DSP, the player and REW need. The result on top
// (mode, delay, channels, MIMO, RC range), then one card of destinations as
// tabs: CamillaDSP, Equalizer APO, the convolution IR, the filter values as
// text, and REW. A tab names the place an export goes to and what that
// place is; its panel holds that export alone: its one filled action beside
// its title, the design name (F21), the folder and what lands in it, its
// options (CamillaDSP's devices, F20; the convolution IR's extra rates; what
// the REW mdat carries, F19, with the staged mdat under 고급, U3) and how
// it ended. A format that cannot carry the result says why in its panel
// (upstream 1.3.0 `_filter_card`), and its tab shows it.
// The CamillaDSP and Equalizer APO panels choose 파일만, 적용 or 관리 (ADR
// 0018, apply.tsx): with 적용 the filled action applies and the export stays
// beside it as a secondary button. With 관리 the same action applies and
// then hands the design to the SECS 2R Host service.
// No 완료 button (U4): the step is done once the current result is exported.
import { useEffect, useId, useRef, type KeyboardEvent, type ReactNode } from "react";
import { errorText } from "../../app/jobs/jobText";
import type { MessageKey } from "../../i18n";
import { useNumber, useT, type Translate } from "../../i18n/react";
import type { CalibrationSummary } from "../../ipc/bindings/CalibrationSummary";
import type { ExportBlock } from "../../ipc/bindings/ExportBlock";
import type { ExportKind } from "../../ipc/bindings/ExportKind";
import type { MdatScope } from "../../ipc/bindings/MdatScope";
import type { Mode } from "../../ipc/bindings/Mode";
import type { Notice } from "../../ipc/bindings/Notice";
import { Advanced, Button, Card, EmptyState, ErrorNotice, Icon, Segmented, Spinner, StatusPill, type PillTone } from "../../ui";
import type { IconName } from "../../ui/Icon";
import { roleCode } from "../../ui/maps/geometry";
import { noticeText } from "../calibrate/text";
import { DESIGN_RATE, rateText } from "../system/outputRate";
import { ApplyBlock, ApplyOutcome, eqapoReason } from "./apply";
import { CamillaDevicesDisclosure, DESIGN_NAME_ID, DesignNameField, ExtraRatesDisclosure } from "./fields";
import {
  EXPORT_KINDS,
  EXPORT_TABS,
  MDAT_SCOPES,
  TAB_KINDS,
  TAB_TARGET,
  applyLevel,
  applyProblem,
  canApply,
  canExport,
  designNameFor,
  designNameRejection,
  exportBlock,
  exportStepState,
  filterModeName,
  inputProblem,
  isFilterKind,
  mdatFileName,
  mdatScopeFor,
  mimoInclusion,
  rateFallback,
  tabState,
  type ApplyProblem,
  type ApplyTarget,
  type ExportActions,
  type ExportRun,
  type ExportTab,
  type ExportView,
  type HostProblem,
  type InputProblem,
  type MimoInclusion,
  type ScreenApplyLevel,
  type TabState,
} from "./types";
import "./export.css";

const MODE_TEXT: Record<Exclude<Mode, "ultra">, MessageKey> = {
  zero: "export.mode.zero",
  mixed: "export.mode.mixed",
  linear: "export.mode.linear",
};

const KIND_TEXT: Record<ExportKind, { title: MessageKey; body: MessageKey; run: MessageKey; exporting: MessageKey }> = {
  camilla_dsp: { title: "export.kind.camilla_dsp", body: "export.kind.camilla_dsp.body", run: "export.kind.camilla_dsp.run", exporting: "export.status.exporting.camilla_dsp" },
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

/** A tab: the place the export goes to, what that place is, and its sign. */
const TAB_TEXT: Record<ExportTab, { name: MessageKey; use: MessageKey; icon: IconName }> = {
  camilla_dsp: { name: "export.tab.camilla_dsp", use: "export.tab.camilla_dsp.use", icon: "chip" },
  equalizer_apo: { name: "export.tab.equalizer_apo", use: "export.tab.equalizer_apo.use", icon: "sliders" },
  convolution: { name: "export.tab.convolution", use: "export.tab.convolution.use", icon: "impulse" },
  filter_text: { name: "export.tab.filter_text", use: "export.tab.filter_text.use", icon: "text" },
  rew: { name: "export.tab.rew", use: "export.tab.rew.use", icon: "axes" },
};

/**
 * What happened at a tab, as a mark that needs no legend: a spinner while it
 * runs, the check the panel's own outcome carries once it is saved or
 * applied, the warning sign when it failed; with the words a screen reader
 * hears after the tab's name. A tab the result cannot go to carries no mark
 * (a grey dot there told a reader nothing): its caption says so in words.
 */
type TabMarkState = Exclude<TabState, "idle" | "blocked">;
const TAB_MARK: Record<TabMarkState, { text: MessageKey; icon: IconName | null }> = {
  busy: { text: "export.tab.state.busy", icon: null },
  done: { text: "export.tab.state.done", icon: "check" },
  failed: { text: "export.tab.state.failed", icon: "warning" },
};

/** The filled action of a target set beyond 파일만. */
const APPLY_RUN: Record<ApplyTarget, Record<Exclude<ScreenApplyLevel, "files">, MessageKey>> = {
  camilla: { apply: "export.apply.camilla.run", manage: "export.manage.camilla.run" },
  eqapo: { apply: "export.apply.eqapo.run", manage: "export.manage.eqapo.run" },
};

/** The one panel under the tabs. */
const PANEL_ID = "export.panel";
/** The step header's pill (`StepHeader`): an action held off by a field on this screen, or by another job, points at it. */
const HEADER_STATUS_ID = "frame.header.export.status";
/** The blocked step's reason in the step header: an action off for want of a result points at it. */
const HEADER_BLOCKED_ID = "frame.header.export.blocked";
/** The hint under the REW mdat's contents (`Segmented`): without a result it says what can be saved, or that nothing can. */
const MDAT_SCOPE_HINT_ID = "export.mdat.scope-hint";

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

/**
 * One RC notice from `result_lines().rc` as a phrase; numbers arrive as
 * strings and are rounded to whole Hz and m³. The upper limit is the 보정
 * screen's RC line: its basis and the evidence for it (ADR 0021).
 */
export function rcText(t: Translate, notice: Notice, number: (value: number, digits?: number) => string): string {
  const p = notice.params;
  const whole = (value: string | undefined) => (value === undefined || Number.isNaN(Number(value)) ? value ?? "" : number(Number(value)));
  switch (notice.code) {
    case "rc_limit":
      return noticeText(t, number, notice) ?? t("jobs.log.unknown");
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
  initialExtraRates = false,
  initialRevertConfirm = false,
}: {
  view: ExportView;
  actions: ExportActions;
  /** Gallery: draw 고급 unfolded. The fold is the screen's own state. */
  initialAdvanced?: boolean;
  /** Gallery: draw the CamillaDSP devices unfolded. The fold is the screen's own state. */
  initialDevices?: boolean;
  /** Gallery: draw the convolution IR's extra rates unfolded. The fold is the screen's own state. */
  initialExtraRates?: boolean;
  /** Gallery and tests: open with 빼기's confirm showing. The confirm is the screen's own state. */
  initialRevertConfirm?: boolean;
}) {
  const rootRef = useRef<HTMLDivElement>(null);
  const runsRef = useRef(view.runs);
  const applyRunsRef = useRef(view.apply.runs);
  // A run that ends brings its outcome into view inside the content's scroll
  // area (a long panel may hold it below the fold); the first draw does not scroll.
  useEffect(() => {
    const before = runsRef.current;
    runsRef.current = view.runs;
    const changed = EXPORT_KINDS.find((kind) => view.runs[kind] && view.runs[kind] !== before[kind]);
    const el = changed ? rootRef.current?.querySelector<HTMLElement>(`[data-ui="export.outcome.${changed}"]`) : null;
    if (el) revealInside(el);
  }, [view.runs]);
  // The same for an apply.
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
      <FilesCard view={view} actions={actions} initialAdvanced={initialAdvanced} initialDevices={initialDevices} initialExtraRates={initialExtraRates} initialRevertConfirm={initialRevertConfirm} />
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
 * What the tab on show needs changed before its filled action can start: for
 * a target set to 적용 or 관리 what holds the apply off, else what holds
 * the export off.
 */
function tabProblem(view: ExportView): InputProblem | ApplyProblem | null {
  const target = TAB_TARGET[view.tab];
  if (target !== undefined && applyLevel(view, target) !== "files") return applyProblem(view, target);
  return inputProblem(view, TAB_KINDS[view.tab][0]);
}

/**
 * Where the frame says why an action is off when nothing in its panel holds
 * it: the pill names the export or the apply that runs (jobs run one at a
 * time), and the blocked step's reason says there is no result yet. The REW
 * mdat without a result is off only when nothing was measured, which the
 * hint under its contents says.
 */
function heldReasonId(view: ExportView, kind?: ExportKind): string | undefined {
  if (view.exporting !== null || view.apply.applying !== null) return HEADER_STATUS_ID;
  if (kind === "before_after_mdat" && !view.results.calibration) return MDAT_SCOPE_HINT_ID;
  return exportStepState(view).status === "blocked" ? HEADER_BLOCKED_ID : undefined;
}

/** Where the screen says why `target`'s apply is off: Equalizer APO's own sentence or the header's pill for a field, else what holds every action. */
function applyReasonId(view: ExportView, target: ApplyTarget): string | undefined {
  const problem = applyProblem(view, target);
  if (problem) return (target === "eqapo" ? eqapoReason(view) : undefined) ?? (problemStatus(problem) ? HEADER_STATUS_ID : undefined);
  return canApply(view, target) || view.apply.applying === target ? undefined : heldReasonId(view);
}

/**
 * The step header carries no action here: each tab's filled action stands
 * beside its title. The pill says what runs, what the tab on show needs
 * changed before its action can start (the field under it says what to
 * change), that an export or an apply failed (it may be in another tab),
 * that the result is stale, or that the step is done.
 */
export function exportHeader(view: ExportView, t: Translate): { status?: ReactNode } {
  const state = exportStepState(view);
  const problem = tabProblem(view);
  const problemPill = problem ? problemStatus(problem) : null;
  const applyingTarget = view.apply.applying;
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
  return { status };
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

/* ── The destinations ───────────────────────────────────────────────────── */

function FilesCard({
  view,
  actions,
  initialAdvanced,
  initialDevices,
  initialExtraRates,
  initialRevertConfirm,
}: {
  view: ExportView;
  actions: ExportActions;
  initialAdvanced: boolean;
  initialDevices: boolean;
  initialExtraRates: boolean;
  initialRevertConfirm: boolean;
}) {
  const t = useT();
  const tab = view.tab;
  return (
    <Card title={t("export.files.title")} data-ui="export.files">
      <DestinationTabs view={view} actions={actions} />
      {/* One panel, drawn anew per tab: a fold or a confirm left open in one tab does not carry into the next. */}
      <div key={tab} className="export-panel" role="tabpanel" id={PANEL_ID} data-ui={PANEL_ID} data-tab={tab} aria-labelledby={`export.tab.${tab}`}>
        {tab === "rew" ? (
          <>
            <ExportItem kind="before_after_mdat" main view={view} actions={actions} />
            <Advanced id="export.advanced" count={1} defaultOpen={initialAdvanced}>
              <ExportItem kind="staged_mdat" view={view} actions={actions} />
            </Advanced>
          </>
        ) : (
          <ExportItem kind={tab} main view={view} actions={actions} initialOpen={tab === "camilla_dsp" ? initialDevices : initialExtraRates} initialRevertConfirm={initialRevertConfirm} />
        )}
      </div>
    </Card>
  );
}

/**
 * The destinations as tabs, one tab stop: the arrows, Home and End move and
 * choose. A tab reads its place's name, what that place is, and, once
 * something happened there, the mark of it with its words for a screen
 * reader. A place the result cannot go to says so where it said what it is.
 */
function DestinationTabs({ view, actions }: { view: ExportView; actions: ExportActions }) {
  const t = useT();
  const refs = useRef<Partial<Record<ExportTab, HTMLButtonElement | null>>>({});
  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    const index = EXPORT_TABS.indexOf(view.tab);
    const last = EXPORT_TABS.length - 1;
    const to =
      event.key === "ArrowRight" ? (index === last ? 0 : index + 1) : event.key === "ArrowLeft" ? (index === 0 ? last : index - 1) : event.key === "Home" ? 0 : event.key === "End" ? last : null;
    if (to === null) return;
    event.preventDefault();
    const tab = EXPORT_TABS[to]!;
    actions.selectTab(tab);
    refs.current[tab]?.focus();
  };
  return (
    <div className="export-tabs" role="tablist" aria-label={t("export.files.title")} data-ui="export.tabs" onKeyDown={onKeyDown}>
      {EXPORT_TABS.map((tab) => {
        const selected = tab === view.tab;
        const state = tabState(view, tab);
        const text = TAB_TEXT[tab];
        return (
          <button
            key={tab}
            ref={(node) => {
              refs.current[tab] = node;
            }}
            type="button"
            role="tab"
            id={`export.tab.${tab}`}
            data-ui={`export.tab.${tab}`}
            data-state={state}
            aria-selected={selected}
            aria-controls={selected ? PANEL_ID : undefined}
            tabIndex={selected ? 0 : -1}
            className="export-tab"
            onClick={() => actions.selectTab(tab)}
          >
            <Icon name={text.icon} size={20} className="export-tab__icon" />
            <span className="export-tab__title">
              <span className="export-tab__name">{t(text.name)}</span>
              {state !== "idle" && state !== "blocked" && <TabMark state={state} />}
            </span>
            <span className="export-tab__use">{t(state === "blocked" ? "export.tab.state.blocked" : text.use)}</span>
          </button>
        );
      })}
    </div>
  );
}

function TabMark({ state }: { state: TabMarkState }) {
  const t = useT();
  const mark = TAB_MARK[state];
  return (
    <span className={`export-tab__mark export-tab__mark--${state}`}>
      {mark.icon ? <Icon name={mark.icon} size={16} weight="semibold" /> : <Spinner size={14} />}
      <span className="visually-hidden">{t(mark.text)}</span>
    </span>
  );
}

/**
 * The export folder on one line: its name, the path, then the two buttons
 * right after the path. Where the folder comes from (the default beside the
 * project, or the one chosen for this project) opens from the name's help.
 */
function FolderRow({ view, actions }: { view: ExportView; actions: ExportActions }) {
  const t = useT();
  const folder = view.folder;
  return (
    <div className="export-folder" data-ui="export.folder">
      <div className="ui-heading-help export-folder__name">
        <span className="export-folder__label" id="export.folder.label">
          {t("export.folder")}
        </span>
        {folder && <HelpTip id="export.folder.help" labelledBy="export.folder.label" content={t(folder.stored ? "export.folder.stored" : "export.folder.default")} />}
      </div>
      <div className="export-folder__value">
        <span className="export-path" data-ui="export.folder.path">
          {folder ? folder.folder : <span className="muted">{t("export.folder.loading")}</span>}
        </span>
        <span className="export-folder__actions">
          <Button size="sm" data-ui="export.folder.change" disabled={view.exporting !== null} onClick={actions.changeFolder}>
            {t("export.folder.change")}
          </Button>
          <Button size="sm" variant="quiet" icon="folder" data-ui="export.folder.open" disabled={!folder} onClick={() => folder && actions.openFolder(folder.folder)}>
            {t("export.folder.open")}
          </Button>
        </span>
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

/**
 * The rate a filter export writes, said under its folder when it is not
 * 48 kHz (ADR 0017); 자동 says it reads the playback device. The REW mdat
 * stays at 48 kHz and says nothing.
 */
function RateLine({ kind, view }: { kind: ExportKind; view: ExportView }) {
  const t = useT();
  const number = useNumber();
  const choice = view.outputRate;
  if (!isFilterKind(kind) || !choice || (choice !== "auto" && choice.fixed === DESIGN_RATE)) return null;
  return (
    <p className="export-item__where export-item__rate" data-ui={`export.kind.${kind}.rate`}>
      <Icon name="settings" size={16} />
      <span>{choice === "auto" ? t("export.rate.auto") : t("export.rate.fixed", { rate: rateText(number, choice.fixed) })}</span>
    </p>
  );
}

/** The exports that choose 파일만, 적용 or 관리 (ADR 0018), and the target each applies to. */
const APPLY_TARGET: Partial<Record<ExportKind, ApplyTarget>> = { camilla_dsp: "camilla", equalizer_apo: "eqapo" };

/**
 * One export. A tab's main one (`main`) opens its panel: its filled action
 * beside its title, right under them why it is off or how it last ended,
 * then how it gets there (적용), what is written, the design name, and the
 * folder with what lands in it. The staged mdat under 고급 is the one
 * export that is not: a secondary button, no fields of its own.
 */
function ExportItem({
  kind,
  main = false,
  view,
  actions,
  initialOpen = false,
  initialRevertConfirm = false,
}: {
  kind: ExportKind;
  main?: boolean;
  view: ExportView;
  actions: ExportActions;
  initialOpen?: boolean;
  initialRevertConfirm?: boolean;
}) {
  const t = useT();
  const text = KIND_TEXT[kind];
  const run = view.runs[kind];
  const running = view.exporting === kind;
  // The tutorial writes nothing, so it applies nothing either.
  const target = view.tutorial ? undefined : APPLY_TARGET[kind];
  const level = target === undefined ? "files" : applyLevel(view, target);
  const titleId = `export.kind.${kind}.title`;
  const block = exportBlock(view, kind);
  const blocked = block ? BLOCK_TEXT[kind]?.[block] : undefined;
  const blockedId = useId();
  const recalibrateId = useId();
  const scoped = kind === "before_after_mdat";
  // A design name that cannot be a folder or an ASIO side without a driver holds the export off.
  const problem = inputProblem(view, kind);
  const off = !canExport(view, kind) || block !== null || problem !== null;
  // Where the screen says why: the format's sentence, the name field's message, the header's pill, else what holds every action.
  const exportReason = blocked ? blockedId : problem?.field === "design_name" ? `${DESIGN_NAME_ID}-error` : problem ? HEADER_STATUS_ID : off && !running ? heldReasonId(view, kind) : undefined;
  // The export folder stands in a tab's main export; the tutorial has none, and the staged mdat goes straight into it.
  const placed = main && !view.tutorial;
  // How the last apply ended, while the target is set beyond 파일만.
  const applied = target !== undefined && level !== "files" ? view.apply.runs[target] : undefined;
  const Title = main ? "h3" : "h4";
  return (
    <section className={scoped ? "export-item export-item--scoped" : "export-item"} data-ui={`export.kind.${kind}`} aria-labelledby={titleId}>
      <div className="ui-heading-help">
        <Title className="export-item__title" id={titleId}>{t(text.title)}</Title>
        <HelpTip id={`export.kind.${kind}.help`} labelledBy={titleId} content={t(text.body)} />
      </div>
      <div className="export-item__action">
        {target !== undefined && level !== "files" && (
          // Beyond 파일만 the filled action applies, off with the reason while something on this screen holds it.
          <Button
            variant="primary"
            data-ui={`export.apply.${target}`}
            busy={view.apply.applying === target}
            disabled={!canApply(view, target)}
            aria-describedby={blocked ? blockedId : applyReasonId(view, target)}
            onClick={target === "camilla" ? actions.applyCamilla : actions.applyEqapo}
          >
            {t(APPLY_RUN[target][level])}
          </Button>
        )}
        {/* The export: the panel's filled action, or beside 적용 a secondary one. CamillaDSP's keeps the name the tutorial calls it by. */}
        <Button
          variant={main && level === "files" ? "primary" : "secondary"}
          data-ui={`export.run.${kind}`}
          busy={running}
          disabled={off}
          aria-describedby={exportReason}
          onClick={() => actions.exportFiles(kind)}
        >
          {t(kind === "camilla_dsp" && level === "files" ? "frame.step.export.primary" : text.run)}
        </Button>
      </div>
      {/* What the actions answer with stands right under them: why they are off, then how the last apply and the last export ended. */}
      {blocked && (
        // The format's scope, not a fault: the info icon and the muted text, never the warning hue.
        // A result restored from an older cache lifts with a new correction: the action follows its sentence.
        <div className="export-item__blocked">
          <p className="export-item__blocked-text" id={blockedId} data-ui={`export.kind.${kind}.blocked`}>
            <Icon name="info" size={18} />
            <span>{t(blocked)}</span>
          </p>
          {block === "stale_cache" && (
            // Both mdat exports carry one: each is named for its format and described by its sentence.
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
      {applied && <ApplyOutcome target={target!} run={applied} view={view} actions={actions} />}
      {run && <Outcome kind={kind} run={run} view={view} actions={actions} />}
      {/* How it gets there comes first: 파일만, 적용 or 관리, with what 적용 reaches. */}
      {target !== undefined && <ApplyBlock target={target} view={view} actions={actions} initialConfirm={initialRevertConfirm} />}
      {/* Then what is written: the mdat's contents, CamillaDSP's devices, the convolution IR's extra rates. */}
      {scoped && <MdatScopeField view={view} actions={actions} />}
      {kind === "camilla_dsp" && <CamillaDevicesDisclosure view={view} actions={actions} defaultOpen={initialOpen} />}
      {kind === "convolution" && <ExtraRatesDisclosure view={view} actions={actions} defaultOpen={initialOpen} />}
      {/* One name for every filter export; the tutorial has no folder to name. */}
      {main && isFilterKind(kind) && !view.tutorial && <DesignNameField view={view} actions={actions} />}
      {/* Where the files go, read as one: the export folder, what lands in it, and at what rate. */}
      <div className={placed ? "export-item__place" : "export-item__place export-item__place--bare"}>
        {placed && <FolderRow view={view} actions={actions} />}
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
        <RateLine kind={kind} view={view} />
      </div>
    </section>
  );
}

/** The `output_rate_fallback` note as a cause: the rate the device reported when there was one, else that it could not be read. */
function fallbackText(t: Translate, number: (value: number, digits?: number) => string, note: Notice): string {
  const rate = Number(note.params.rate);
  return note.params.rate !== undefined && Number.isFinite(rate) && rate > 0
    ? t("export.rate.fallback.unsupported", { rate: rateText(number, rate) })
    : t("export.rate.fallback.unread");
}

function Outcome({ kind, run, view, actions }: { kind: ExportKind; run: ExportRun; view: ExportView; actions: ExportActions }) {
  const t = useT();
  const number = useNumber();
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
  const fallback = rateFallback(run);
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
      {fallback ? (
        // 자동 could not use the playback device's rate: what was written instead, and where the rate is set.
        <ErrorNotice
          id={`${id}.rate_fallback`}
          tone="warning"
          cause={fallbackText(t, number, fallback)}
          next={t("export.rate.fallback.next")}
          action={
            <Button variant="secondary" size="sm" iconEnd="arrowRight" data-ui={`${id}.go_system`} onClick={actions.goToSystem}>
              {t("export.go_system")}
            </Button>
          }
        />
      ) : view.outputRate === "auto" && isFilterKind(kind) ? (
        // 자동 decided the rate when exporting: the row said it would read the device, the outcome says what it read.
        <p className="export-done__hint tabular" data-ui={`${id}.rate`}>
          {t("export.rate.written", { rate: rateText(number, run.rate) })}
        </p>
      ) : null}
      {hint && (
        // What to do with the folder: Equalizer APO's files go into its own config folder; the convolution IR's guide says which file goes where.
        <p className="export-done__hint" data-ui={`${id}.hint`}>
          {t(hint, { mode: filterModeName(view.settings) })}
        </p>
      )}
    </div>
  );
}
