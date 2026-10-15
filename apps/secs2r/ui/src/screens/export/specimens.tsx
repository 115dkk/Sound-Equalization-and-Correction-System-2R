// 내보내기 in each state, 1280×800 through FrameBox, plus the densest state
// at 1024×700.
import { useEffect, useLayoutEffect, useRef, useState, type ReactNode } from "react";
import { PrefsScope, usePrefs } from "../../app/prefs";
import { initialJobs, type JobsState } from "../../app/jobs/jobStore";
import { FrameBox, T0, replay } from "../../gallery/frameSpecimens";
import type { SpecimenDef } from "../../gallery/Specimen";
import { useT } from "../../i18n/react";
import { ExportScreen, exportHeader } from "./ExportScreen";
import {
  exportComputingFirst,
  exportFrame,
  exportDone,
  exportExporting,
  exportFailed,
  exportNoMimo,
  exportNoResult,
  exportReady,
  exportStale,
  exportTutorial,
} from "./fixtures";
import type { ExportActions, ExportView } from "./types";

export const noopExportActions: ExportActions = { exportFiles: () => {}, changeFolder: () => {}, openFolder: () => {}, goToCalibrate: () => {} };

const exportJob = replay([{ type: "event", at: T0, event: { event: "started", job: 12, kind: "export" } }]);

const calibrateJob = replay([
  { type: "event", at: T0, event: { event: "started", job: 11, kind: "calibrate" } },
  { type: "event", at: T0 + 2000, event: { event: "progress", job: 11, stage: "engine.design", done: 4, total: 11 } },
]);

const exportHistory = replay([
  { type: "event", at: T0, event: { event: "started", job: 12, kind: "export" } },
  { type: "event", at: T0 + 3000, event: { event: "finished", job: 12, status: "succeeded", result: null, error: null } },
]);

/** Gallery: shows the bottom of the step's content (where 고급 opens). */
/** Draws `from`, then `to`, the way a run ends in the app (the screen brings the new outcome into view). */
function AfterRun({ from, to }: { from: ExportView; to: ExportView }) {
  const [view, setView] = useState(from);
  useEffect(() => setView(to), [to]);
  return <ExportSpecimen view={view} />;
}

function ScrolledToEnd({ children }: { children: ReactNode }) {
  const ref = useRef<HTMLDivElement>(null);
  useLayoutEffect(() => {
    const content = ref.current?.closest<HTMLElement>('[data-ui="frame.content"]');
    if (content) content.scrollTop = content.scrollHeight;
  }, []);
  return <div ref={ref}>{children}</div>;
}

export function ExportSpecimen({
  view,
  jobs = initialJobs,
  initialAdvanced,
  scrolled,
  fileName,
  children,
}: {
  view: ExportView;
  jobs?: JobsState;
  initialAdvanced?: boolean;
  /** Draw the content scrolled to its end. */
  scrolled?: boolean;
  fileName?: string;
  children?: ReactNode;
}) {
  const t = useT();
  const screen = <ExportScreen view={view} actions={noopExportActions} initialAdvanced={initialAdvanced ?? false} />;
  return (
    <>
      <FrameBox frame={exportFrame(view, fileName)} jobs={jobs} header={exportHeader(view, noopExportActions, t)}>
        {scrolled ? <ScrolledToEnd>{screen}</ScrolledToEnd> : screen}
      </FrameBox>
      {children}
    </>
  );
}

/** The same scope with the tutorial accent (the frame's tutorial look). */
export function TutorialAccent({ children }: { children: ReactNode }) {
  const outer = usePrefs();
  return (
    <PrefsScope key={`${outer.locale}-${outer.theme}`} initial={{ locale: outer.locale, theme: outer.theme, expert: outer.expert, accent: "tutorial" }} className="gal-fill">
      {children}
    </PrefsScope>
  );
}

const frame = (id: string, title: string, render: () => ReactNode, size: { width: number; height: number } = { width: 1280, height: 800 }): SpecimenDef => ({ id, title, ...size, render });

export const exportSpecimens: SpecimenDef[] = [
  frame("screen.export.empty", "Export · no correction yet", () => <ExportSpecimen view={exportNoResult} />),
  frame("screen.export.computing", "Export · first correction computing", () => <ExportSpecimen view={exportComputingFirst} jobs={calibrateJob} />),
  frame("screen.export.stale", "Export · stale result (settings changed)", () => <ExportSpecimen view={exportStale} />),
  frame("screen.export.ready", "Export · ready, MIMO included", () => <ExportSpecimen view={exportReady} />),
  frame("screen.export.exporting", "Export · REW mdat being saved", () => <ExportSpecimen view={exportExporting} jobs={exportJob} />),
  frame("screen.export.done", "Export · done, files written", () => <ExportSpecimen view={exportDone} jobs={exportHistory} fileName="거실 7.1.4.secs" />),
  frame("screen.export.failed", "Export · CamillaDSP export failed", () => <ExportSpecimen view={exportFailed} />),
  frame("screen.export.advanced", "Export · 고급 open (staged mdat, U3), MIMO left out, scrolled to the end", () => <ExportSpecimen view={exportNoMimo} initialAdvanced scrolled />),
  frame("screen.export.tutorial", "Export · tutorial (nothing is saved)", () => (
    <TutorialAccent>
      <ExportSpecimen view={exportTutorial} fileName="튜토리얼.secs" />
    </TutorialAccent>
  )),
  frame("screen.export.done.min", "Export · done, 고급 open, scrolled to the end, smallest window (1024 × 700)", () => <ExportSpecimen view={exportDone} jobs={exportHistory} initialAdvanced scrolled />, { width: 1024, height: 700 }),
  frame("screen.export.failed.min", "Export · failed from the header, smallest window (1024 × 700): the outcome scrolls into view", () => <AfterRun from={exportReady} to={exportFailed} />, { width: 1024, height: 700 }),
];
