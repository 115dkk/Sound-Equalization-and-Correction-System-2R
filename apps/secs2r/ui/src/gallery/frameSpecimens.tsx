// The frame in each state: empty, normal, busy with a job, stale, error,
// blocked and the tutorial accent; plus the frame's menus and the job tray.
import { useEffect, useMemo, useState, type ReactNode } from "react";
import { AppFrame } from "../app/AppFrame";
import { JobTray } from "../app/jobs/JobTray";
import { initialJobs, jobsReducer, type JobsAction, type JobsState } from "../app/jobs/jobStore";
import { useJobs, type JobsApi } from "../app/jobs/useJobs";
import { ProjectMenu, SettingsButton } from "../app/menus";
import { STEP_PRIMARY, type FrameModel, type ProjectSummary, type StepId } from "../app/model";
import { PrefsScope, usePrefs } from "../app/prefs";
import { useT } from "../i18n/react";
import { IpcProvider, useIpc } from "../ipc/context";
import { createMockService, ipcError } from "../ipc/mock";
import { HelpPanel } from "../screens/help/HelpPanel";
import { SAMPLE_ROLES } from "../ipc/fixtures";
import type { PrimaryAction } from "../app/StepHeader";
import { ArrangementMap, Card, EmptyState, ErrorNotice, Graph, StatusPill } from "../ui";
import { isSubRole, roleCode } from "../ui/maps/geometry";
import { logGrid, measurementCurves } from "./data";
import type { SpecimenDef } from "./Specimen";

export const T0 = Date.UTC(2026, 9, 14, 5, 12, 0);

/** A tray state built by replaying events at fixed times (deterministic screenshots). */
export function replay(actions: JobsAction[]): JobsState {
  return actions.reduce(jobsReducer, initialJobs);
}

export const staticApi = (state: JobsState, now: number): JobsApi => ({ state, now, cancel: () => {}, clearHistory: () => {} });

const busyJobs = replay([
  { type: "event", at: T0, event: { event: "started", job: 7, kind: "calibrate" } },
  { type: "event", at: T0 + 2000, event: { event: "progress", job: 7, stage: "engine.design", done: 1, total: 11 } },
  { type: "event", at: T0 + 14000, event: { event: "progress", job: 7, stage: "engine.design", done: 5, total: 11 } },
]);

const twoJobs = replay([
  { type: "event", at: T0, event: { event: "started", job: 3, kind: "measure" } },
  { type: "event", at: T0 + 3000, event: { event: "progress", job: 3, stage: "L", done: 1, total: 13 } },
  { type: "event", at: T0 + 15000, event: { event: "progress", job: 3, stage: "Lb", done: 8, total: 13 } },
  { type: "event", at: T0 + 9000, event: { event: "log", job: 3, code: "measurement_low_snr", params: { role: "SW2", snr_db: "17.2", threshold_db: "20" } } },
  { type: "event", at: T0 + 1000, event: { event: "started", job: 4, kind: "sample_load" } },
  { type: "event", at: T0 + 4000, event: { event: "started", job: 5, kind: "mimo" } },
  { type: "event", at: T0 + 5000, event: { event: "progress", job: 5, stage: "engine.mimo_iir", done: 3, total: 11 } },
  { type: "cancel_requested", job: 5 },
]);

const historyJobs = replay([
  { type: "event", at: T0, event: { event: "started", job: 1, kind: "sample_load" } },
  { type: "event", at: T0 + 2000, event: { event: "finished", job: 1, status: "succeeded", result: null, error: null } },
  { type: "event", at: T0 + 3000, event: { event: "started", job: 2, kind: "measure" } },
  { type: "event", at: T0 + 9000, event: { event: "log", job: 2, code: "measurement_low_snr", params: { role: "SW2", snr_db: "17.2", threshold_db: "20" } } },
  { type: "event", at: T0 + 12000, event: { event: "log", job: 2, code: "rew_sweep_retry", params: { attempt: "2" } } },
  { type: "event", at: T0 + 15000, event: { event: "finished", job: 2, status: "cancelled", result: null, error: ipcError("cancelled") } },
  { type: "event", at: T0 + 20000, event: { event: "started", job: 3, kind: "measure" } },
  {
    type: "event",
    at: T0 + 26000,
    event: {
      event: "finished",
      job: 3,
      status: "failed",
      result: null,
      error: ipcError("rew_unreachable", "GET http://127.0.0.1:4735/version: connection refused (os error 10061)"),
    },
  },
]);

