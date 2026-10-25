// The tray is a projection of the session ledger, never a second job owner.
import { useEffect, useMemo, useState } from "react";
import type { JobId } from "../../ipc/bindings/JobId";
import { activeJobs, trayJobs, type LedgerJob, type LedgerState } from "../../state/jobLedger";
import { useSession } from "../../state/react";
import { useJobLedger } from "../../state/useJobLedger";

export type JobView = LedgerJob & { id: JobId; stage: string | null; done: number; total: number };
export type JobsState = Omit<LedgerState, "jobs"> & { jobs: JobView[] };
export type JobsApi = { state: JobsState; cancel: (job: JobId) => void; clearHistory: () => void; now: number };

/** Preserve the tray's view shape without retaining another copy of job state. */
export function trayState(ledger: LedgerState): JobsState {
  const state = trayJobs(ledger);
  return { ...state, jobs: state.jobs.map((job) => ({ ...job, id: job.job,
    stage: job.progress?.stage ?? null, done: job.progress?.done ?? 0, total: job.progress?.total ?? 0,
  })) };
}

export function useJobs(): JobsApi {
  const session = useSession();
  const ledger = useJobLedger();
  const state = useMemo(() => trayState(ledger), [ledger]);
  const [now, setNow] = useState(() => Date.now());

  // A display clock for elapsed and remaining time while something runs.
  const running = activeJobs(state).length > 0;
  useEffect(() => {
    if (!running) return;
    setNow(Date.now());
    const timer = window.setInterval(() => setNow(Date.now()), 1000);
    return () => window.clearInterval(timer);
  }, [running]);

  return { state, cancel: session.jobs.cancel, clearHistory: session.jobs.clearHistory, now };
}
