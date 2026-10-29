// 내보내기 in each state, 1280×800 through FrameBox, plus the densest state
// at 1024×700. The filter formats' scope (text: RC only; Equalizer APO: RC
// and BA) shows on the results with subs, with MIMO, and with RC alone.
// 적용 (ADR 0018) shows each CamillaDSP and Equalizer APO state under its row,
// and 관리 the SECS 2R Host service's states in the same rows.
import { useEffect, useLayoutEffect, useRef, useState, type ReactNode } from "react";
import { PrefsScope, usePrefs } from "../../app/prefs";
import { initialLedger, type LedgerState } from "../../state/jobLedger";
import { FrameBox, T0, replay } from "../../gallery/frameSpecimens";
import type { SpecimenDef } from "../../gallery/Specimen";
import { useT } from "../../i18n/react";
import { ExportScreen, exportHeader } from "./ExportScreen";
import { StopManagingConfirm } from "../../host/parts";
import { RevertConfirm } from "./apply";
import {
  applyCamillaApplied,
  applyCamillaApplying,
  applyCamillaBadAddress,
  applyCamillaChecking,
  applyCamillaOther,
  applyCamillaPlaceholders,
  applyCamillaRemote,
  applyCamillaRolledBack,
  applyCamillaThis,
  applyCamillaUnreachable,
  applyEqapoAbsent,
  applyEqapoDamaged,
  applyEqapoDenied,
  applyEqapoMissing,
  applyEqapoNotWindows,
  applyEqapoOfficialUnknown,
  applyEqapoOther,
  applyEqapoPresent,
  applyEqapoReverted,
  DESIGN_NAME_SAMPLES,
  exportComputingFirst,
  exportConvolutionDone,
  exportConvolutionFailed,
  exportDevicesAsio,
  exportDevicesAsioUnnamed,
  exportDevicesTyped,
  exportDevicesWasapi,
  exportFrame,
  exportNamed,
  exportDone,
  exportEqapoDone,
  exportExporting,
  exportFailed,
  exportMeasurementsDone,
  exportMeasurementsOnly,
  exportMimoNoSubs,
  exportNoMimo,
  exportNoResult,
  exportRcOnly,
  exportReady,
  exportNothingMeasured,
  exportRecalibrating,
  exportResultsOnly,
  exportStale,
  exportStaleCache,
  exportStaleCacheMeasurements,
  exportTutorial,
  manageCamillaGivenUp,
  manageCamillaInstalling,
  manageCamillaNoExe,
  manageCamillaNotInstalled,
  manageCamillaOtherUser,
  manageCamillaRestarting,
  manageCamillaRunning,
  manageEqapoRepaired,
  manageNotWindows,
} from "./fixtures";
import type { DesignNameProblem, ExportActions, ExportView } from "./types";

export const noopExportActions: ExportActions = {
  exportFiles: () => {},
  setMdatScope: () => {},
  setDesignName: () => {},
  setCamillaDevices: () => {},
  refreshDevices: () => {},
  changeFolder: () => {},
  openFolder: () => {},
  goToCalibrate: () => {},
  recalibrate: () => {},
  setApplyLevel: () => {},
  setCamillaAddress: () => {},
  saveCamillaAddress: () => {},
  refreshApplyStatus: () => {},
  applyCamilla: () => {},
  applyEqapo: () => {},
  revertEqapo: () => {},
  setCamillaExe: () => {},
  saveCamillaExe: () => {},
  pickCamillaExe: () => {},
  installHost: () => {},
  stopManaging: () => {},
};

const NAME_PROBLEMS = Object.keys(DESIGN_NAME_SAMPLES) as DesignNameProblem[];

const exportJob = replay([{ type: "event", at: T0, event: { event: "started", job: 12, kind: "export" } }]);

const calibrateJob = replay([
  { type: "event", at: T0, event: { event: "started", job: 11, kind: "calibrate" } },
  { type: "event", at: T0 + 2000, event: { event: "progress", job: 11, stage: "engine.design", done: 4, total: 11 } },
]);

const applyJob = replay([{ type: "event", at: T0, event: { event: "started", job: 14, kind: "apply" } }]);

