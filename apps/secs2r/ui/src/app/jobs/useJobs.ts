// Connects the tray state to the service: one `jobs` request for the
// snapshots on mount, then pushed events only. Cancel calls `job_cancel`
// and shows 취소하는 중 until the service reports the job finished.
import { useCallback, useEffect, useReducer, useRef, useState } from "react";
import { TERMINAL_JOB_LIMIT, observeSnapshotStart, snapshotFence } from "../../state/jobRetention";
import { IpcFailure } from "../../ipc/client";
import type { Ipc } from "../../ipc/context";
import type { JobId } from "../../ipc/bindings/JobId";
import { activeJobs, initialJobs, jobsReducer, type JobsState, type SnapshotFinish } from "./jobStore";

export type JobsApi = { state: JobsState; cancel: (job: JobId) => void; clearHistory: () => void; now: number };

const asIpcError = (error: unknown) =>
  error instanceof IpcFailure ? error.error : { code: "internal" as const, params: {}, retryable: false, detail: String(error) };

export function useJobs(ipc: Ipc, initial: JobsState = initialJobs): JobsApi {
  const [state, dispatch] = useReducer(jobsReducer, initial);
  const [now, setNow] = useState(() => Date.now());
  const current = useRef(state);
  current.current = state;

  useEffect(() => {
    let alive = true;
    let reading = true;
    const fence = snapshotFence(current.current.retention, current.current.jobs.map((job) => job.id), current.current.observedOrder);
    let observationOrder = current.current.observedOrder;
    const finished = new Map<number, SnapshotFinish>();
    const off = ipc.events.subscribe((event) => {
      const at = Date.now();
      const order = observationOrder = Math.max(observationOrder, current.current.observedOrder) + 1;
      if (reading && event.event === "started") observeSnapshotStart(fence, event.job);
      if (reading && event.event === "finished" && !fence.baselineIds.includes(event.job)
          && (fence.firstStarted === null || event.job < fence.firstStarted)) {
        finished.set(event.job, { job: event.job, status: event.status, error: event.error, at, order });
        if (finished.size > TERMINAL_JOB_LIMIT) finished.delete(finished.keys().next().value!);
      }
      dispatch({ type: "event", event, at, order });
    });
    ipc
      .request({ method: "jobs" })
      .then((jobs) => alive && dispatch({ type: "snapshots", jobs, at: Date.now(), finished: [...finished.values()], fence }))
      .catch((error: unknown) => alive && dispatch({ type: "request_failed", job: null, error: asIpcError(error), at: Date.now() }))
      .finally(() => { reading = false; finished.clear(); });
    return () => {
      alive = false;
      off();
    };
  }, [ipc]);

  // A display clock for elapsed and remaining time while something runs.
  const running = activeJobs(state).length > 0;
  useEffect(() => {
    if (!running) return;
    setNow(Date.now());
    const timer = window.setInterval(() => setNow(Date.now()), 1000);
    return () => window.clearInterval(timer);
  }, [running]);

  const cancel = useCallback(
    (job: JobId) => {
      dispatch({ type: "cancel_requested", job });
      ipc
        .request({ method: "job_cancel", params: { job } })
        .then((snapshot) => dispatch({ type: "snapshots", jobs: [snapshot], at: Date.now() }))
        .catch((error: unknown) => dispatch({ type: "request_failed", job, error: asIpcError(error), at: Date.now() }));
    },
    [ipc],
  );

  const clearHistory = useCallback(() => dispatch({ type: "clear_history" }), []);

  return { state, cancel, clearHistory, now };
}
