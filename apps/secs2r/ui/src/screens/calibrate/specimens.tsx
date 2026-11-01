// Gallery specimens of the 보정 screen: every state in a 1280×800 frame,
// the densest one again at 1024×700. Settings states scroll the frame's
// content to their section so the screenshot shows it.
import { useLayoutEffect, useRef, type ReactNode } from "react";
import { initialLedger, type LedgerAction, type LedgerState } from "../../state/jobLedger";
import type { CalibrationState, FrameModel } from "../../app/model";
import { PrefsScope, usePrefs } from "../../app/prefs";
import { FrameBox, T0, replay } from "../../gallery/frameSpecimens";
import type { SpecimenDef } from "../../gallery/Specimen";
import { useNumber, useT } from "../../i18n/react";
import { Card } from "../../ui";
import { CalibrateScreen, calibrateHeader, type CalibrateScreenProps } from "./CalibrateScreen";
import * as fx from "./fixtures";
import { noticeText } from "./text";
import { calibrateStepState, type CalibrateActions, type CalibrateView } from "./types";

const noop = () => {};
export const noActions: CalibrateActions = {
  goToMeasurements: noop,
  goToExport: noop,
  calibrate: noop,
  calibrateSubs: noop,
  recommendXo: noop,
  computeMimo: noop,
  changeCompute: noop,
  refreshCompute: noop,
  openGpuHelp: noop,
  selectTab: noop,
  selectChannel: noop,
  selectXoGroup: noop,
  previewXo: noop,
  changeSetting: async () => true,
  takeSubValues: noop,
  setTargetPoints: async () => true,
  applyTargetPreset: noop,
  saveTarget: noop,
  loadTarget: noop,
};

function calibrationState(view: CalibrateView): CalibrationState {
  if (view.running === "calibrate") return "computing";
  if (view.failure?.job === "calibrate") return "failed";
  const result = view.results.calibration;
  if (!result) return "none";
  return result.stale ? "stale" : "done";
}

function frameOf(view: CalibrateView, fileName = "거실 7.1.4.secs"): FrameModel {
  const result = view.results.calibration;
  return {
    current: "calibrate",
    project: {
      preset: view.config.preset,
      subCount: view.config.summary.sub_count,
      outputs: view.config.summary.outputs.length,
      positionsDone: view.positions.length,
      positionsTotal: 9,
      calibration: calibrationState(view),
      path: { kind: "rew", state: "ok" },
      fileName,
      dirty: true,
    },
    steps: {
      system: { status: "done" },
      connect: { status: "done" },
      measure: { status: "ready" },
      calibrate: calibrateStepState(view),
      export: result ? { status: "ready" } : view.running === "calibrate" ? { status: "blocked", reason: "computing" } : { status: "blocked", reason: "no_result" },
    },
  };
}

const runningJobs = (kind: "calibrate" | "mimo", stage: string, total: number): LedgerState =>
  replay([
    { type: "event", at: T0, event: { event: "started", job: 7, kind } },
    { type: "event", at: T0 + 3000, event: { event: "progress", job: 7, stage, done: 2, total } },
    { type: "event", at: T0 + 12000, event: { event: "progress", job: 7, stage, done: 5, total } },
  ]);

const failedJobs = replay([
  { type: "event", at: T0, event: { event: "started", job: 7, kind: "calibrate" } },
  {
    type: "event",
    at: T0 + 9000,
    event: { event: "finished", job: 7, status: "failed", result: null, error: fx.failedView.failure!.error },
  },
] satisfies LedgerAction[]);

/** 보정 계산 with MIMO that failed in its MIMO part (`stage: "mimo"`): the tray's entry says the correction finished. */
const mimoFailedJobs = replay([
  { type: "event", at: T0, event: { event: "started", job: 7, kind: "calibrate" } },
  { type: "event", at: T0 + 24000, event: { event: "finished", job: 7, status: "failed", result: null, error: fx.mimoStageError } },
] satisfies LedgerAction[]);

/** Scrolls the frame's content so `target` sits at the top (settings specimens). */
function ScrollTo({ target }: { target: string }) {
  const marker = useRef<HTMLSpanElement>(null);
  useLayoutEffect(() => {
    const content = marker.current?.closest<HTMLElement>(".app-content");
    const go = () => {
      const element = content?.querySelector<HTMLElement>(`[data-ui="${target}"]`);
      if (!content || !element) return;
      content.scrollTop += element.getBoundingClientRect().top - content.getBoundingClientRect().top - 16;
    };
    go();
    const frame = requestAnimationFrame(go);
    void document.fonts?.ready.then(go);
    return () => cancelAnimationFrame(frame);
  }, [target]);
  return <span ref={marker} hidden />;
}