const filledProject: ProjectSummary = {
  preset: "7.1.4",
  subCount: 2,
  outputs: 13,
  positionsDone: 3,
  positionsTotal: 9,
  calibration: "none",
  path: { kind: "rew", state: "ok" },
  fileName: "거실 7.1.4.secs",
  dirty: true,
};

const emptyProject: ProjectSummary = {
  preset: null,
  subCount: 0,
  outputs: 0,
  positionsDone: 0,
  positionsTotal: 0,
  calibration: "none",
  path: { kind: "none", state: "warning" },
  fileName: null,
  dirty: false,
};

const model = (current: StepId, project: ProjectSummary, steps: Partial<FrameModel["steps"]> = {}): FrameModel => ({
  current,
  project,
  steps: {
    system: { status: "done" },
    connect: { status: "done" },
    measure: { status: "ready" },
    calibrate: { status: "ready" },
    export: { status: "blocked", reason: "no_result" },
    ...steps,
  },
});

const ROLES = [...SAMPLE_ROLES];
const CHANNELS = ROLES.map((role) => ({ role, label: roleCode(role), sub: isSubRole(role) }));

function MeasureContent({ stale = false }: { stale?: boolean }) {
  const t = useT();
  const freqs = useMemo(() => logGrid(5, 22000, 280), []);
  const curves = useMemo(() => measurementCurves(freqs, ROLES, 3, isSubRole), [freqs]);
  return (
    <div className="gal-step-grid">
      <Card title={t("map.arrangement.title")} dense>
        <ArrangementMap id="measure.map" count={9} done={[1, 2, 3]} current={4} onSelect={() => {}} />
      </Card>
      <Card title={t("plot.measurement.title")} dense stale={stale}>
        <Graph id="measure.plot" label={t("plot.measurement.title")} freqs={freqs} curves={curves} channels={CHANNELS} roleOrder={ROLES} positions={[1, 2, 3]} yRange={[-35, 20]} height={300} stale={stale} />
      </Card>
    </div>
  );
}

/** A frame drawn from fixed data, for screen specimens (src/screens/<step>/specimens.tsx). */
export function FrameBox({
  frame,
  jobs,
  now = T0 + 16000,
  trayOpen,
  primaryBusy,
  noPrimary,
  header,
  headerStatus,
  aside,
  children,
}: {
  frame: FrameModel;
  jobs: JobsState;
  now?: number;
  trayOpen?: boolean;
  primaryBusy?: boolean;
  /** No primary action (the empty project's next action belongs to the P30 screen). */
  noPrimary?: boolean;
  /** A screen's own header (`<step>Header(view, actions, t)`): its primary
   * action, or none when it returns none, and its status. */
  header?: { primary?: PrimaryAction; status?: ReactNode };
  headerStatus?: ReactNode;
  /** The frame's aside slot (the help panel). */
  aside?: ReactNode;
  children: ReactNode;
}) {
  const t = useT();
  const [current, setCurrent] = useState(frame.current);
  const step = { ...frame, current };
  const blocked = step.steps[current].status === "blocked";
  return (
    <AppFrame
      model={step}
      onNavigate={setCurrent}
      primary={header ? header.primary : noPrimary ? undefined : { id: `${current}.primary`, label: t(STEP_PRIMARY[current]), onClick: () => {}, busy: primaryBusy && !blocked }}
      headerStatus={header ? header.status : headerStatus}
      projectActions={{ onNew: () => {}, onOpen: () => {}, onSave: () => {}, onSaveAs: () => {} }}
      onHelp={() => {}}
      version={{ version: "0.13.0", prerelease: true }}
      tray={<JobTray api={staticApi(jobs, now)} defaultOpen={trayOpen} />}
      aside={aside}
      shortcuts={false}
    >
      {children}
    </AppFrame>
  );
}

function Placeholder() {
  const t = useT();
  return <EmptyState title={t("frame.placeholder.title")} description={t("frame.placeholder.body")} />;
}

