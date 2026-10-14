// Gallery specimens of 측정·가져오기 (`screen.measure.<state>`): every state
// of the packet in a 1280 × 800 frame, the densest one again at 1024 × 700,
// and the two mdat dialogs drawn in place over their frame.
import type { ReactNode } from "react";
import { AppFrame } from "../../app/AppFrame";
import { JobTray } from "../../app/jobs/JobTray";
import { initialJobs, type JobsState } from "../../app/jobs/jobStore";
import type { FrameModel, PathKind, StepState } from "../../app/model";
import { PrefsScope, usePrefs } from "../../app/prefs";
import { replay, staticApi, T0 } from "../../gallery/frameSpecimens";
import type { SectionDef, SpecimenDef } from "../../gallery/Specimen";
import { useT } from "../../i18n/react";
import { baseName } from "./format";
import {
  countdownView,
  doneView,
  emptyView,
  filesView,
  graphHiddenView,
  importedView,
  lowSnrView,
  mdatAssignView,
  measuringView,
  noPathView,
  partialView,
  sampleLoadingView,
  tutorialView,
} from "./fixtures";
import { MdatAssignDialog, MdatFilesDialog } from "./MdatDialogs";
import { MeasureOptions, MeasureScreen, measureHeader } from "./MeasureScreen";
import { donePositions, measureStepState, type MeasureActions, type MeasureView } from "./types";

const noop = () => {};
export const noopActions: MeasureActions = {
  measurePosition: noop,
  cancelJob: noop,
  clearPosition: () => Promise.resolve("undo-token"),
  undo: () => Promise.resolve(),
  selectPosition: noop,
  setGraphHidden: noop,
  setStartDelay: noop,
  setSweepLength: noop,
  setRepetitions: noop,
  pickMdat: noop,
  importMdat: noop,
  closeMdat: noop,
  loadSample: noop,
  openCalibrate: noop,
};

/** The frame's model for a measure view: earlier steps done (or 연결·점검 skipped on the import path). */
function frameModel(view: MeasureView): FrameModel {
  const measure = measureStepState(view);
  const measured = donePositions(view).length;
  const calibrate: StepState =
    measure.status === "blocked" && measure.reason === "measuring"
      ? { status: "blocked", reason: "measuring" }
      : view.records.some((record) => record.roles.length > 0)
        ? { status: "ready" }
        : { status: "blocked", reason: "no_measurements" };
  const kind: PathKind = view.path === "import" || view.path === "none" ? "none" : (view.config.method as PathKind);
  return {
    current: "measure",
    steps: {
      system: { status: "done" },
      connect: view.path === "import" ? { status: "skipped" } : view.path === "none" ? { status: "ready" } : { status: "done" },
      measure,
      calibrate,
      export: { status: "blocked", reason: "no_result" },
    },
    project: {
      preset: view.project.preset || "7.1.4",
      subCount: view.project.sub_count,
      outputs: view.config.roles.length,
      positionsDone: measured,
      positionsTotal: view.layout.count,
      calibration: "none",
      path: { kind, state: kind === "none" ? "warning" : "ok" },
      fileName: view.project.path ? baseName(view.project.path) : null,
      dirty: view.project.dirty,
    },
  };
}

function MeasureFrame({ view, jobs = initialJobs, now = T0 + 16000 }: { view: MeasureView; jobs?: JobsState; now?: number }) {
  const t = useT();
  const header = measureHeader(view, noopActions, t);
  return (
    <AppFrame
      model={frameModel(view)}
      onNavigate={noop}
      primary={header.primary}
      headerStatus={header.status}
      projectActions={{ onNew: noop, onOpen: noop, onSave: noop, onSaveAs: noop }}
      onHelp={noop}
      version={{ version: "0.13.0", prerelease: true }}
      tray={<JobTray api={staticApi(jobs, now)} />}
      shortcuts={false}
    >
      <MeasureScreen view={view} actions={noopActions} />
    </AppFrame>
  );
}

/** A dialog drawn in place over its frame (the app portals it instead). */
function WithDialog({ view, dialog }: { view: MeasureView; dialog: ReactNode }) {
  return (
    <div className="gal-fill">
      <MeasureFrame view={view} />
      <div className="measure-spec-dialog">{dialog}</div>
    </div>
  );
}

function AssignDialogSpecimen() {
  const view = mdatAssignView;
  return (
    <WithDialog
      view={{ ...view, mdat: null }}
      dialog={view.mdat && <MdatAssignDialog inline pending={view.mdat} roles={view.config.roles} count={view.layout.count} fallbackPosition={4} onImport={noop} onClose={noop} />}
    />
  );
}

function FilesDialogSpecimen() {
  const view = filesView;
  return (
    <WithDialog
      view={view}
      dialog={<MdatFilesDialog inline open count={view.layout.count} records={view.records} roleCount={view.config.roles.length} busy={false} onLoad={noop} onClear={noop} onClose={noop} />}
    />
  );
}