function CalibrateFrame({
  view,
  jobs = initialLedger,
  scrollTo,
  initial,
  fileName,
}: {
  view: CalibrateView;
  jobs?: LedgerState;
  scrollTo?: string | undefined;
  initial?: CalibrateScreenProps["initial"];
  fileName?: string | undefined;
}) {
  const t = useT();
  return (
    <FrameBox frame={frameOf(view, fileName)} jobs={jobs} header={calibrateHeader(view, t)}>
      <CalibrateScreen view={view} actions={noActions} initial={initial} />
      {scrollTo && <ScrollTo target={scrollTo} />}
    </FrameBox>
  );
}

/** A themed scope with changed prefs (tutorial accent) around a frame. */
function Scoped({ tutorial, children }: { tutorial?: boolean; children: ReactNode }) {
  const outer = usePrefs();
  const prefs = { locale: outer.locale, theme: outer.theme, accent: tutorial ? ("tutorial" as const) : outer.accent, ...(outer.skin ? { skin: outer.skin } : {}) };
  return (
    <PrefsScope key={`${outer.locale}-${outer.theme}-${outer.skin ?? "default"}`} initial={prefs} className="gal-fill">
      {children}
    </PrefsScope>
  );
}

const frame = (id: string, title: string, render: () => ReactNode): SpecimenDef => ({ id: `screen.calibrate.${id}`, title: `보정 · ${title}`, width: 1280, height: 800, render });

/** The range card on each upper-limit basis (ADR 0021), its RC line from the mock's calibration. */
const RANGE_SPECIMENS: [string, string, CalibrateView][] = [
  ["strict", "strict Schroeder ×1 (default) with the room volume and the transition", fx.rangeViews.strict],
  ["strict-x2", "strict Schroeder ×2", fx.rangeViews.strictTimes2],
  ["strict-x4", "strict Schroeder ×4", fx.rangeViews.strictTimes4],
  ["lenient", "lenient Schroeder, its band's T60 in the line", fx.rangeViews.lenient],
  ["lenient-x2", "lenient Schroeder ×2", fx.rangeViews.lenientTimes2],
  ["agreement", "position agreement at nine positions: no room volume, the time it takes", fx.rangeViews.agreement],
  ["agreement-unavailable", "two measured positions: agreement off, with the reason", fx.rangeViews.agreementUnavailable],
  ["agreement-short", "agreement chosen, a position cleared since: still chosen, with what to do", fx.rangeViews.agreementShort],
  ["manual", "직접 입력: the number field", fx.rangeViews.manual],
  ["no-volume", "strict Schroeder without a room volume: 500 Hz and the hint to enter it", fx.rangeViews.noVolume],
];

/** Every kind of RC line in the range card's list. */
function RcLines() {
  const t = useT();
  const number = useNumber();
  return (
    <Card title={t("calibrate.range.title")} className="calibrate-settings__card">
      <ul className="calibrate-notes-list" data-ui="calibrate.range.notes">
        {fx.rcLimitNotes.map((note, i) => (
          <li key={i}>{noticeText(t, number, note)}</li>
        ))}
      </ul>
    </Card>
  );
}

/** The merged band set line (ADR 0024) for one extra band, two bands, and a band merged with the RC band. */
function RcBandsLines() {
  const t = useT();
  const number = useNumber();
  return (
    <Card title={t("calibrate.verdict.title")} className="calibrate-settings__card">
      <ul className="calibrate-notes-list" data-ui="calibrate.verdict.rc_bands.lines">
        {fx.rcBandsNotes.map((note, i) => (
          <li key={i}>{noticeText(t, number, note)}</li>
        ))}
      </ul>
    </Card>
  );
}

/** The 극저역 보호 line for each case: a sub's high-pass, a sub keeping the user's, a main, and the warning. */
function LowProtectLines() {
  const t = useT();
  const number = useNumber();
  return (
    <Card title={t("calibrate.verdict.title")} className="calibrate-settings__card">
      <ul className="calibrate-notes-list" data-ui="calibrate.verdict.low_protect.lines">
        {fx.lowProtectNotes.map((note, i) => (
          <li key={i}>{noticeText(t, number, note)}</li>
        ))}
      </ul>
    </Card>
  );
}

