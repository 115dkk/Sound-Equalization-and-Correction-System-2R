// 연결·점검 in each state the packet names, the floating mic meter, and the
// smallest window.
import type { ReactNode } from "react";
import { initialLedger } from "../../state/jobLedger";
import type { FrameModel, PathKind, PathState, ProjectSummary } from "../../app/model";
import { PrefsScope, usePrefs, type Prefs } from "../../app/prefs";
import { useT } from "../../i18n/react";
import { FrameBox } from "../../gallery/frameSpecimens";
import type { SpecimenDef } from "../../gallery/Specimen";
import { ConnectScreen, connectHeader, micInput } from "./ConnectScreen";
import {
  emptyView,
  levelDoneView,
  levelPassedView,
  levelHalfView,
  mdatView,
  mockView,
  nativeErrorView,
  nativeView,
  rewNoApiView,
  rewJavaView,
  rewNotProView,
  rewOffView,
  rewSwitchingView,
  rewUntestedView,
} from "./fixtures";
import { MicMeterPanel } from "./MicMeterPanel";
import { connectStepState, pathReady } from "./state";
import type { ConnectActions, ConnectView } from "./types";

/** Actions that do nothing (the gallery draws fixed views). */
const noopConnectActions: ConnectActions = {
  choosePath: () => {},
  refreshMethods: () => {},
  launchRew: () => {},
  readRewSettings: () => {},
  setRewDriver: () => {},
  setInputDevice: () => {},
  setInputChannel: () => {},
  setOutputDevice: () => {},
  setSweepLength: () => {},
  setShareMode: () => {},
  playLevel: () => {},
  stopLevel: () => {},
  playAllLevels: () => {},
  setMasterLevel: () => {},
  skipLevels: () => {},
  checkSweepLevel: () => {},
  applySweepLevel: () => {},
  next: () => {},
};

function frameFor(view: ConnectView, project: Partial<ProjectSummary> = {}): FrameModel {
  const connect = connectStepState(view);
  const kind: PathKind = view.path === null || view.path === "mdat" ? "none" : view.path;
  const state: PathState = view.path === null ? "warning" : pathReady(view) ? "ok" : view.methods?.native_error && view.path === "native" ? "error" : "warning";
  const measure = view.path === "mdat" || connect.status === "done" || connect.status === "ready" ? { status: "ready" as const } : { status: "blocked" as const, reason: connect.reason ?? "no_path" };
  return {
    current: "connect",
    project: {
      preset: "7.1.4",
      subCount: 2,
      outputs: 13,
      positionsDone: 0,
      positionsTotal: 9,
      calibration: "none",
      path: { kind, state },
      fileName: "거실 7.1.4.secs",
      dirty: true,
      ...project,
    },
    steps: {
      system: { status: "done" },
      connect,
      measure,
      calibrate: { status: "blocked", reason: "no_measurements" },
      export: { status: "blocked", reason: "no_result" },
    },
  };
}

function ConnectFrame({ view, project }: { view: ConnectView; project?: Partial<ProjectSummary> }) {
  const t = useT();
  const header = connectHeader(view, noopConnectActions, t);
  return (
    <FrameBox frame={frameFor(view, project)} jobs={initialLedger} header={header}>
      <ConnectScreen view={view} actions={noopConnectActions} />
    </FrameBox>
  );
}

/** The tutorial is the same frame with the tutorial accent (its own prefs scope). */
function WithPrefs({ patch, children }: { patch: Partial<Prefs>; children: ReactNode }) {
  const outer = usePrefs();
  const prefs: Prefs = { locale: outer.locale, theme: outer.theme, accent: outer.accent, ...(outer.skin ? { skin: outer.skin } : {}), ...patch };
  return (
    <PrefsScope key={`${outer.locale}-${outer.theme}-${outer.skin ?? "default"}`} initial={prefs} className="gal-fill">
      {children}
    </PrefsScope>
  );
}

function FloatingMeter() {
  const t = useT();
  return <MicMeterPanel id="frame.mic" floating view={{ input: micInput(t, levelHalfView), monitor: "receiving", reading: levelHalfView.level.reading }} onClose={() => {}} />;
}

const frame = (id: string, title: string, view: ConnectView, project?: Partial<ProjectSummary>): SpecimenDef => ({
  id,
  title,
  width: 1280,
  height: 800,
  render: () => <ConnectFrame view={view} {...(project ? { project } : {})} />,
});

export const connectSpecimens: SpecimenDef[] = [
  frame("screen.connect.empty", "Connect · nothing chosen yet", emptyView),
  frame("screen.connect.rew_off", "Connect · REW chosen, not running", rewOffView, { path: { kind: "rew", state: "warning" } }),
  frame("screen.connect.rew_no_api", "Connect · REW running without the API", rewNoApiView),
  frame("screen.connect.rew_untested", "Connect · REW connected, licence not known yet; devices read from REW", rewUntestedView),
  frame("screen.connect.rew_not_pro", "Connect · REW refused a sweep for want of Pro (settings read)", rewNotProView),
  frame("screen.connect.rew_java", "Connect · REW on Java: unopened devices listed by name only", rewJavaView),
  frame("screen.connect.rew_switching", "Connect · REW switching its driver, device fields waiting", rewSwitchingView),
  frame("screen.connect.native", "Connect · native measurement with devices", nativeView),
  frame("screen.connect.native-sharing", "Connect · saved device sharing choice shown in local details", {
    ...nativeView, settings: { ...nativeView.settings!, share_mode: "shared" },
  }),
  frame("screen.connect.native_error", "Connect · native device list could not be read", nativeErrorView),
  frame("screen.connect.level_half", "Connect · level check half done, Rs playing in turn", levelHalfView),
  frame("screen.connect.level_done", "Connect · every channel has a level", levelDoneView),
  frame("screen.connect.level_passed", "Connect · every level in range: the header goes on to 측정하기", levelPassedView),
  frame("screen.connect.mock", "Connect · simulated room", mockView, { fileName: "샘플 7.1.4.secs", dirty: false }),
  frame("screen.connect.mdat", "Connect · mdat import only (step skipped)", mdatView),
  {
    id: "screen.connect.tutorial",
    title: "Connect · tutorial accent (simulated room)",
    width: 1280,
    height: 800,
    render: () => (
      <WithPrefs patch={{ accent: "tutorial" }}>
        <ConnectFrame view={{ ...mockView, level: { ...mockView.level, playing: "Ls", playingAll: true, reading: { rms_dbfs: -23.8, peak_dbfs: -14.1, spl_db: null, clipping: false } } }} project={{ fileName: "튜토리얼.secs", dirty: false, path: { kind: "mock", state: "ok" } }} />
      </WithPrefs>
    ),
  },
  { ...frame("screen.connect.level_half.min", "Connect · smallest window (1024 × 700)", levelHalfView), width: 1024, height: 700 },
  { id: "screen.connect.mic_panel", title: "Connect · the mic meter as a floating panel (upstream Ctrl+M)", width: 400, render: () => <FloatingMeter /> },
];
