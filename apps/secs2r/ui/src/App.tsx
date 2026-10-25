// Composition only: screens own their drawing, hooks own their service calls.
import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState, useSyncExternalStore, type ReactNode } from "react";
import { AppFrame } from "./app/AppFrame";
import { JobTray } from "./app/jobs/JobTray";
import { errorText } from "./app/jobs/jobText";
import { useJobs } from "./app/jobs/useJobs";
import { PrefsScope, usePrefs, type Prefs } from "./app/prefs";
import { SettingsFailed } from "./app/SettingsFailed";
import { canLeave, type StepId } from "./app/model";
import type { RecoveryAction } from "./app/StepHeader";
import { useFrameModel } from "./app/useFrameModel";
import { pickLocale } from "./i18n";
import { useT } from "./i18n/react";
import type { IpcError } from "./ipc/bindings/IpcError";
import type { Ipc } from "./ipc/context";
import { createTauriIpc } from "./ipc/tauri";
import { createSession } from "./state/session";
import { SessionProvider, useHost, useResource, useSession } from "./state/react";
import { useWindowClose } from "./state/closing";
import { useAppSettingsLoad } from "./state/useAppSettingsLoad";
import { Button, EmptyState, ErrorNotice } from "./ui";
import { SystemScreen, systemHeader } from "./screens/system/SystemScreen";
import { ConnectScreen, connectHeader, micInput } from "./screens/connect/ConnectScreen";
import { MeasureScreen, measureHeader } from "./screens/measure/MeasureScreen";
import { isRunning } from "./screens/measure/types";
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
import { tutorialQuests, type TutorialView } from "./screens/tutorial/quests";
import { wrongOutput } from "./screens/tutorial/questHost";

