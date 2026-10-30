// The updater (ADR 0020) as the frame's parts see it. One scope polls the
// service for the badge, the notice and the settings section together, and
// adds what the service does not keep: whether the phase answers a check
// or a restart the user started (the badge shows a failure, or the install
// that follows 지금 다시 시작, only then) and what restarting now costs
// (unsaved changes and running jobs: `update_restart` installs and
// relaunches without the close flow's save prompt).
import { createContext, useCallback, useContext, useMemo, useState, type ReactNode } from "react";
import type { UpdatePolicy } from "../../ipc/bindings/UpdatePolicy";
import type { UpdateStatus } from "../../ipc/bindings/UpdateStatus";
import { activeJobs } from "../../state/jobLedger";
import { useOptionalSession, useResource } from "../../state/react";
import { useJobLedger } from "../../state/useJobLedger";
import { useUpdate } from "../../state/update";

export type UpdateView = {
  /** The last status read; null until the first read answers. */
  status: UpdateStatus | null;
  /** The phase answers 지금 확인, 받아서 설치 or 지금 다시 시작 (지금 설치) from this window. */
  userAction: boolean;
  /** The project has changes that are not saved. */
  dirty: boolean;
  /** Jobs are running. */
  busy: boolean;
};

export type UpdateActions = {
  check: () => Promise<void>;
  download: () => Promise<void>;
  restart: () => Promise<void>;
  dismiss: () => Promise<void>;
  setPolicy: (policy: UpdatePolicy) => Promise<boolean>;
  /** Opens a release's page outside the app (`update_open_release`). */
  openRelease: (version: string) => Promise<void>;
};

export type UpdateApi = { view: UpdateView; actions: UpdateActions };

const Context = createContext<UpdateApi | null>(null);

/** The scope's view and actions; null outside a scope or without a session (frame specimens, most tests). */
export function useUpdateApi(): UpdateApi | null {
  return useContext(Context);
}

/**
 * Gives its children the updater. It reads the service only inside a
 * session, and an inner scope uses the outer one's reads.
 */
export function UpdateScope({
  children,
  userAction = false,
}: {
  children: ReactNode;
  /** Start as if the phase answered the user's own check (gallery). */
  userAction?: boolean;
}) {
  const session = useOptionalSession();
  const outer = useContext(Context);
  if (!session || outer) return <>{children}</>;
  return <LiveScope userAction={userAction}>{children}</LiveScope>;
}

/** A fixed view and actions (tests): no session needed. */
export function StaticUpdateScope({ api, children }: { api: UpdateApi; children: ReactNode }) {
  return <Context.Provider value={api}>{children}</Context.Provider>;
}

/** No action of the user's, one sent and not answered yet, or one answered whose result is still the phase. */
type Mark = "none" | "sent" | "answered";

/**
 * Phases that keep an action the user's: the check running, the download
 * it or 받아서 설치 started, the install after 지금 다시 시작, a failure.
 */
const ANSWERS = new Set(["checking", "downloading", "installing", "failed"]);

function LiveScope({ children, userAction }: { children: ReactNode; userAction: boolean }) {
  const update = useUpdate();
  const ledger = useJobLedger();
  const project = useResource("project");
  const busy = activeJobs(ledger).length > 0;
  const status = update.status;
  const [mark, setMark] = useState<Mark>(userAction ? "answered" : "none");
  const [seen, setSeen] = useState(status);
  // Each new read answers whatever was sent last (the hook drops older
  // reads), so the first one after the user's action is its result.
  if (seen !== status) {
    setSeen(status);
    if (mark !== "none") setMark(ANSWERS.has(status?.phase.kind ?? "") ? "answered" : "none");
  }
  const { check: sendCheck, download: sendDownload, restart: sendRestart, dismiss, setPolicy, openRelease } = update;
  const check = useCallback(async () => {
    setMark("sent");
    await sendCheck();
  }, [sendCheck]);
  const download = useCallback(async () => {
    setMark("sent");
    await sendDownload();
  }, [sendDownload]);
  const restart = useCallback(async () => {
    setMark("sent");
    await sendRestart();
  }, [sendRestart]);
  const kind = status?.phase.kind ?? "";
  const dirty = project?.dirty ?? false;
  const api = useMemo<UpdateApi>(
    () => ({
      view: { status, userAction: mark !== "none" && ANSWERS.has(kind), dirty, busy },
      actions: { check, download, restart, dismiss, setPolicy, openRelease },
    }),
    [status, mark, kind, dirty, busy, check, download, restart, dismiss, setPolicy, openRelease],
  );
  return <Context.Provider value={api}>{children}</Context.Provider>;
}