const applyHistory = replay([
  { type: "event", at: T0, event: { event: "started", job: 14, kind: "apply" } },
  { type: "event", at: T0 + 2000, event: { event: "finished", job: 14, status: "succeeded", result: null, error: null } },
]);

const exportHistory = replay([
  { type: "event", at: T0, event: { event: "started", job: 12, kind: "export" } },
  { type: "event", at: T0 + 3000, event: { event: "finished", job: 12, status: "succeeded", result: null, error: null } },
]);

/** Draws `from`, then `to`, the way a run ends in the app (the screen brings the new outcome into view). */
function AfterRun({ from, to }: { from: ExportView; to: ExportView }) {
  const [view, setView] = useState(from);
  useEffect(() => setView(to), [to]);
  return <ExportSpecimen view={view} />;
}

/** Gallery: scrolls the step's content to its end (`"end"`, where 고급 opens) or to the element with the `data-ui` `to`, as a user scrolling there would. */
function Scrolled({ to, children }: { to: string; children: ReactNode }) {
  const ref = useRef<HTMLDivElement>(null);
  useLayoutEffect(() => {
    const content = ref.current?.closest<HTMLElement>('[data-ui="frame.content"]');
    if (!content) return;
    const el = to === "end" ? null : ref.current?.querySelector<HTMLElement>(`[data-ui="${to}"]`);
    content.scrollTop = el ? el.getBoundingClientRect().top - content.getBoundingClientRect().top + content.scrollTop - 16 : content.scrollHeight;
  }, [to]);
  return <div ref={ref}>{children}</div>;
}

function ExportSpecimen({
  view,
  jobs = initialLedger,
  initialAdvanced,
  initialDevices,
  scrolled,
  fileName,
  children,
}: {
  view: ExportView;
  jobs?: LedgerState;
  initialAdvanced?: boolean;
  /** Draw the CamillaDSP devices unfolded. */
  initialDevices?: boolean;
  /** Draw the content scrolled to its end, or to the element with this `data-ui`. */
  scrolled?: true | string;
  fileName?: string;
  children?: ReactNode;
}) {
  const t = useT();
  const screen = <ExportScreen view={view} actions={noopExportActions} initialAdvanced={initialAdvanced ?? false} initialDevices={initialDevices ?? false} />;
  return (
    <>
      <FrameBox frame={exportFrame(view, fileName)} jobs={jobs} header={exportHeader(view, noopExportActions, t)}>
        {scrolled ? <Scrolled to={scrolled === true ? "end" : scrolled}>{screen}</Scrolled> : screen}
      </FrameBox>
      {children}
    </>
  );
}

/** 빼기's confirm as it opens over the screen (drawn in place so other specimens stay uncovered). */
function RevertOverlay() {
  return (
    <div style={{ position: "absolute", inset: 0, zIndex: "var(--z-dialog)", display: "grid", placeItems: "center", background: "var(--scrim)" }}>
      <RevertConfirm inline open onClose={() => {}} onConfirm={() => {}} />
    </div>
  );
}

/** 관리 끄기's confirm as it opens over the screen, for what the view's service manages. */
function StopOverlay({ view }: { view: ExportView }) {
  return (
    <div style={{ position: "absolute", inset: 0, zIndex: "var(--z-dialog)", display: "grid", placeItems: "center", background: "var(--scrim)" }}>
      <StopManagingConfirm id="export.manage.camilla.stop.confirm" inline open host={view.apply.host.status} onClose={() => {}} onConfirm={() => {}} />
    </div>
  );
}

/** The same scope with the tutorial accent (the frame's tutorial look). */
export function TutorialAccent({ children }: { children: ReactNode }) {
  const outer = usePrefs();
  return (
    <PrefsScope key={`${outer.locale}-${outer.theme}-${outer.skin ?? "default"}`} initial={{ locale: outer.locale, theme: outer.theme, accent: "tutorial", ...(outer.skin ? { skin: outer.skin } : {}) }} className="gal-fill">
      {children}
    </PrefsScope>
  );
}

const frame = (id: string, title: string, render: () => ReactNode, size: { width: number; height: number } = { width: 1280, height: 800 }): SpecimenDef => ({ id, title, ...size, render });