/** A frame running on the mock service: 보정 계산 starts a job, the tray follows it, 취소 stops it. */
function LiveFrame() {
  const t = useT();
  const mock = useMemo(() => createMockService({ tickMs: 700 }), []);
  useEffect(() => () => mock.dispose(), [mock]);
  const ipc = useMemo(() => ({ request: mock.request, events: mock.events }), [mock]);
  return (
    <IpcProvider value={ipc}>
      <LiveFrameInner start={() => void mock.request({ method: "calibrate", params: { settings: null, subs_only: false } }).catch(() => {})} label={t(STEP_PRIMARY.calibrate)} />
    </IpcProvider>
  );
}

function LiveFrameInner({ start, label }: { start: () => void; label: string }) {
  const api = useJobs(useIpc());
  const running = api.state.jobs.some((job) => job.kind === "calibrate" && (job.status === "running" || job.status === "cancel_requested"));
  const [current, setCurrent] = useState<StepId>("calibrate");
  return (
    <AppFrame
      model={{ ...model("calibrate", { ...filledProject, positionsDone: 9, calibration: running ? "computing" : "none" }), current }}
      onNavigate={setCurrent}
      primary={{ id: "calibrate.compute", label, onClick: start, busy: running }}
      projectActions={{ onNew: () => {}, onOpen: () => {}, onSave: () => {}, onSaveAs: () => {} }}
      onHelp={() => {}}
      version={{ version: "0.13.0", prerelease: true }}
      tray={<JobTray api={api} />}
      shortcuts={false}
    >
      <MeasureContent />
    </AppFrame>
  );
}

export const frameSpecimens: SpecimenDef[] = [
  {
    id: "frame.empty",
    title: "Frame · empty project",
    width: 1280,
    height: 800,
    render: () => (
      <FrameBox
        frame={model("system", emptyProject, {
          system: { status: "ready" },
          connect: { status: "blocked", reason: "no_speakers" },
          measure: { status: "blocked", reason: "no_speakers" },
          calibrate: { status: "blocked", reason: "no_measurements" },
          export: { status: "blocked", reason: "no_result" },
        })}
        jobs={initialJobs}
        noPrimary
      >
        <Placeholder />
      </FrameBox>
    ),
  },
  {
    id: "frame.normal",
    title: "Frame · measuring, three positions done",
    width: 1280,
    height: 800,
    render: () => (
      <FrameBox frame={model("measure", filledProject)} jobs={initialJobs}>
        <MeasureContent />
      </FrameBox>
    ),
  },
  {
    id: "frame.min",
    title: "Frame · smallest window (1024 × 700)",
    width: 1024,
    height: 700,
    render: () => (
      <FrameBox frame={model("measure", filledProject, { calibrate: { status: "blocked", reason: "no_measurements" } })} jobs={busyJobs}>
        <MeasureContent />
      </FrameBox>
    ),
  },
  {
    id: "frame.busy",
    title: "Frame · busy with a job",
    width: 1280,
    height: 800,
    render: () => <BusyFrame />,
  },
  {
    id: "frame.stale",
    title: "Frame · stale result",
    width: 1280,
    height: 800,
    render: () => <StaleFrame />,
  },
  {
    id: "frame.error",
    title: "Frame · job failed, history open",
    width: 1280,
    height: 800,
    render: () => (
      <FrameBox frame={model("measure", { ...filledProject, path: { kind: "rew", state: "error" } })} jobs={historyJobs} now={T0 + 30000} trayOpen>
        <MeasureContent />
      </FrameBox>
    ),
  },
  {
    id: "frame.blocked",
    title: "Frame · blocked step",
    width: 1280,
    height: 800,
    render: () => (
      <FrameBox frame={model("export", filledProject)} jobs={initialJobs}>
        <Placeholder />
      </FrameBox>
    ),
  },
  {
    id: "frame.tutorial",
    title: "Frame · tutorial accent",
    width: 1280,
    height: 800,
    render: () => <TutorialScope />,
  },
  {
    id: "frame.skipped",
    title: "Frame · mdat path (연결·점검 skipped)",
    width: 1280,
    height: 800,
    render: () => (
      <FrameBox frame={model("measure", { ...filledProject, path: { kind: "none", state: "warning" } }, { connect: { status: "skipped" } })} jobs={initialJobs}>
        <MeasureContent />
      </FrameBox>
    ),
  },
  {
    id: "frame.aside",
    title: "Frame · aside slot: the help panel between the header and the open job history",
    width: 1280,
    height: 800,
    render: () => (
      <FrameBox
        frame={model("measure", { ...filledProject, path: { kind: "rew", state: "error" } })}
        jobs={historyJobs}
        now={T0 + 30000}
        trayOpen
        aside={<HelpPanel view={{ step: "measure", tutorial: false }} actions={{ close: () => {}, startTutorial: () => {} }} initialTab="trouble" />}
      >
        <MeasureContent />
      </FrameBox>
    ),
  },
  {
    id: "frame.live",
    title: "Frame · live on the mock service (click Compute, then Cancel)",
    width: 1280,
    height: 800,
    render: () => <LiveFrame />,
  },
];