/** The tutorial accent (its own prefs scope, like the frame's tutorial specimen). */
function Tutorial({ view }: { view: MeasureView }) {
  const outer = usePrefs();
  return (
    <PrefsScope key={`${outer.locale}-${outer.theme}`} initial={{ locale: outer.locale, theme: outer.theme, expert: outer.expert, accent: "tutorial" }} className="gal-fill">
      <MeasureFrame view={view} jobs={view.job ? sampleJobs : initialJobs} />
    </PrefsScope>
  );
}

const measuringJobs = replay([
  { type: "event", at: T0, event: { event: "started", job: 12, kind: "measure" } },
  { type: "event", at: T0 + 4000, event: { event: "progress", job: 12, stage: "L", done: 1, total: 13 } },
  { type: "event", at: T0 + 46000, event: { event: "progress", job: 12, stage: "Lb", done: 7, total: 13 } },
]);

const countdownJobs = replay([
  { type: "event", at: T0, event: { event: "started", job: 12, kind: "measure" } },
  { type: "event", at: T0 + 3000, event: { event: "progress", job: 12, stage: "countdown", done: 3, total: 10 } },
]);

const lowSnrJobs = replay([
  { type: "event", at: T0, event: { event: "started", job: 11, kind: "measure" } },
  { type: "event", at: T0 + 61000, event: { event: "log", job: 11, code: "measurement_low_snr", params: { role: "SW2", snr_db: "17.23", threshold_db: "20" } } },
  { type: "event", at: T0 + 88000, event: { event: "log", job: 11, code: "measurement_low_snr", params: { role: "Tbr", snr_db: "18.61", threshold_db: "20" } } },
  { type: "event", at: T0 + 104000, event: { event: "finished", job: 11, status: "succeeded", result: null, error: null } },
]);

const importJobs = replay([
  { type: "event", at: T0, event: { event: "started", job: 5, kind: "mdat_import" } },
  { type: "event", at: T0 + 2000, event: { event: "progress", job: 5, stage: "mdat_import", done: 78, total: 78 } },
  { type: "event", at: T0 + 2500, event: { event: "finished", job: 5, status: "succeeded", result: null, error: null } },
]);

const sampleJobs = replay([{ type: "event", at: T0 + 15000, event: { event: "started", job: 3, kind: "sample_load" } }]);

const frame = (id: string, title: string, render: () => ReactNode, size: "default" | "min" = "default"): SpecimenDef => ({
  id,
  title,
  width: size === "min" ? 1024 : 1280,
  height: size === "min" ? 700 : 800,
  render,
});

export const measureSpecimens: SpecimenDef[] = [
  frame("screen.measure.empty", "Measure · nothing measured, project not saved yet", () => <MeasureFrame view={emptyView} />),
  frame("screen.measure.partial", "Measure · three of nine positions measured", () => <MeasureFrame view={partialView} />),
  frame("screen.measure.measuring", "Measure · measuring position 4 (role progress)", () => <MeasureFrame view={measuringView} jobs={measuringJobs} now={T0 + 52000} />),
  frame("screen.measure.measuring.min", "Measure · measuring position 4 at 1024 × 700", () => <MeasureFrame view={measuringView} jobs={measuringJobs} now={T0 + 52000} />, "min"),
  frame("screen.measure.countdown", "Measure · start delay counting down", () => <MeasureFrame view={countdownView} jobs={countdownJobs} now={T0 + 3500} />),
  frame("screen.measure.done", "Measure · every position measured", () => <MeasureFrame view={doneView} />),
  frame("screen.measure.graph-hidden", "Measure · graph hidden, map enlarged", () => <MeasureFrame view={graphHiddenView} />),
  frame("screen.measure.graph-hidden.min", "Measure · graph hidden at 1024 × 700", () => <MeasureFrame view={graphHiddenView} />, "min"),
  frame("screen.measure.low-snr", "Measure · low S/N warning on position 4", () => <MeasureFrame view={lowSnrView} jobs={lowSnrJobs} now={T0 + 110000} />),
  frame("screen.measure.mdat-assign", "Measure · mdat assignment with a doubled channel", () => <AssignDialogSpecimen />),
  frame("screen.measure.mdat-assign.min", "Measure · mdat assignment at 1024 × 700", () => <AssignDialogSpecimen />, "min"),
  frame("screen.measure.files", "Measure · mdat files by position", () => <FilesDialogSpecimen />),
  frame("screen.measure.imported", "Measure · imported from mdat (연결·점검 skipped)", () => <MeasureFrame view={importedView} jobs={importJobs} now={T0 + 9000} />),
  frame("screen.measure.no-path", "Measure · no measurement method chosen", () => <MeasureFrame view={noPathView} />),
  frame("screen.measure.tutorial", "Measure · tutorial accent with demo data", () => <Tutorial view={tutorialView} />),
  frame("screen.measure.tutorial-loading", "Measure · tutorial, demo data loading (device busy)", () => <Tutorial view={sampleLoadingView} />),
  {
    id: "screen.measure.options",
    title: "Measure · options with 고급 open (acoustic timing: one repetition)",
    width: 380,
    render: () => <MeasureOptions view={partialView} actions={noopActions} busy={false} advancedOpen />,
  },
];

export const measureSection: SectionDef = { id: "screen.measure", title: "Screen · 측정·가져오기", specimens: measureSpecimens };