export const exportSpecimens: SpecimenDef[] = [
  frame("screen.export.empty", "Export · no correction yet", () => <ExportSpecimen view={exportNoResult} />),
  frame("screen.export.empty.mdat", "Export · no correction yet: the REW mdat holds 원 측정만 and says why, its button on", () => (
    <ExportSpecimen view={exportNoResult} scrolled="export.kind.before_after_mdat" />
  )),
  frame("screen.export.empty.mdat-done", "Export · no correction yet, the measurements saved as 'SECS 원 측정.mdat'", () => (
    <ExportSpecimen view={exportMeasurementsDone} jobs={exportHistory} scrolled="export.kind.before_after_mdat" />
  )),
  frame("screen.export.empty.nothing", "Export · nothing measured: the REW mdat has nothing to save", () => (
    <ExportSpecimen view={exportNothingMeasured} scrolled="export.kind.before_after_mdat" />
  )),
  frame("screen.export.computing", "Export · first correction computing", () => <ExportSpecimen view={exportComputingFirst} jobs={calibrateJob} />),
  frame("screen.export.stale", "Export · stale result (settings changed)", () => <ExportSpecimen view={exportStale} />),
  frame("screen.export.stale-cache", "Export · reopened project, result from an earlier version: both mdat rows blocked, 재보정 after the sentence", () => (
    <ExportSpecimen view={exportStaleCache} initialAdvanced scrolled="export.kind.before_after_mdat" />
  )),
  frame("screen.export.stale-cache.measurements", "Export · reopened project with an old cache, REW mdat set to 원 측정만: that row exports, the staged row stays blocked", () => (
    <ExportSpecimen view={exportStaleCacheMeasurements} initialAdvanced scrolled="export.kind.before_after_mdat" />
  )),
  frame("screen.export.recalibrating", "Export · 재보정 running from an mdat row: the button busy, every export off", () => (
    <ExportSpecimen view={exportRecalibrating} jobs={calibrateJob} initialAdvanced scrolled="export.kind.before_after_mdat" />
  )),
  frame("screen.export.ready", "Export · ready, MIMO included", () => <ExportSpecimen view={exportReady} />),
  frame("screen.export.filters", "Export · subs and MIMO: the text and Equalizer APO rows say why they are off", () => (
    <ExportSpecimen view={exportReady} scrolled="export.files" />
  )),
  frame("screen.export.mdat.all", "Export · REW mdat 모두 (default): 'SECS 원 측정+보정 결과 <모드>.mdat'", () => (
    <ExportSpecimen view={exportReady} scrolled="export.kind.before_after_mdat" />
  )),
  frame("screen.export.mdat.measurements", "Export · REW mdat 원 측정만: 'SECS 원 측정.mdat' in SECS_mdat_measurements_…", () => (
    <ExportSpecimen view={exportMeasurementsOnly} scrolled="export.kind.before_after_mdat" />
  )),
  frame("screen.export.mdat.results", "Export · REW mdat 보정 결과만: 'SECS 보정 결과 <모드>.mdat'", () => (
    <ExportSpecimen view={exportResultsOnly} scrolled="export.kind.before_after_mdat" />
  )),
  frame("screen.export.rc-only", "Export · RC only (no subs, no MIMO): all three filter formats on", () => <ExportSpecimen view={exportRcOnly} scrolled="export.files" />),
  frame("screen.export.mimo", "Export · MIMO included, no subs: the text and Equalizer APO rows blocked by MIMO", () => (
    <ExportSpecimen view={exportMimoNoSubs} scrolled="export.files" />
  )),
  frame("screen.export.eqapo-done", "Export · Equalizer APO configuration saved, with where to copy it", () => (
    <ExportSpecimen view={exportEqapoDone} jobs={exportHistory} scrolled="export.kind.equalizer_apo" />
  )),
  frame("screen.export.convolution", "Export · convolution IR row: subs and MIMO in, so it stays on while the text is off", () => (
    <ExportSpecimen view={exportReady} scrolled="export.kind.equalizer_apo" />
  )),
  frame("screen.export.convolution.done", "Export · convolution IR saved: a WAV and a CSV per pair, with where the guide is", () => (
    <ExportSpecimen view={exportConvolutionDone} jobs={exportHistory} scrolled="export.kind.equalizer_apo" />
  )),
  frame("screen.export.convolution.failed", "Export · convolution IR failed (disk full), the raw text behind 자세히", () => (
    <ExportSpecimen view={exportConvolutionFailed} scrolled="export.kind.equalizer_apo" />
  )),
  frame("screen.export.name.empty", "Export · design name empty: timestamped folders, said under the field", () => <ExportSpecimen view={exportRcOnly} scrolled="export.files" />),
  frame("screen.export.name.named", "Export · design name '거실 영화': each filter row names its SECS_<kind>_거실 영화 folder, the field says the next export replaces it", () => (
    <ExportSpecimen view={exportNamed} scrolled="export.files" />
  )),
  ...NAME_PROBLEMS.map((problem) =>
    frame(`screen.export.name.${problem}`, `Export · design name problem '${problem}': the message under the field, the filter buttons and the header's primary off`, () => (
      <ExportSpecimen view={{ ...exportRcOnly, designName: DESIGN_NAME_SAMPLES[problem] }} scrolled="export.files" />
    )),
  ),
  frame("screen.export.devices.closed", "Export · CamillaDSP devices folded, the summary reading WASAPI on both sides", () => (
    <ExportSpecimen view={exportDevicesWasapi} scrolled="export.files" />
  )),
  frame("screen.export.devices.file", "Export · CamillaDSP devices open: file placeholders on both sides (the default)", () => (
    <ExportSpecimen view={exportReady} initialDevices scrolled="export.kind.camilla_dsp" />
  )),
  frame("screen.export.devices.wasapi", "Export · CamillaDSP devices open: WASAPI chosen from the device list, playback exclusive S24", () => (
    <ExportSpecimen view={exportDevicesWasapi} initialDevices scrolled="export.kind.camilla_dsp" />
  )),
  frame("screen.export.devices.typed", "Export · CamillaDSP devices open, no device list: playback name typed, capture by loopback (exclusive off)", () => (
    <ExportSpecimen view={exportDevicesTyped} initialDevices scrolled="export.kind.camilla_dsp" />
  )),
  frame("screen.export.devices.asio", "Export · CamillaDSP devices open: ASIO playback, with the note that ASIO needs CamillaDSP's ASIO build", () => (
    <ExportSpecimen view={exportDevicesAsio} initialDevices scrolled="export.kind.camilla_dsp" />
  )),
  frame("screen.export.devices.asio-unnamed", "Export · ASIO capture without a driver name: the field says so and the header's primary is off", () => (
    <ExportSpecimen view={exportDevicesAsioUnnamed} initialDevices scrolled="export.kind.camilla_dsp" />
  )),
  frame("screen.export.exporting", "Export · REW mdat being saved", () => <ExportSpecimen view={exportExporting} jobs={exportJob} />),
  frame("screen.export.done", "Export · done, files written", () => <ExportSpecimen view={exportDone} jobs={exportHistory} fileName="거실 7.1.4.secs" />),
  frame("screen.export.failed", "Export · CamillaDSP export failed", () => <ExportSpecimen view={exportFailed} />),
  frame("screen.export.advanced", "Export · 고급 open (staged mdat, U3), MIMO left out, scrolled to the end", () => <ExportSpecimen view={exportNoMimo} initialAdvanced scrolled />),
  frame("screen.export.tutorial", "Export · tutorial (no export folder)", () => (
    <TutorialAccent>
      <ExportSpecimen view={exportTutorial} fileName="튜토리얼.secs" />
    </TutorialAccent>
  )),
  frame("screen.export.apply.camilla.files", "Apply · CamillaDSP 파일만 (the default): the choice under its devices, the header's primary exports", () => (
    <ExportSpecimen view={exportReady} scrolled="export.kind.camilla_dsp" />
  )),
  frame("screen.export.apply.camilla.placeholders", "Apply · CamillaDSP 적용 with file placeholders: the row says to choose devices", () => (
    <ExportSpecimen view={applyCamillaPlaceholders} scrolled="export.kind.camilla_dsp" />
  )),
  frame("screen.export.apply.camilla.checking", "Apply · CamillaDSP 적용, the status being read", () => (
    <ExportSpecimen view={applyCamillaChecking} scrolled="export.kind.camilla_dsp" />
  )),
  frame("screen.export.apply.camilla.unreachable", "Apply · CamillaDSP 적용, nothing at 127.0.0.1:1234: why, what to check, the raw text behind 자세히; the header's primary applies, the row keeps its export", () => (
    <ExportSpecimen view={applyCamillaUnreachable} scrolled="export.kind.camilla_dsp" />
  )),
  frame("screen.export.apply.camilla.this", "Apply · CamillaDSP 4.1.3 processing this design", () => <ExportSpecimen view={applyCamillaThis} scrolled="export.kind.camilla_dsp" />),
  frame("screen.export.apply.camilla.other", "Apply · CamillaDSP running another design (its title) with 1,204 clipped samples", () => (
    <ExportSpecimen view={applyCamillaOther} scrolled="export.kind.camilla_dsp" />
  )),
  frame("screen.export.apply.camilla.applying", "Apply · CamillaDSP에 적용 running: the header's primary busy, every export off, the tray shows the job", () => (
    <ExportSpecimen view={applyCamillaApplying} jobs={applyJob} scrolled="export.kind.camilla_dsp" />
  )),
  frame("screen.export.apply.camilla.applied", "Apply · applied to CamillaDSP, now running this design", () => (
    <ExportSpecimen view={applyCamillaApplied} jobs={applyHistory} scrolled="export.kind.camilla_dsp" />
  )),
  frame("screen.export.apply.camilla.rolled-back", "Apply · CamillaDSP refused the new configuration and went back: its error in sight", () => (
    <ExportSpecimen view={applyCamillaRolledBack} jobs={applyHistory} scrolled="export.kind.camilla_dsp" />
  )),
  frame("screen.export.apply.camilla.address", "Apply · an address without a port: the field says why, the header's primary is off", () => (
    <ExportSpecimen view={applyCamillaBadAddress} scrolled="export.kind.camilla_dsp" />
  )),
  frame("screen.export.apply.camilla.remote", "Apply · CamillaDSP on another computer: the one-line note under the address", () => (
    <ExportSpecimen view={applyCamillaRemote} scrolled="export.kind.camilla_dsp" />
  )),
  frame("screen.export.apply.eqapo.not-windows", "Apply · not Windows: Equalizer APO's 적용 off with the reason", () => (
    <ExportSpecimen view={applyEqapoNotWindows} scrolled="export.kind.equalizer_apo" />
  )),
  frame("screen.export.apply.eqapo.missing", "Apply · Equalizer APO not installed: 적용 stays chosen, its button off with the reason", () => (
    <ExportSpecimen view={applyEqapoMissing} scrolled="export.kind.equalizer_apo" />
  )),
  frame("screen.export.apply.eqapo.absent", "Apply · Equalizer APO XT installed, SECS 2R's line absent: what 적용 does to config.txt", () => (
    <ExportSpecimen view={applyEqapoAbsent} scrolled="export.kind.equalizer_apo" />
  )),
  frame("screen.export.apply.eqapo.present", "Apply · the line present with this design, the quiet config.txt에서 빼기 after it", () => (
    <ExportSpecimen view={applyEqapoPresent} jobs={applyHistory} scrolled="export.kind.equalizer_apo" />
  )),
  frame("screen.export.apply.eqapo.other", "Apply · the line present with another design (CamillaDSP runs this one)", () => (
    <ExportSpecimen view={applyEqapoOther} scrolled="export.kind.equalizer_apo" />
  )),
  frame("screen.export.apply.eqapo.official", "Apply · the official edition, a design whose match is not known yet", () => (
    <ExportSpecimen view={applyEqapoOfficialUnknown} scrolled="export.kind.equalizer_apo" />
  )),
  frame("screen.export.apply.eqapo.damaged", "Apply · one of SECS 2R's two lines edited by hand: 적용 repairs them", () => (
    <ExportSpecimen view={applyEqapoDamaged} scrolled="export.kind.equalizer_apo" />
  )),
  frame("screen.export.apply.eqapo.denied", "Apply · no write access to the config folder: the folder, and that it needs write access", () => (
    <ExportSpecimen view={applyEqapoDenied} scrolled="export.kind.equalizer_apo" />
  )),
  frame("screen.export.apply.eqapo.reverted", "Apply · SECS 2R's line taken out of config.txt", () => (
    <ExportSpecimen view={applyEqapoReverted} scrolled="export.kind.equalizer_apo" />
  )),
  frame("screen.export.apply.eqapo.revert-confirm", "Apply · config.txt에서 빼기 asks in one sentence", () => (
    <ExportSpecimen view={applyEqapoPresent} scrolled="export.kind.equalizer_apo">
      <RevertOverlay />
    </ExportSpecimen>
  )),
  frame("screen.export.manage.camilla.no-exe", "Manage · CamillaDSP 관리 without camilladsp.exe: the field says so, the header's primary is off", () => (
    <ExportSpecimen view={manageCamillaNoExe} scrolled="export.kind.camilla_dsp" />
  )),
  frame("screen.export.manage.camilla.not-installed", "Manage · the service not installed: why it is needed, 서비스 설치, Windows asks once", () => (
    <ExportSpecimen view={manageCamillaNotInstalled} scrolled="export.kind.camilla_dsp" />
  )),
  frame("screen.export.manage.camilla.installing", "Manage · 서비스 설치 waiting for Windows's administrator approval", () => (
    <ExportSpecimen view={manageCamillaInstalling} scrolled="export.kind.camilla_dsp" />
  )),
  frame("screen.export.manage.camilla.running", "Manage · the service runs CamillaDSP (PID 4812) with this design, the quiet 관리 끄기 after it", () => (
    <ExportSpecimen view={manageCamillaRunning} scrolled="export.kind.camilla_dsp" />
  )),
  frame("screen.export.manage.camilla.restarting", "Manage · CamillaDSP stopped, the service restarts it at a stated time (restart 2 within 5 minutes)", () => (
    <ExportSpecimen view={manageCamillaRestarting} scrolled="export.kind.camilla_dsp" />
  )),
  frame("screen.export.manage.camilla.given-up", "Manage · CamillaDSP kept stopping, the service no longer restarts it: what to do, the reason behind 자세히", () => (
    <ExportSpecimen view={manageCamillaGivenUp} scrolled="export.kind.camilla_dsp" />
  )),
  frame("screen.export.manage.camilla.other-user", "Manage · another user at the console: the service waits for the user who started managing", () => (
    <ExportSpecimen view={manageCamillaOtherUser} scrolled="export.kind.camilla_dsp" />
  )),
  frame("screen.export.manage.eqapo.repaired", "Manage · Equalizer APO's line repair on, SECS 2R's line put back 2 minutes ago", () => (
    <ExportSpecimen view={manageEqapoRepaired} scrolled="export.kind.equalizer_apo" />
  )),
  frame("screen.export.manage.stop-confirm", "Manage · 관리 끄기 asks in one sentence (both targets managed)", () => (
    <ExportSpecimen view={manageCamillaRunning} scrolled="export.kind.camilla_dsp">
      <StopOverlay view={manageCamillaRunning} />
    </ExportSpecimen>
  )),
  frame("screen.export.manage.not-windows", "Manage · not Windows: 관리 off with the reason in both rows, CamillaDSP on 적용", () => (
    <ExportSpecimen view={manageNotWindows} scrolled="export.kind.camilla_dsp" />
  )),
  frame("screen.export.done.min", "Export · done, 고급 open, scrolled to the end, smallest window (1024 × 700)", () => <ExportSpecimen view={exportDone} jobs={exportHistory} initialAdvanced scrolled />, { width: 1024, height: 700 }),
  frame("screen.export.failed.min", "Export · failed from the header, smallest window (1024 × 700): the outcome scrolls into view", () => <AfterRun from={exportReady} to={exportFailed} />, { width: 1024, height: 700 }),
];
