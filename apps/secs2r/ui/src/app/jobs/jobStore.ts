// Job tray state, built only from what the service pushes (JobEvent) and
// the snapshots `jobs` and `job_cancel` return. No polling: elapsed time is
// computed from the clock at render, the estimate from the progress rate of
// the current stage.
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
  cancellable: boolean;
  error: IpcError | null;
};

export type HistoryEntry =
  | { id: string; at: number; job: JobId; kind: JobKind; type: "log"; code: string; params: Record<string, string> }
  | { id: string; at: number; job: JobId; kind: JobKind; type: "finished"; status: JobStatus; error: IpcError | null }
  | { id: string; at: number; job: JobId | null; kind: JobKind | null; type: "request_failed"; error: IpcError };

export type JobsState = { jobs: JobView[]; history: HistoryEntry[]; seq: number };

export type JobsAction =
  | { type: "event"; event: JobEvent; at: number }
  | { type: "snapshots"; jobs: readonly JobSnapshot[]; at: number }
  | { type: "cancel_requested"; job: JobId }
  | { type: "request_failed"; job: JobId | null; error: IpcError; at: number }
  | { type: "clear_history" };

export const HISTORY_LIMIT = 200;

export const initialJobs: JobsState = { jobs: [], history: [], seq: 0 };

export const isActive = (status: JobStatus) => status === "running" || status === "cancel_requested";

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

export function jobsReducer(state: JobsState, action: JobsAction): JobsState {
  switch (action.type) {
    case "event": {
      const { event, at } = action;
      const known = state.jobs.find((job) => job.id === event.job);
      if (event.event === "started") {
        if (known) return state;
        return { ...state, jobs: [...state.jobs, blank(event.job, event.kind, at)] };
      }
      if (!known) return state;
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
      const next = update(state, event.job, (job) => ({ ...job, status: event.status, error: event.error, finishedAt: at, cancellable: false }));
      return push(next, { at, job: event.job, kind: known.kind, type: "finished", status: event.status, error: event.error });
    }
    case "snapshots": {
      let next = state;
      for (const snap of action.jobs) {
        const known = next.jobs.find((job) => job.id === snap.job);
        const progress = snap.progress;
        if (!known) {
          const view = blank(snap.job, snap.kind, action.at);
          next = {
            ...next,
            jobs: [
              ...next.jobs,
              {
                ...view,
                status: snap.status,
                cancellable: snap.cancellable,
                stage: progress?.stage ?? null,
                done: progress?.done ?? 0,
                total: progress?.total ?? 0,
                stageStartDone: progress?.done ?? 0,
                error: snap.error,
                finishedAt: isActive(snap.status) ? null : action.at,
              },
            ],
          };
        } else {
          next = update(next, snap.job, (job) => ({
            ...job,
            status: isActive(job.status) || job.finishedAt === null ? snap.status : job.status,
            cancellable: snap.cancellable,
            error: snap.error ?? job.error,
          }));
        }
      }
      return next;
    }
    case "cancel_requested":
      return update(state, action.job, (job) => (job.status === "running" ? { ...job, status: "cancel_requested" } : job));
    case "request_failed": {
      const kind = action.job === null ? null : (state.jobs.find((job) => job.id === action.job)?.kind ?? null);
      return push(state, { at: action.at, job: action.job, kind, type: "request_failed", error: action.error });
    }
    case "clear_history":
      return { ...state, history: [] };
  }
}

export const activeJobs = (state: JobsState): JobView[] => state.jobs.filter((job) => isActive(job.status));

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
