// Job tray state, built only from what the service pushes (JobEvent) and
// the snapshots `jobs` and `job_cancel` return. No polling: elapsed time is
// computed from the clock at render, the estimate from the progress rate of
// the current stage.
import { initialRetention, isActiveJob, retainJobs, retiredFromSnapshot, type SnapshotFence, type JobRetention } from "../../state/jobRetention";
import type { IpcError } from "../../ipc/bindings/IpcError";
import type { JobEvent } from "../../ipc/bindings/JobEvent";
import type { JobId } from "../../ipc/bindings/JobId";
import type { JobKind } from "../../ipc/bindings/JobKind";
import type { JobSnapshot } from "../../ipc/bindings/JobSnapshot";
import type { JobStatus } from "../../ipc/bindings/JobStatus";

export type JobView = {
  id: JobId;
  kind: JobKind;
  status: JobStatus;
  stage: string | null;
  done: number;
  /** 0 when the stage has no known size (indeterminate). */
  total: number;
  startedAt: number;
  stageStartedAt: number;
  stageStartDone: number;
  finishedAt: number | null;
  completionOrder: number | null;
  cancellable: boolean;
  error: IpcError | null;
};

export type HistoryEntry =
  | { id: string; at: number; job: JobId; kind: JobKind; type: "log"; code: string; params: Record<string, string> }
  | { id: string; at: number; job: JobId; kind: JobKind; type: "finished"; status: JobStatus; error: IpcError | null }
  | { id: string; at: number; job: JobId | null; kind: JobKind | null; type: "request_failed"; error: IpcError };

export type JobsState = { jobs: JobView[]; history: HistoryEntry[]; seq: number; observedOrder: number; retention: JobRetention };

export type SnapshotFinish = { job: JobId; status: JobStatus; error: IpcError | null; at: number; order: number };

export type JobsAction =
  | { type: "event"; event: JobEvent; at: number; order?: number }
  | { type: "snapshots"; jobs: readonly JobSnapshot[]; at: number; finished?: readonly SnapshotFinish[]; fence?: SnapshotFence }
  | { type: "cancel_requested"; job: JobId }
  | { type: "request_failed"; job: JobId | null; error: IpcError; at: number }
  | { type: "clear_history" };

const HISTORY_LIMIT = 200;

export const initialJobs: JobsState = { jobs: [], history: [], seq: 0, observedOrder: 0, retention: initialRetention };

/** Background jobs the tray does not list: the level monitor shows on its
 * screen as the meter, and the status watch runs whenever the app does. */
const TRAY_HIDDEN: readonly JobKind[] = ["level_monitor", "status_watch"];


const blank = (id: JobId, kind: JobKind, at: number): JobView => ({
  id,
  kind,
  status: "running",
  stage: null,
  done: 0,
  total: 0,
  startedAt: at,
  stageStartedAt: at,
  stageStartDone: 0,
  finishedAt: null,
  completionOrder: null,
  cancellable: true,
  error: null,
});

type WithoutId<T> = T extends unknown ? Omit<T, "id"> : never;

function push(state: JobsState, entry: WithoutId<HistoryEntry>): JobsState {
  const seq = state.seq + 1;
  const history = [{ ...entry, id: `h${seq}` } as HistoryEntry, ...state.history].slice(0, HISTORY_LIMIT);
  return { ...state, history, seq };
}

function update(state: JobsState, id: JobId, change: (job: JobView) => JobView): JobsState {
  return { ...state, jobs: state.jobs.map((job) => (job.id === id ? change(job) : job)) };
}

function keep(state: JobsState, jobs: JobView[]): JobsState {
  return { ...state, ...retainJobs(jobs, (job) => job.id, state.retention) };
}

