// The app: preferences, the IPC and the frame. Step screens arrive in
// P30–P32 and the calls behind them in P33; until then each step shows a
// placeholder and project commands say they are not in this build.
import { useEffect, useMemo, useState } from "react";
import { AppFrame } from "./app/AppFrame";
import { ConnectedJobTray } from "./app/jobs/JobTray";
import type { FrameModel, StepId } from "./app/model";
import { PrefsScope, type Prefs } from "./app/prefs";
import { pickLocale } from "./i18n";
import { useT } from "./i18n/react";
import type { BootstrapInfo } from "./ipc/bindings/BootstrapInfo";
import { IpcFailure, request } from "./ipc/client";
import { IpcProvider, type Ipc } from "./ipc/context";
import { createEventBus } from "./ipc/events";
import { EmptyState, ErrorNotice, useToast } from "./ui";

const initialModel: FrameModel = {
  current: "system",
  steps: {
    system: { status: "ready" },
    connect: { status: "ready" },
    measure: { status: "ready" },
    calibrate: { status: "blocked", reason: "no_measurements" },
    export: { status: "blocked", reason: "no_result" },
  },
  project: {
    preset: null,
    subCount: 0,
    outputs: 0,
    positionsDone: 0,
    positionsTotal: 0,
    calibration: "none",
    path: { kind: "none", state: "warning" },
    fileName: null,
    dirty: false,
  },
};

function Shell() {
  const t = useT();
  const toast = useToast();
  const [model, setModel] = useState(initialModel);
  const [info, setInfo] = useState<BootstrapInfo | null>(null);
  const [failure, setFailure] = useState<string | null>(null);

  useEffect(() => {
    let alive = true;
    request({ method: "bootstrap" })
      .then((value) => alive && setInfo(value))
      .catch((error: unknown) => alive && setFailure(error instanceof IpcFailure ? error.error.detail : String(error)));
    return () => {
      alive = false;
    };
  }, []);

  const unavailable = () => toast.show({ message: t("frame.unavailable"), tone: "neutral" });
  const navigate = (step: StepId) => setModel((m) => ({ ...m, current: step }));

  return (
    <AppFrame
      model={model}
      onNavigate={navigate}
      projectActions={{ onNew: unavailable, onOpen: unavailable, onSave: unavailable, onSaveAs: unavailable }}
      onHelp={unavailable}
      version={info ? { version: info.version, prerelease: info.prerelease } : null}
      tray={<ConnectedJobTray />}
    >
      {failure ? (
        <ErrorNotice cause={t("error.bootstrap")} detail={failure} id="app.bootstrap.error" />
      ) : (
        <EmptyState title={t("frame.placeholder.title")} description={t("frame.placeholder.body")} id="frame.placeholder" />
      )}
    </AppFrame>
  );
}

export function App() {
  const prefs = useMemo<Prefs>(() => ({ locale: pickLocale(navigator.languages), theme: "system", expert: false, accent: "default" }), []);
  const ipc = useMemo<Ipc>(() => ({ request, events: createEventBus() }), []);
  return (
    <IpcProvider value={ipc}>
      <PrefsScope initial={prefs} root>
        <Shell />
      </PrefsScope>
    </IpcProvider>
  );
}
