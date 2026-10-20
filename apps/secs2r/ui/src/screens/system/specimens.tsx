// 시스템 in each state the packet names: a new project, the sample 7.1.4
// room, a configuration with problems, the confirm dialog of a preset change
// with measurements, the expert view, and the smallest window.
import type { ReactNode } from "react";
import type { FrameModel, ProjectSummary } from "../../app/model";
import { initialJobs } from "../../app/jobs/jobStore";
import { PrefsScope, usePrefs, type Prefs } from "../../app/prefs";
import { useT } from "../../i18n/react";
import { FrameBox } from "../../gallery/frameSpecimens";
import type { SpecimenDef } from "../../gallery/Specimen";
import { ConfirmDialog } from "../../ui";
import { invalidView, newProjectView, sampleView } from "./fixtures";
import { changeCopy, SystemScreen, systemHeader } from "./SystemScreen";
import { systemStepState } from "./state";
import type { SystemActions, SystemView } from "./types";

const token = async () => "undo-1";

/** Actions that do nothing (the gallery draws fixed views). */
export const noopSystemActions: SystemActions = {
  configureMeasurement: () => {},
  applyPreset: token,
  setSubCount: token,
  setRole: () => {},
  setSize: () => {},
  setOutput: () => {},
  setPolarity: () => {},
  addSpeaker: () => {},
  removeSpeaker: () => {},
  renumberOutputs: () => {},
  savePatchSheet: () => {},
  setDspPreset: () => {},
  setDspFields: () => {},
  setArrangement: token,
  setStartDelay: () => {},
  setTiming: () => {},
  undo: async () => {},
  next: () => {},
};

function frameFor(view: SystemView, project: Partial<ProjectSummary> = {}): FrameModel {
  const system = systemStepState(view);
  const measured = view.positionsDone.length;
  const path: ProjectSummary["path"] = project.path ?? { kind: "none", state: "warning" };
  const blocked = system.status !== "done" ? system.reason ?? "no_speakers"
    : path.kind === "none" ? "no_path" : path.state !== "ok" ? "path_not_ready" : null;
  return {
    current: "system",
    project: {
      preset: view.config.speakers.length ? view.config.preset : null,
      subCount: view.config.summary.sub_count,
      outputs: view.config.summary.outputs.length,
      positionsDone: measured,
      positionsTotal: view.layout.count,
      calibration: "none",
      path,
      fileName: null,
      dirty: false,
      ...project,
    },
    steps: {
      system,
      connect: blocked ? { status: "blocked", reason: blocked } : { status: path.kind === "mdat" ? "skipped" : "ready" },
      measure: blocked ? { status: "blocked", reason: blocked }
        : path.kind === "mdat" || measured > 0 ? { status: "ready" } : { status: "blocked", reason: "no_levels", count: view.config.speakers.length },
      calibrate: measured > 0 ? { status: "ready" } : { status: "blocked", reason: "no_measurements" },
      export: { status: "blocked", reason: "no_result" },
    },
  };
}

const sampleProject: Partial<ProjectSummary> = { path: { kind: "rew", state: "ok" }, fileName: "거실 7.1.4.secs", dirty: true };

function SystemFrame({ view, project, children }: { view: SystemView; project?: Partial<ProjectSummary>; children?: ReactNode }) {
  const t = useT();
  const path = project?.path?.kind;
  const configurable = !["checking", "rew_unknown", "not_required"].includes(view.outputDeviceState) && (!path || path === "none" || path === "native");
  const actions = { ...noopSystemActions, configureMeasurement: configurable ? noopSystemActions.configureMeasurement : null };
  return (
    <FrameBox frame={frameFor(view, project)} jobs={initialJobs} header={systemHeader(view, actions, t)}>
      <SystemScreen view={view} actions={actions} />
      {children}
    </FrameBox>
  );
}

/** The same frame under other display settings (the expert view), in its own prefs scope. */
function WithPrefs({ patch, children }: { patch: Partial<Prefs>; children: ReactNode }) {
  const outer = usePrefs();
  const prefs: Prefs = { locale: outer.locale, theme: outer.theme, expert: outer.expert, accent: outer.accent, ...patch };
  return (
    <PrefsScope key={`${outer.locale}-${outer.theme}-${outer.accent}`} initial={prefs} className="gal-fill">
      {children}
    </PrefsScope>
  );
}

/** The preset-change dialog as it opens over the screen (drawn in place so other specimens stay uncovered). */
function ConfirmOverlay() {
  const t = useT();
  const copy = changeCopy(t, { kind: "preset", preset: "5.1" }, sampleView.positionsDone.length);
  return (
    <div style={{ position: "absolute", inset: 0, zIndex: "var(--z-dialog)", display: "grid", placeItems: "center", background: "var(--scrim)" }}>
      <ConfirmDialog inline open id="system.confirm" title={copy.title} body={copy.body} confirmLabel={copy.confirm} onClose={() => {}} onConfirm={() => {}} />
    </div>
  );
}

const DEVICE_PATHS = {
  default: { kind: "native", state: "ok" },
  checking: { kind: "native", state: "warning" },
  rew_unknown: { kind: "rew", state: "warning" },
  not_required: { kind: "mdat", state: "ok" },
  unavailable: { kind: "native", state: "warning" },
} satisfies Record<string, ProjectSummary["path"]>;

export const systemSpecimens: SpecimenDef[] = [
  ...(["default", "checking", "rew_unknown", "not_required", "unavailable"] as const).map((state) => ({
    id: `screen.system.device-${state}`, title: `System · measurement output ${state}`, width: 1280, height: 800,
    render: () => <SystemFrame view={{ ...newProjectView, outputDeviceState: state }} project={{ path: DEVICE_PATHS[state] }} />,
  })),
  {
    id: "screen.system.new",
    title: "System · new project (2.0, nothing measured, no device yet)",
    width: 1280,
    height: 800,
    render: () => <SystemFrame view={newProjectView} />,
  },
  {
    id: "screen.system.sample",
    title: "System · sample 7.1.4 with two subs, three positions measured",
    width: 1280,
    height: 800,
    render: () => <SystemFrame view={sampleView} project={sampleProject} />,
  },
  {
    id: "screen.system.invalid",
    title: "System · problems: duplicate output, outputs past the device, a speaker without a band",
    width: 1280,
    height: 800,
    render: () => <SystemFrame view={invalidView} project={{ fileName: "홈시어터.secs", dirty: true }} />,
  },
  {
    id: "screen.system.confirm",
    title: "System · confirm a preset change that clears measurements (U5)",
    width: 1280,
    height: 800,
    render: () => (
      <div className="gal-fill" style={{ position: "relative" }}>
        <SystemFrame view={sampleView} project={sampleProject} />
        <ConfirmOverlay />
      </div>
    ),
  },
  {
    id: "screen.system.expert",
    title: "System · expert display (band column, role names in the file)",
    width: 1280,
    height: 800,
    render: () => (
      <WithPrefs patch={{ expert: true }}>
        <SystemFrame view={sampleView} project={sampleProject} />
      </WithPrefs>
    ),
  },
  {
    id: "screen.system.sample.min",
    title: "System · smallest window (1024 × 700)",
    width: 1024,
    height: 700,
    render: () => <SystemFrame view={sampleView} project={sampleProject} />,
  },
];