function Shell({ tutorialStart }: { tutorialStart?: string | undefined }) {
  const session = useSession();
  const t = useT();
  const prefs = usePrefs();
  const host = useHost();
  const info = useResource("bootstrap");
  const project = useResource("project");
  const failure = useSyncExternalStore(session.errors.subscribe, session.errors.getSnapshot);
  const jobs = useJobs();
  const [helpOpen, setHelpOpen] = useState(false);
  const [meterOpen, setMeterOpen] = useState(false);
  const [settingFocus, setSettingFocus] = useState<{ step: StepId; selector: string; fallback?: string | undefined } | null>(null);
  const navigateToSetting = useCallback((step: StepId, selector: string, fallback?: string) => {
    session.navigate(step);
    // Consume only a navigation that actually reached its destination.
    setSettingFocus(session.host.getSnapshot().current === step ? { step, selector, fallback } : null);
  }, [session]);
  const configureMeasurement = useCallback(() => {
    const path = session.host.getSnapshot().path;
    if (path !== null && path !== "native") return;
    navigateToSetting("connect", path === "native" ? '[data-ui="connect.output_device"]' : '[data-ui="connect.method.native"]');
  }, [session, navigateToSetting]);
  const system = useSystemScreen(configureMeasurement);
  const connect = useConnectScreen(host.current === "connect");
  useLayoutEffect(() => {
    if (settingFocus === null) return;
    setSettingFocus(null);
    if (host.current !== settingFocus.step) return;
    let target = document.querySelector<HTMLElement>(settingFocus.selector);
    if (!target && settingFocus.fallback) target = document.querySelector<HTMLElement>(settingFocus.fallback);
    if (target?.matches(":disabled")) {
      // With no output devices the picker is disabled; focus its visible label
      // so the user still arrives at the unavailable setting, not another field.
      target = target.closest(".ui-field")?.querySelector<HTMLElement>("label") ?? document.getElementById("frame.header.title");
      if (target) target.tabIndex = -1;
    }
    target?.focus({ preventScroll: true });
    // Only move the user's scroll area. scrollIntoView also moves the frame's
    // overflow-hidden ancestors, shifting the rail and header out of view.
    for (let node = target?.parentElement; target && node; node = node.parentElement) {
      if (!/(auto|scroll)/.test(getComputedStyle(node).overflowY) || node.scrollHeight <= node.clientHeight) continue;
      node.scrollTop = Math.max(0, target.getBoundingClientRect().top - node.getBoundingClientRect().top + node.scrollTop - 16);
      break;
    }
  }, [settingFocus, host.current]);
  const measure = useMeasureScreen();
  const goToMeasurements = useCallback(() => navigateToSetting("measure", '[data-ui="measure.positions"] [aria-current="true"]', '[data-ui="measure.positions"] button'), [navigateToSetting]);
  // A project warning's 다시 측정: that position picked (never during a device
  // job) and its measure action focused; on the import path, the import.
  const measureJob = measure.view?.job ?? null;
  const remeasure = useCallback((position: number) => {
    if (!isRunning(measureJob)) measure.actions.selectPosition(position);
    navigateToSetting("measure", '[data-ui="measure.position.start"]', '[data-ui="measure.mdat.load"]');
  }, [measureJob, measure.actions, navigateToSetting]);
  const calibrate = useCalibrateScreen(goToMeasurements);
  const exporting = useExportScreen();
  const { model, navigate } = useFrameModel({ system: system.view, connect: connect.view, measure: measure.view, calibrate: calibrate.view, export: exporting.view });
  const reason = model.steps[host.current].reason;
  let recovery: RecoveryAction | undefined;
  const recover = (label: string, step: StepId, selector: string): RecoveryAction => ({
    id: "frame.recovery", label, onClick: () => navigateToSetting(step, selector),
  });
  const connectionSetup = () => recover(t("system.device.configure"), "connect", host.path === "native"
    ? '[data-ui="connect.output_device"]' : host.path === "rew" && !(connect.view?.methods?.rew.reachable && connect.view.methods.rew.pro === false)
      ? '[data-ui="connect.rew.launch"]' : '[data-ui="connect.method.native"]');
  const measurementSetup = () => {
    if (model.steps.measure.reason === "device_busy") return undefined;
    if (host.path === null || model.steps.connect.reason === "path_not_ready") return connectionSetup();
    if (model.steps.connect.reason === "no_levels") return recover(t("connect.level.title"), "connect", '[data-ui="connect.level.play_all"]');
    return recover(t("connect.primary.next"), "measure", host.path === "mdat" ? '[data-ui="measure.mdat.load"]' : '[data-ui="measure.position.start"]');
  };
  if (reason === "no_speakers") {
    recovery = recover(t("system.device.configure"), "system", '[data-ui="system.preset"] button');
  } else if (reason === "config_invalid") {
    recovery = { id: "frame.recovery", label: t("system.device.configure"), onClick: () => navigateToSetting("system",
      '[data-ui^="system.patch."][data-ui*=".out"][aria-invalid="true"]', '[data-ui^="system.patch."][data-ui*=".out"], [data-ui="system.patch.renumber"]') };
  } else if (reason === "no_path") {
    recovery = recover(t("system.device.configure"), "connect", '[data-ui="connect.method.native"]');
  } else if (reason === "path_not_ready") {
    recovery = connectionSetup();
  } else if (reason === "no_levels") {
    recovery = recover(t("connect.level.title"), "connect", '[data-ui="connect.level.play_all"]');
  } else if (reason === "no_measurements") {
    recovery = measurementSetup();
  } else if (reason === "no_result" && model.steps.calibrate.reason !== "measuring") {
    recovery = calibrate.view && calibrate.view.positions.length > 0
      ? recover(t("export.go_calibrate"), "calibrate", '[data-ui="calibrate.compute"]') : measurementSetup();
  }
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
  const [tutorialReady, setTutorialReady] = useState(false);
  const preparing = useRef(false);
  useEffect(() => {
    if (!project?.tutorial) { setTutorialReady(false); preparing.current = false; return; }
    if (!ready || !system.view || !connect.view || !calibrate.view || !exporting.view || preparing.current || tutorialReady) return;
    preparing.current = true;
    session.run(async () => {
      const index = tutorialStart && !tutorialStart.startsWith("system.") ? null : wrongOutput(session.stores.config.getSnapshot()!);
      if (index !== null) {
        await session.call({ method: "config_set_speaker", params: { index, role: null, size: null, band_index: 0, out: 16, pol: null } });
      }
      // The retry quest asks the user to choose the silent mock path. No real
      // device or REW is opened as a side effect of a tutorial card.
      await session.configure({ method: tutorialStart && !tutorialStart.startsWith("system.") && tutorialStart !== "connect.retry" ? "mock" : "none" });
      setTutorialReady(true);
    });
  }, [project?.tutorial, ready, system.view, connect.view, measure.view, calibrate.view, exporting.view, tutorialReady, tutorialStart, session]);
  const quests = useMemo(() => tutorialQuests({ system: system.actions, connect: connect.actions, measure: measure.actions, calibrate: calibrate.actions, goTo: navigate }),
    [system.actions, connect.actions, measure.actions, calibrate.actions, navigate]);
  const tutorialView: TutorialView | null = system.view && connect.view && measure.view && calibrate.view && exporting.view ? {
    step: host.current, system: system.view, connect: connect.view, measure: measure.view, calibrate: calibrate.view, export: exporting.view,
  } : null;
  let content: ReactNode = null;
  let header: ReturnType<typeof systemHeader> = {};
  switch (host.current) {
    case "system": if (system.view) { content = <SystemScreen view={system.view} actions={system.actions} />; header = systemHeader(system.view, system.actions, t); } break;
    case "connect": if (connect.view) { content = <ConnectScreen view={connect.view} actions={connect.actions} />; header = connectHeader(connect.view, connect.actions, t); } break;
    case "measure": if (measure.view) { content = <MeasureScreen view={measure.view} actions={measure.actions} />; header = measureHeader(measure.view, measure.actions, t); } break;
    case "calibrate": if (calibrate.view) { content = <CalibrateScreen view={calibrate.view} actions={calibrate.actions} initial={project?.tutorial ? { editSub: "SW3" } : undefined} blocked={model.steps.calibrate.status === "blocked"} />; header = calibrateHeader(calibrate.view, t); } break;
    case "export": if (exporting.view) { content = <ExportScreen view={exporting.view} actions={exporting.actions} />; header = exportHeader(exporting.view, exporting.actions, t); } break;
  }
  const error = failure ? errorText(t, failure.error) : null;
  const body = <>
    <AppFrame model={model} onNavigate={navigate} primary={header.primary} headerStatus={header.status} recovery={recovery}
      projectActions={dialogs.menu} onHelp={() => setHelpOpen((open) => !open)}
      version={info ? { version: info.version, prerelease: info.prerelease } : null} tray={<JobTray api={jobs} />} onRemeasure={remeasure}
      aside={helpOpen || meterOpen ? <>
        {helpOpen && <HelpPanel view={{ step: host.current, tutorial: project?.tutorial ?? false }} actions={{ close: () => setHelpOpen(false), startTutorial: dialogs.offerTutorial }} />}
        {meterOpen && <div className="app-mic"><MicMeterPanel id="app.mic" floating view={{ input: connect.view ? micInput(t, connect.view) : null, monitor: host.monitor, reading: host.meter }} onClose={() => setMeterOpen(false)} /></div>}
      </> : null}>
      {error && failure && <ErrorNotice id="app.error" cause={error.cause} next={error.next} detail={failure.error.detail} code={failure.error.code} />}
      {content ?? (ready && !project ? <EmptyState id="app.project.empty" title={t("project.new.title")} action={<Button variant="primary" onClick={dialogs.menu.onNew}>{t("frame.project.new")}</Button>} /> : null)}
    </AppFrame>
    <ProjectDialogs view={dialogs.view} actions={dialogs.actions} />
  </>;
  const kept = tutorialView ? quests.filter((q) => !q.when || q.when(tutorialView)) : [];
  return project?.tutorial && tutorialReady && tutorialView ? <TutorialLayer quests={quests} view={tutorialView} step={host.current}
    stepOpen={(_view, step) => canLeave(model.steps, step)}
    initial={tutorialStart ? { index: Math.max(0, kept.findIndex((q) => q.id === tutorialStart)) } : undefined}
    actions={{ goTo: navigate, finish: dialogs.finishTutorial, restart: () => {
      setTutorialReady(false);
      preparing.current = true;
      session.run(async () => {
        await session.monitor(false);
        await session.call({ method: "project_new", params: { path: "", preset: "7.1.4", sub_count: 3, sample: true, tutorial: true } });
        await session.configure({ method: "none" });
        const index = wrongOutput(session.stores.config.getSnapshot()!);
        if (index !== null) await session.call({ method: "config_set_speaker", params: { index, role: null, size: null, band_index: 0, out: 16, pol: null } });
        session.navigate("system");
        setTutorialReady(true);
      });
    } }}>{body}</TutorialLayer> : body;
}