/** The 저음 도착 맞추기 and 방 울림 줄이기 lines for each case (ADR 0025). */
function LfLines() {
  const t = useT();
  const number = useNumber();
  return (
    <Card title={t("calibrate.verdict.title")} className="calibrate-settings__card">
      <ul className="calibrate-notes-list" data-ui="calibrate.verdict.lf.lines">
        {fx.lfNoteCases.map((note, i) => (
          <li key={i}>{noticeText(t, number, note)}</li>
        ))}
      </ul>
    </Card>
  );
}

export const calibrateSpecimens: SpecimenDef[] = [
  frame("empty", "no result yet: plan, position-1 preview and target", () => <CalibrateFrame view={fx.emptyView} />),
  ...Object.entries(fx.planViews).map(([name, view]) => frame(`plan-${name}`, `calculation plan: ${name}`, () => <CalibrateFrame view={view} />)),
  frame("computing", "first run computing (busy primary, job in the tray)", () => <CalibrateFrame view={fx.computingView} jobs={runningJobs("calibrate", "engine.design", 11)} />),
  frame("ready-mimo", "before the first run, MIMO on with three positions: XO 추천 포함 and MIMO 함께 계산 right before 보정 계산, the plan names MIMO", () => <CalibrateFrame view={fx.readyMimoView} />),
  frame("ready-mimo-two", "before the first run with two positions: MIMO 함께 계산 off, its reason on its own line under it; 보정 계산 stays on the switches' line", () => <CalibrateFrame view={fx.readyMimoTwoView} />),
  frame("ready-reasons", "before the first run with two positions after a manual XO edit: each switch's reason under it, the plan and the button on the switches' line", () => <CalibrateFrame view={fx.readyReasonsView} />),
  frame("computing-reasons", "first run computing from two positions: the busy line as tall as the controls and their reasons", () => (
    <CalibrateFrame view={{ ...fx.readyReasonsView, running: "calibrate" }} jobs={runningJobs("calibrate", "engine.design", 11)} />
  )),
  {
    id: "screen.calibrate.ready-mimo.min",
    title: "보정 · XO 추천 포함 and MIMO 함께 계산 before the first run at the smallest window (1024 × 700), MIMO's reason under it",
    width: 1024,
    height: 700,
    render: () => <CalibrateFrame view={fx.readyMimoTwoView} />,
  },
  frame("computing-mimo", "first run computing the correction and MIMO: the busy line and the pill name both, the tray at the MIMO stage", () => (
    <CalibrateFrame view={fx.computingMimoView} jobs={runningJobs("calibrate", "engine.mimo_iir", 11)} />
  )),
  frame("mimo-failed", "MIMO failed after the correction: the result stays, the MIMO figure says 계산 실패", () => <CalibrateFrame view={fx.mimoFailedView} jobs={mimoFailedJobs} />),
  frame("mimo-failed-card", "MIMO failed after the correction: the MIMO card's notice, not the result card's", () => (
    <CalibrateFrame view={fx.mimoFailedView} jobs={mimoFailedJobs} scrollTo="calibrate.settings.mimo" />
  )),
  frame("mimo-stale-correction", "MIMO behind a changed correction: 다시 보정하기 computes both, MIMO만 계산 off", () => (
    <CalibrateFrame view={fx.mimoBehindCorrectionView} scrollTo="calibrate.settings.mimo" />
  )),
  frame("mimo-stale-settings", "MIMO behind its own settings on a current correction: MIMO만 계산", () => (
    <CalibrateFrame view={fx.mimoStaleView} scrollTo="calibrate.settings.mimo" />
  )),
  frame("spl", "current result, SPL with target handles and the RC band; XO 추천 포함 and MIMO 함께 계산 before 다시 보정하기", () => <CalibrateFrame view={fx.splView} />),
  // XO 추천 포함 and the stale recommendation (ADR 0028).
  frame("xo-rec-manual", "a manual XO edit turned XO 추천 포함 off: the reason under the result card's switch", () => <CalibrateFrame view={fx.xoRecManualOffView} />),
  frame("xo-rec-manual-card", "a manual XO edit turned XO 추천 포함 off: the crossover card's switch under the value, the reason under it", () => (
    <CalibrateFrame view={fx.xoRecManualOffView} scrollTo="calibrate.settings.xo" />
  )),
  frame("xo-rec-stale", "the recommendation computed for other settings: the values stay, a warning after them", () => (
    <CalibrateFrame view={fx.xoRecStaleView} scrollTo="calibrate.settings.xo" />
  )),
  {
    id: "screen.calibrate.xo-rec-manual.min",
    title: "보정 · XO 추천 포함 turned off by a manual edit at the smallest window (1024 × 700): the run wraps as a unit",
    width: 1024,
    height: 700,
    render: () => <CalibrateFrame view={fx.xoRecManualOffView} />,
  },
  frame("target-menu", "target menu open (presets, 사용자 지정, save, load)", () => <CalibrateFrame view={fx.splView} initial={{ targetMenu: true }} />),
  frame("gd", "current result, GD with the RC band", () => <CalibrateFrame view={fx.gdView} />),
  frame("xo", "current result, XO with the draggable line", () => <CalibrateFrame view={fx.xoView} />),
  frame("ba", "BA with the table and warnings", () => <CalibrateFrame view={fx.baView} />),
  frame("stale", "stale after a mode change", () => <CalibrateFrame view={fx.staleView} />),
  frame("no-position-1", "before a run, position 1 not measured: the channel's place says so", () => <CalibrateFrame view={fx.noPosition1View} />),
  frame("loading", "curves loading after a channel change: the last curves dimmed, the status over them", () => <CalibrateFrame view={fx.loadingView} />),
  frame("failed", "failed run: cause, next step, 자세히", () => <CalibrateFrame view={fx.failedView} jobs={failedJobs} />),
  frame("subs-only", "subs-only result on BA (DBA group)", () => <CalibrateFrame view={fx.subsOnlyView} />),
  frame("mimo", "MIMO computed: SPL with the MIMO curve", () => <CalibrateFrame view={fx.mimoView} />),
  frame("mimo-scorecard", "scorecard after MIMO: before → after → MIMO in each figure, the key and the late bass", () => (
    <CalibrateFrame view={fx.mimoView} scrollTo="calibrate.scorecard.card" />
  )),
  frame("late-positions", "위치별 늦은 저음 open: three positions, position 1 the reference, with MIMO", () => (
    <CalibrateFrame view={fx.mimoView} initial={{ latePositions: true }} scrollTo="calibrate.scorecard.positions" />
  )),
  frame("mimo-stale-scorecard", "MIMO stale: the scorecard compares before and after only", () => (
    <CalibrateFrame view={fx.mimoStaleView} scrollTo="calibrate.scorecard.card" />
  )),
  frame("one-position", "one measured position: late bass there, no positions table", () => (
    <CalibrateFrame view={fx.onePositionView} scrollTo="calibrate.scorecard.card" />
  )),
  {
    id: "screen.calibrate.mimo-scorecard.min",
    title: "보정 · scorecard after MIMO at the smallest window (1024 × 700): over the graph, figures on one line",
    width: 1024,
    height: 700,
    render: () => <CalibrateFrame view={fx.mimoView} scrollTo="calibrate.scorecard.card" />,
  },
  frame("mimo-settings", "MIMO section after a run", () => <CalibrateFrame view={fx.mimoView} scrollTo="calibrate.settings.mimo" />),
  frame("gpu-auto", "automatic GPU selection and actual GPU result", () => <CalibrateFrame view={fx.gpuView} scrollTo="calibrate.settings.mimo" />),
  frame("gpu-selected", "manual GPU preference", () => <CalibrateFrame view={fx.gpuSelectedView} scrollTo="calibrate.settings.mimo" />),
  frame("gpu-iir", "saved GPU preference with IIR calculated on CPU", () => <CalibrateFrame view={fx.gpuIirReferenceView} scrollTo="calibrate.settings.mimo" />),
  frame("gpu-cpu", "manual CPU preference and previous GPU result", () => <CalibrateFrame view={fx.cpuSelectedView} scrollTo="calibrate.settings.mimo" />),
  frame("gpu-request-unavailable", "saved GPU choice unavailable: select Automatic or CPU to recover", () => <CalibrateFrame view={fx.requestedGpuUnavailableView} scrollTo="calibrate.settings.mimo" />),
  frame("gpu-unavailable", "missing runtime: normal CPU path and setup help", () => <CalibrateFrame view={fx.runtimeMissingView} scrollTo="calibrate.settings.mimo" />),
  frame("gpu-running", "compute device locked during a job", () => <CalibrateFrame view={fx.runningMimoView} jobs={runningJobs("mimo", "engine.mimo_iir", 11)} scrollTo="calibrate.settings.mimo" />),
  frame("gpu-loading", "compute devices loading", () => <CalibrateFrame view={fx.computeLoadingView} scrollTo="calibrate.settings.mimo" />),
  frame("gpu-error", "compute status failed with retry", () => <CalibrateFrame view={fx.computeErrorView} scrollTo="calibrate.settings.mimo" />),
  frame("gpu-fallback", "GPU selected but previous result recovered on CPU", () => <CalibrateFrame view={fx.gpuFallbackView} scrollTo="calibrate.settings.mimo" />),
  { id: "screen.calibrate.gpu.min", title: "보정 · GPU at the smallest window", width: 1024, height: 700,
    render: () => <CalibrateFrame view={fx.gpuView} scrollTo="calibrate.settings.mimo" /> },
  frame("mimo-budget-96k", "MIMO FIR supports counted at the miniDSP's 96 kHz: past its FIR budget, the rate named in the line", () => (
    <CalibrateFrame view={fx.minidspView} scrollTo="calibrate.settings.mimo" />
  )),
  frame("mode-fir-over-96k", "mixed mode at the miniDSP's 96 kHz: the channel FIR past its taps per channel, one warning line", () => (
    <CalibrateFrame view={fx.minidspView} scrollTo="calibrate.settings.mode" />
  )),
  frame("mimo-running", "MIMO computing: 보정 계산 and the other jobs are off", () => (
    <CalibrateFrame view={fx.runningMimoView} jobs={runningJobs("mimo", "engine.mimo_iir", 11)} scrollTo="calibrate.settings.ba" />
  )),
  frame("settings", "settings: mode and correction range", () => <CalibrateFrame view={fx.splView} scrollTo="calibrate.settings" />),
  frame("no-subs", "no subs: the XO section says why", () => <CalibrateFrame view={fx.noSubsView} scrollTo="calibrate.settings.xo" />),
  frame("no-subs-run", "no subs: no XO 추천 포함 beside 다시 보정하기, only MIMO 함께 계산", () => <CalibrateFrame view={fx.noSubsView} />),
  frame("ba-settings", "BA settings with 고급 open on a fixed sub", () => <CalibrateFrame view={fx.baView} scrollTo="calibrate.settings.ba" initial={{ editSub: "SW2" }} />),
  frame("ba-editor", "BA 고급: values, fixed filters and MIMO support of a fixed sub", () => (
    <CalibrateFrame view={fx.baView} scrollTo="calibrate.ba.values" initial={{ editSub: "SW2" }} />
  )),
  frame("ultra", "초고지연 mode and its delay limit", () => <CalibrateFrame view={fx.ultraView} scrollTo="calibrate.settings.mode" />),
  ...RANGE_SPECIMENS.map(([id, title, view]) => frame(`range-${id}`, `RC upper limit: ${title}`, () => <CalibrateFrame view={view} scrollTo="calibrate.settings.range" />)),
  {
    id: "screen.calibrate.range.min",
    title: "보정 · RC upper limit at the smallest window (1024 × 700): lenient Schroeder ×2, the row wraps",
    width: 1024,
    height: 700,
    render: () => <CalibrateFrame view={fx.rangeViews.lenientTimes2} scrollTo="calibrate.settings.range" />,
  },
  { id: "screen.calibrate.rc-lines", title: "보정 · the RC line for each kind of evidence, then a missing volume and a note from before the bases", width: 720, render: () => <RcLines /> },
  // Extra correction bands (ADR 0024).
  frame("range-extra-none", "extra correction bands: none, only 보정 구간 추가 under the edges", () => (
    <CalibrateFrame view={fx.extraBandViews.none} scrollTo="calibrate.settings.range" />
  )),
  frame("range-extra-two", "two extra bands (2–4 kHz, 15–20 kHz): a row each with its edges and 빼기, then 보정 구간 추가", () => (
    <CalibrateFrame view={fx.extraBandViews.two} scrollTo="calibrate.settings.range" />
  )),
  frame("range-extra-refused", "the second band's upper edge refused (12000 Hz, below its lower edge): the reason at that input, the stored 20000 Hz in it", () => (
    <CalibrateFrame view={fx.extraBandViews.refused} scrollTo="calibrate.settings.range" />
  )),
  frame("range-extra-four", "four extra bands: the add action hidden", () => <CalibrateFrame view={fx.extraBandViews.four} scrollTo="calibrate.range.extra" />),
  frame("spl-extra-band", "SPL with an extra band at 15–20 kHz: shaded outside the RC band and it, its label left of its edge", () => <CalibrateFrame view={fx.extraBandViews.one} />),
  frame("spl-extra-bands", "SPL with two extra bands: shaded outside the RC band and both; a label that finds no room is left out", () => <CalibrateFrame view={fx.extraBandViews.two} />),
  frame("rc-bands-notes", "calculation notes open: the merged band set after an extra-band calculation", () => (
    <CalibrateFrame view={fx.extraBandViews.two} initial={{ notesOpen: true }} scrollTo="calibrate.verdict.notes" />
  )),
  { id: "screen.calibrate.rc-bands-lines", title: "보정 · the band set line for one extra band, two bands, and a band merged with the RC band", width: 720, render: () => <RcBandsLines /> },
  // 극저역 보호, 자동 설정 (ADR 0022) and 수동 설정 (the input cut, ADR 0031): one group in the range card's body, never under 고급.
  frame("low-protect-off", "극저역 보호 자동 and 수동 설정 off (the default) in the range card's body", () => (
    <CalibrateFrame view={fx.lowProtectViews.off} scrollTo="calibrate.range.low_end" />
  )),
  frame("low-protect-on", "극저역 보호 자동 설정 on, 고급 stays folded", () => <CalibrateFrame view={fx.lowProtectViews.on} scrollTo="calibrate.range.low_end" />),
  frame("input-hp-on", "극저역 보호 수동 설정 on at 27 Hz: its frequency under the switch", () => <CalibrateFrame view={fx.inputHpViews.on} scrollTo="calibrate.range.low_end" />),
  frame("input-hp-both", "극저역 보호 자동 and 수동 설정 both on", () => <CalibrateFrame view={fx.inputHpViews.both} scrollTo="calibrate.range.low_end" />),
  frame("input-hp-notes", "calculation notes open: the 수동 설정 line", () => (
    <CalibrateFrame view={fx.inputHpViews.on} initial={{ notesOpen: true }} scrollTo="calibrate.verdict.notes" />
  )),
  frame("low-protect-result", "극저역 보호 on with the RC lower limit at 15 Hz: the sub bus warning and the notes open, a line per channel", () => (
    <CalibrateFrame view={fx.lowProtectViews.unheld} initial={{ notesOpen: true }} />
  )),
  { id: "screen.calibrate.low-protect-lines", title: "보정 · the 극저역 보호 line for a sub's high-pass, a sub keeping the user's, a main, and the bus warning", width: 720, render: () => <LowProtectLines /> },
  // 저음 도착 맞추기, 고역 위상 맞추기 and 방 울림 줄이기 (ADR 0025, 0030): a card of their own between BA and MIMO, never under 고급.
  frame("lf-off", "저음 도착과 울림 off (the default): its card between BA and MIMO", () => <CalibrateFrame view={fx.lfViews.off} scrollTo="calibrate.settings.lf" />),
  frame("lf-on", "저음 도착 맞추기, 고역 위상 맞추기 and 방 울림 줄이기 on in 표준 mode", () => <CalibrateFrame view={fx.lfViews.on} scrollTo="calibrate.settings.lf" />),
  frame("lf-mixed", "저음 도착 맞추기 on in 저지연 mode: it aligns only part of the way, said under the switch", () => (
    <CalibrateFrame view={fx.lfViews.mixed} scrollTo="calibrate.settings.lf" />
  )),
  frame("lf-zero", "최소 지연 mode with all stored on: 저음 도착 맞추기 and 고역 위상 맞추기 off with the reason, 방 울림 줄이기 still on", () => (
    <CalibrateFrame view={fx.lfViews.zero} scrollTo="calibrate.settings.lf" />
  )),
  frame("lf-result", "all on in 표준 mode: the notes open, a 저음 도착 line, the cut modes and the unwound 고역 위상 for each main", () => (
    <CalibrateFrame view={fx.lfViews.on} initial={{ notesOpen: true }} scrollTo="calibrate.verdict.notes" />
  )),
  { id: "screen.calibrate.lf-lines", title: "보정 · the 저음 도착 맞추기, 방 울림 줄이기 and 고역 위상 맞추기 lines: aligned, held by the limit, not measured, cut modes, nothing to cut, unwound, held by the delay limit, no span to unwind", width: 720, render: () => <LfLines /> },
  frame("tutorial", "tutorial accent", () => (
    <Scoped tutorial>
      <CalibrateFrame view={fx.splView} fileName="튜토리얼.secs" />
    </Scoped>
  )),
  {
    id: "screen.calibrate.ba.min",
    title: "보정 · BA at the smallest window (1024 × 700)",
    width: 1024,
    height: 700,
    render: () => <CalibrateFrame view={fx.baView} />,
  },
];