function BusyFrame() {
  const t = useT();
  return (
    <FrameBox
      frame={model("calibrate", { ...filledProject, positionsDone: 9, calibration: "computing" }, { measure: { status: "done" }, export: { status: "blocked", reason: "computing" } })}
      jobs={busyJobs}
      primaryBusy
      headerStatus={<StatusPill tone="busy">{t("frame.status.calibration.computing")}</StatusPill>}
    >
      <MeasureContent />
    </FrameBox>
  );
}

function StaleFrame() {
  const t = useT();
  return (
    <FrameBox
      frame={model("calibrate", { ...filledProject, positionsDone: 9, calibration: "stale", dirty: true }, { measure: { status: "done" }, export: { status: "ready" } })}
      jobs={replay([
        { type: "event", at: T0, event: { event: "started", job: 9, kind: "calibrate" } },
        { type: "event", at: T0 + 31000, event: { event: "finished", job: 9, status: "succeeded", result: null, error: null } },
      ])}
      headerStatus={<StatusPill tone="warning">{t("frame.status.calibration.stale")}</StatusPill>}
    >
      <div className="gal-stack">
        <ErrorNotice tone="warning" cause={t("error.stale_inputs")} next={t("error.stale_inputs.next")} />
        <MeasureContent stale />
      </div>
    </FrameBox>
  );
}

/** The tutorial is the same frame with the tutorial accent (its own prefs scope). */
function TutorialScope() {
  const outer = usePrefs();
  const prefs = { locale: outer.locale, theme: outer.theme, expert: outer.expert, accent: "tutorial" as const };
  return (
    <PrefsScope key={`${outer.locale}-${outer.theme}`} initial={prefs} className="gal-fill">
      <TutorialFrame />
    </PrefsScope>
  );
}

function TutorialFrame() {
  return (
    <FrameBox frame={model("connect", { ...filledProject, positionsDone: 0, path: { kind: "mock", state: "ok" }, fileName: "튜토리얼.secs", dirty: false }, { connect: { status: "ready" }, measure: { status: "ready" }, calibrate: { status: "blocked", reason: "no_measurements" } })} jobs={initialJobs}>
      <Placeholder />
    </FrameBox>
  );
}

export const trayAndMenuSpecimens: SpecimenDef[] = [
  { id: "jobs.idle", title: "Job tray · idle", width: 960, render: () => <JobTray api={staticApi(initialJobs, T0)} /> },
  { id: "jobs.running", title: "Job tray · three jobs (determinate, indeterminate, cancel requested)", width: 1020, render: () => <JobTray api={staticApi(twoJobs, T0 + 18000)} /> },
  { id: "jobs.history", title: "Job tray · history with a log, a cancel and an error", width: 960, render: () => <JobTray api={staticApi(historyJobs, T0 + 30000)} defaultOpen /> },
  {
    id: "frame.project-menu",
    title: "Project menu (open)",
    width: 300,
    render: () => (
      <div className="gal-rail-sample">
        <ProjectMenu name="거실 7.1.4.secs" dirty actions={{ onNew: () => {}, onOpen: () => {}, onSave: () => {}, onSaveAs: () => {} }} initiallyOpen inline />
      </div>
    ),
  },
  {
    id: "frame.settings",
    title: "Settings (open)",
    width: 380,
    render: () => (
      <div className="gal-rail-sample">
        <SettingsButton initiallyOpen inline />
      </div>
    ),
  },
];