function Preferences({ root, initialPrefs, tutorialStart }: { root: boolean; initialPrefs?: Prefs | undefined; tutorialStart?: string | undefined }) {
  const session = useSession();
  const { state, retry } = useAppSettingsLoad();
  // The failure a retry is reading past: the panel stays (its button busy)
  // instead of the window going blank while the load state is loading again.
  const [retrying, setRetrying] = useState<IpcError | null>(null);
  useEffect(() => { void session.start(); return () => session.dispose(); }, [session]);
  const failed = state.status === "failed" ? state.error : state.status === "loading" ? retrying : null;
  if (failed) {
    // No settings yet: the defaults the window has before they arrive. The
    // keys give the loaded settings a fresh scope instead of these defaults.
    const defaults: Prefs = initialPrefs ?? { locale: pickLocale(navigator.languages), theme: "system", accent: "default" };
    return <PrefsScope key="settings-failed" initial={defaults} root={root}>
      <SettingsFailed error={failed} busy={state.status === "loading"} onRetry={() => {
        setRetrying(failed);
        void retry().finally(() => setRetrying(null));
      }} />
    </PrefsScope>;
  }
  if (state.status !== "ready") return null;
  const settings = state.value;
  const initial: Prefs = initialPrefs ?? { locale: settings.language === "en" ? "en" : settings.language === "ko" ? "ko" : pickLocale(navigator.languages), theme: settings.theme, icon: settings.icon, skin: settings.skin, accent: "default" };
  return <PrefsScope key="settings" initial={initial} root={root} onChange={(prefs) => {
    const current = session.stores.appSettings.getSnapshot();
    if (!current) return;
    const icon = prefs.icon ?? current.icon;
    if (current.language !== prefs.locale || current.theme !== prefs.theme || current.icon !== icon || current.skin !== (prefs.skin ?? "default")) {
      session.run(() => session.call({ method: "app_settings_set", params: { settings: { ...current, language: prefs.locale, theme: prefs.theme, icon, skin: prefs.skin ?? "default" } } }));
    }
  }}><Shell tutorialStart={tutorialStart} /></PrefsScope>;
}

export function App({ ipc: supplied, root = true, initialPrefs, tutorialStart }: { ipc?: Ipc; root?: boolean; initialPrefs?: Prefs; /** Gallery: first quest to show after preparing the tutorial project. */ tutorialStart?: string } = {}) {
  const ipc = useMemo(() => supplied ?? createTauriIpc(), [supplied]);
  const session = useMemo(() => createSession(ipc), [ipc]);
  return <SessionProvider value={session}><Preferences root={root} initialPrefs={initialPrefs} tutorialStart={tutorialStart} /></SessionProvider>;
}
