// Gallery specimens of the 보정 screen: every state in a 1280×800 frame,
// the densest one again at 1024×700. Settings states scroll the frame's
// content to their section so the screenshot shows it.
import { useLayoutEffect, useRef, type ReactNode } from "react";
import { initialJobs, type JobsAction, type JobsState } from "../../app/jobs/jobStore";
import type { CalibrationState, FrameModel } from "../../app/model";
import { PrefsScope, usePrefs } from "../../app/prefs";
import { FrameBox, T0, replay } from "../../gallery/frameSpecimens";
import type { SpecimenDef } from "../../gallery/Specimen";
import { useT } from "../../i18n/react";
import { CalibrateScreen, calibrateHeader, type CalibrateScreenProps } from "./CalibrateScreen";
import * as fx from "./fixtures";
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
  changeSetting: noop,
  takeSubValues: noop,
  setTargetPoints: noop,
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

const runningJobs = (kind: "calibrate" | "mimo", stage: string, total: number): JobsState =>
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
] satisfies JobsAction[]);

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
  jobs = initialJobs,
  scrollTo,
  initial,
  fileName,
}: {
  view: CalibrateView;
  jobs?: JobsState;
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

export const calibrateSpecimens: SpecimenDef[] = [
  frame("empty", "no result yet: plan, position-1 preview and target", () => <CalibrateFrame view={fx.emptyView} />),
  ...Object.entries(fx.planViews).map(([name, view]) => frame(`plan-${name}`, `calculation plan: ${name}`, () => <CalibrateFrame view={view} />)),
  frame("computing", "first run computing (busy primary, job in the tray)", () => <CalibrateFrame view={fx.computingView} jobs={runningJobs("calibrate", "engine.design", 11)} />),
  frame("spl", "current result, SPL with target handles and the RC band", () => <CalibrateFrame view={fx.splView} />),
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
  frame("mimo-running", "MIMO computing: 보정 계산 and the other jobs are off", () => (
    <CalibrateFrame view={fx.runningMimoView} jobs={runningJobs("mimo", "engine.mimo_iir", 11)} scrollTo="calibrate.settings.ba" />
  )),
  frame("settings", "settings: mode and correction range", () => <CalibrateFrame view={fx.splView} scrollTo="calibrate.settings" />),
  frame("no-subs", "no subs: the XO section says why", () => <CalibrateFrame view={fx.noSubsView} scrollTo="calibrate.settings.xo" />),
  frame("ba-settings", "BA settings with 고급 open on a fixed sub", () => <CalibrateFrame view={fx.baView} scrollTo="calibrate.settings.ba" initial={{ editSub: "SW2" }} />),
  frame("ba-editor", "BA 고급: values, fixed filters and MIMO support of a fixed sub", () => (
    <CalibrateFrame view={fx.baView} scrollTo="calibrate.ba.values" initial={{ editSub: "SW2" }} />
  )),
  frame("ultra", "초고지연 mode and its delay limit", () => <CalibrateFrame view={fx.ultraView} scrollTo="calibrate.settings.mode" />),
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