export function jobsReducer(state: JobsState, action: JobsAction): JobsState {
  switch (action.type) {
    case "event": {
      const { event, at } = action;
      const order = Math.max(action.order ?? 0, state.observedOrder + 1);
      const known = state.jobs.find((job) => job.id === event.job);
      if (event.event === "started") {
        // Started is emitted once per job; allocated ids can arrive out of order.
        // Only an actually retained id is a known duplicate of this event.
        if (known || TRAY_HIDDEN.includes(event.kind)) return state;
        return keep({ ...state, observedOrder: order }, [...state.jobs, blank(event.job, event.kind, at)]);
      }
      // Meter readings and path status are screen data, not job history.
      if (!known || !isActiveJob(known.status) || event.event === "meter" || event.event === "measure_status") return state;
      state = { ...state, observedOrder: order };
      if (event.event === "progress") {
        return update(state, event.job, (job) => {
          // A new stage restarts its count (engine stages) or changes its size;
          // measurement names each role as its stage but keeps one count.
          const newStage = job.total !== event.total || event.done < job.done;
          return {
            ...job,
            stage: event.stage,
            done: event.done,
            total: event.total,
            stageStartedAt: newStage ? at : job.stageStartedAt,
            stageStartDone: newStage ? event.done : job.stageStartDone,
          };
        });
      }
      if (event.event === "log") {
        return push(state, { at, job: event.job, kind: known.kind, type: "log", code: event.code, params: event.params });
      }
      // finished
      const next = keep(state, [...state.jobs.filter((job) => job.id !== event.job), { ...known, status: event.status, error: event.error, finishedAt: at, completionOrder: order, cancellable: false }]);
      return push(next, { at, job: event.job, kind: known.kind, type: "finished", status: event.status, error: event.error });
    }
    case "snapshots": {
      let next = state;
      // Use the fence before this response: an older, previously unseen active
      // job in the initial snapshot must survive newer local completions.
      const fence = state.retention;
      for (const original of action.jobs) {
        const finished = action.finished?.find((event) => event.job === original.job);
        const snap = finished ? { ...original, status: finished.status, error: finished.error, cancellable: false } : original;
        if (TRAY_HIDDEN.includes(snap.kind)) continue;
        const known = next.jobs.find((job) => job.id === snap.job);
        if (known && !isActiveJob(known.status)) continue;
        if (!known && retiredFromSnapshot(fence, snap.job, action.fence)) continue;
        const progress = snap.progress;
        if (!known) {
          next = keep({ ...next, observedOrder: Math.max(next.observedOrder, finished?.order ?? 0) }, [...next.jobs, {
            ...blank(snap.job, snap.kind, action.at), status: snap.status,
            cancellable: snap.cancellable, stage: progress?.stage ?? null,
            done: progress?.done ?? 0, total: progress?.total ?? 0,
            stageStartDone: progress?.done ?? 0, error: snap.error,
            finishedAt: isActiveJob(snap.status) ? null : finished?.at ?? action.at,
            completionOrder: isActiveJob(snap.status) ? null : finished?.order ?? action.fence?.sequence ?? 0,
          }]);
        } else {
          const status = known.status === "cancel_requested" && snap.status === "running" ? known.status : snap.status;
          const order = isActiveJob(status) ? next.observedOrder : next.observedOrder + 1;
          const updated = { ...known, status, cancellable: snap.cancellable, error: snap.error ?? known.error,
            finishedAt: isActiveJob(snap.status) ? null : action.at, completionOrder: isActiveJob(snap.status) ? null : order };
          next = keep({ ...next, observedOrder: order }, [...next.jobs.filter((job) => job.id !== snap.job), updated]);
          if (!isActiveJob(snap.status)) next = push(next, { at: action.at, job: snap.job, kind: snap.kind, type: "finished", status: snap.status, error: snap.error });
        }
      }
      return next;
    }
    case "cancel_requested":
      return update(state, action.job, (job) => (job.status === "running" ? { ...job, status: "cancel_requested" } : job));
    case "request_failed": {
      const kind = action.job === null ? null : (state.jobs.find((job) => job.id === action.job)?.kind ?? null);
      // A refused job_cancel leaves the job running; the button must offer cancel again.
      const reverted = action.job === null ? state : update(state, action.job, (job) => (job.status === "cancel_requested" ? { ...job, status: "running" } : job));
      return push(reverted, { at: action.at, job: action.job, kind, type: "request_failed", error: action.error });
    }
    case "clear_history":
      return { ...state, history: [] };
  }
}

export const activeJobs = (state: JobsState): JobView[] => state.jobs.filter((job) => isActiveJob(job.status));

/** Milliseconds left at the current stage's rate, or null when unknown. */
export function remainingMs(job: JobView, now: number): number | null {
  if (job.total <= 0 || job.done >= job.total) return null;
  const doneInStage = job.done - job.stageStartDone;
  if (doneInStage <= 0) return null;
  const perUnit = (now - job.stageStartedAt) / doneInStage;
  return perUnit * (job.total - job.done);
}

export const elapsedMs = (job: JobView, now: number): number => (job.finishedAt ?? now) - job.startedAt;

/** Errors in the history the user has not opened yet are counted by the tray. */
export const errorCount = (state: JobsState): number =>
  state.history.filter((entry) => (entry.type === "finished" && entry.status === "failed") || entry.type === "request_failed").length;
