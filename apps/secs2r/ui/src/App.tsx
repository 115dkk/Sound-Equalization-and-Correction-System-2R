// Composition only: screens own their drawing, hooks own their service calls.
import { useEffect, useMemo, useRef, useState, useSyncExternalStore, type ReactNode } from "react";
import { AppFrame } from "./app/AppFrame";
import { JobTray } from "./app/jobs/JobTray";
import { errorText } from "./app/jobs/jobText";
import { useJobs } from "./app/jobs/useJobs";
import { PrefsScope, usePrefs, type Prefs } from "./app/prefs";
import { useFrameModel } from "./app/useFrameModel";
import { pickLocale } from "./i18n";
import { useT } from "./i18n/react";
import { IpcProvider, type Ipc } from "./ipc/context";
import { createTauriIpc } from "./ipc/tauri";
import { createSession } from "./state/session";
import { SessionProvider, useHost, useResource, useSession } from "./state/react";
import { useWindowClose } from "./state/closing";
import { Button, EmptyState, ErrorNotice } from "./ui";
import { SystemScreen, systemHeader } from "./screens/system/SystemScreen";
import { ConnectScreen, connectHeader } from "./screens/connect/ConnectScreen";
import { MeasureScreen, measureHeader } from "./screens/measure/MeasureScreen";
import { CalibrateScreen, calibrateHeader } from "./screens/calibrate/CalibrateScreen";
import { ExportScreen, exportHeader } from "./screens/export/ExportScreen";
import { useSystemScreen } from "./screens/system/useSystemScreen";
import { useConnectScreen } from "./screens/connect/useConnectScreen";
import { useMeasureScreen } from "./screens/measure/useMeasureScreen";
import { useCalibrateScreen } from "./screens/calibrate/useCalibrateScreen";
import { useExportScreen } from "./screens/export/useExportScreen";
import { useProjectScreen } from "./screens/project/useProjectScreen";
import { ProjectDialogs } from "./screens/project/ProjectDialogs";
import { HelpPanel } from "./screens/help/HelpPanel";
import { MicMeterPanel } from "./screens/connect/MicMeterPanel";
import { TutorialLayer } from "./screens/tutorial/Tutorial";
import { sampleQuests } from "./screens/tutorial/sampleQuests";

function Shell() {
  const session = useSession();
  const t = useT();
  const prefs = usePrefs();
  const host = useHost();
  const info = useResource("bootstrap");
  const project = useResource("project");
  const failure = useSyncExternalStore(session.errors.subscribe, session.errors.getSnapshot);
  const jobs = useJobs(session.ipc);
  const [helpOpen, setHelpOpen] = useState(false);
  const [meterOpen, setMeterOpen] = useState(false);
  const system = useSystemScreen();
  const connect = useConnectScreen(host.current === "connect");
  const measure = useMeasureScreen();
  const calibrate = useCalibrateScreen();
  const exporting = useExportScreen();
  const { model, navigate } = useFrameModel({ system: system.view, connect: connect.view, measure: measure.view, calibrate: calibrate.view, export: exporting.view });
  const closeRequest = useRef<() => void>(() => {});
  const close = useWindowClose(() => closeRequest.current());
  const dialogs = useProjectScreen(close);
  closeRequest.current = dialogs.requestClose;
  const ready = useSyncExternalStore(session.ready.subscribe, session.ready.getSnapshot);
  const offered = useRef(false);
  useEffect(() => {
    if (ready && !project && !offered.current) { offered.current = true; dialogs.menu.onNew(); }
  }, [ready, project, dialogs.menu]);

  useEffect(() => {
    const accent = project?.tutorial ? "tutorial" : "default";
    if (prefs.accent !== accent) prefs.set({ accent });
  }, [project?.tutorial, prefs]);
  useEffect(() => {
    const key = (event: KeyboardEvent) => {
      if (event.defaultPrevented || event.isComposing || event.altKey || event.shiftKey) return;
      if ((event.ctrlKey || event.metaKey) && (event.code === "KeyM" || event.key.toLowerCase() === "m")) {
        event.preventDefault(); setMeterOpen((open) => !open);
      }
    };
    window.addEventListener("keydown", key);
    return () => window.removeEventListener("keydown", key);
  }, []);
  const quests = useMemo(() => sampleQuests({ openHelp: () => setHelpOpen(true), compute: () => session.run(async () => {
    if (!(session.stores.project.getSnapshot()?.positions.length)) {
      await session.monitor(false);
      const started = await session.call({ method: "sample_load", params: { positions: null } });
      await session.waitForJob(started.job);
    }
    calibrate.actions.calibrate();
  }) })
    .map((quest) => quest.id === "sample.compute" ? { ...quest, targets: ["calibrate.compute"] } : quest), [calibrate.actions]);
  let content: ReactNode = null;
  let header: ReturnType<typeof systemHeader> = {};
  switch (host.current) {
    case "system": if (system.view) { content = <SystemScreen view={system.view} actions={system.actions} />; header = systemHeader(system.view, system.actions, t); } break;
    case "connect": if (connect.view) { content = <ConnectScreen view={connect.view} actions={connect.actions} />; header = connectHeader(connect.view, connect.actions, t); } break;
    case "measure": if (measure.view) { content = <MeasureScreen view={measure.view} actions={measure.actions} />; header = measureHeader(measure.view, measure.actions, t); } break;
    case "calibrate": if (calibrate.view) { content = <CalibrateScreen view={calibrate.view} actions={calibrate.actions} />; header = calibrateHeader(calibrate.view, calibrate.actions, t); } break;
    case "export": if (exporting.view) { content = <ExportScreen view={exporting.view} actions={exporting.actions} />; header = exportHeader(exporting.view, exporting.actions, t); } break;
  }
  const error = failure ? errorText(t, failure.error) : null;
  const body = <>
    <AppFrame model={model} onNavigate={navigate} primary={header.primary} headerStatus={header.status}
      projectActions={dialogs.menu} onHelp={() => setHelpOpen((open) => !open)}
      version={info ? { version: info.version, prerelease: info.prerelease } : null} tray={<JobTray api={jobs} />}
      aside={helpOpen ? <HelpPanel view={{ step: host.current, tutorial: project?.tutorial ?? false }} actions={{ close: () => setHelpOpen(false), startTutorial: dialogs.offerTutorial }} /> : null}>
      {error && failure && <ErrorNotice id="app.error" cause={error.cause} next={error.next} detail={failure.error.detail} code={failure.error.code} />}
      {content ?? (ready && !project ? <EmptyState id="app.project.empty" title={t("project.new.title")} description={t("project.new.where")} action={<Button variant="primary" onClick={dialogs.menu.onNew}>{t("frame.project.new")}</Button>} /> : host.current === "measure" && project ? <EmptyState id="app.measure.path" title={t("frame.blocked.no_path.short")} description={t("frame.blocked.no_path")} action={<Button variant="primary" onClick={() => navigate("connect")}>{t("frame.step.system.primary")}</Button>} /> : null)}
    </AppFrame>
    <ProjectDialogs view={dialogs.view} actions={dialogs.actions} />
    {meterOpen && <MicMeterPanel id="app.mic" floating view={{ input: connect.view?.settings?.input_device || null, monitor: host.monitor, reading: host.meter }} onClose={() => setMeterOpen(false)} />}
  </>;
  return project?.tutorial ? <TutorialLayer quests={quests} view={{ helpOpen, computing: calibrate.view?.running !== null, computed: !!calibrate.view?.results.calibration }} step={host.current}
    actions={{ goTo: navigate, finish: dialogs.finishTutorial, restart: dialogs.restartTutorial }}>{body}</TutorialLayer> : body;
}

function Preferences({ root, initialPrefs }: { root: boolean; initialPrefs?: Prefs | undefined }) {
  const session = useSession();
  const settings = useResource("appSettings");
  useEffect(() => { void session.start(); return () => session.dispose(); }, [session]);
  if (!settings) return null;
  const initial: Prefs = initialPrefs ?? { locale: settings.language === "en" ? "en" : settings.language === "ko" ? "ko" : pickLocale(navigator.languages), expert: settings.expert_display, theme: settings.theme, accent: "default" };
  return <PrefsScope initial={initial} root={root} onChange={(prefs) => {
    const current = session.stores.appSettings.getSnapshot();
    if (current && (current.language !== prefs.locale || current.expert_display !== prefs.expert || current.theme !== prefs.theme)) {
      session.run(() => session.call({ method: "app_settings_set", params: { settings: { ...current, language: prefs.locale, expert_display: prefs.expert, theme: prefs.theme } } }));
    }
  }}><Shell /></PrefsScope>;
}

export function App({ ipc: supplied, root = true, initialPrefs }: { ipc?: Ipc; root?: boolean; initialPrefs?: Prefs } = {}) {
  const ipc = useMemo(() => supplied ?? createTauriIpc(), [supplied]);
  const session = useMemo(() => createSession(ipc), [ipc]);
  return <IpcProvider value={ipc}><SessionProvider value={session}><Preferences root={root} initialPrefs={initialPrefs} /></SessionProvider></IpcProvider>;
}
