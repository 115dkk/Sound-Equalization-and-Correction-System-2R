import type { RequestFn } from "../ipc/context";
import { IpcFailure } from "../ipc/client";
import type { IpcError } from "../ipc/bindings/IpcError";
import type { JobEvent } from "../ipc/bindings/JobEvent";
import type { JobId } from "../ipc/bindings/JobId";
import type { JobKind } from "../ipc/bindings/JobKind";
import type { JobSnapshot } from "../ipc/bindings/JobSnapshot";
import type { JobStarted } from "../ipc/bindings/JobStarted";
import type { JobStatus } from "../ipc/bindings/JobStatus";
import type { Request } from "../ipc/bindings/Request";
import { initialRetention, isActiveJob, retainJobs, TERMINAL_JOB_LIMIT, snapshotFence, observeSnapshotStart, retiredFromSnapshot, type SnapshotFence, type JobRetention } from "./jobRetention";

export type FinishedJob = Extract<JobEvent, { event: "finished" }>;
/** Only the request fields that a screen or completion action actually needs. */
export type JobRequestContext = (
  | Extract<Request, { method: "measure_position" | "export" }>
  | { method: "calibrate"; params: { subs_only: boolean; mimo: boolean } }
  | { method: "sample_load" | "mdat_import" | "recommend_xo" | "mimo" }
  /** 적용 (ADR 0018): which target the `apply` job pushes to. */
  | { method: "apply_camilla" | "apply_eqapo" }
) & { epoch: number };
export type LedgerJob = JobSnapshot & {
  epoch: number; request: JobRequestContext | null;
  completionHandled: boolean; completionOrder: number | null;
  startedAt: number; stageStartedAt: number; stageStartDone: number; finishedAt: number | null;
};
export type HistoryEntry =
  | { id: string; at: number; job: JobId; kind: JobKind; type: "log"; code: string; params: Record<string, string> }
  | { id: string; at: number; job: JobId; kind: JobKind; type: "finished"; status: JobStatus; error: IpcError | null }
  | { id: string; at: number; job: JobId | null; kind: JobKind | null; type: "request_failed"; error: IpcError };
export type LedgerState = { jobs: LedgerJob[]; history: HistoryEntry[]; seq: number; observedOrder: number; retention: JobRetention };
export type SnapshotFinish = { event: FinishedJob; at: number; order: number };
export type LedgerAction =
  | { type: "snapshots"; jobs: readonly JobSnapshot[]; at: number; epoch: number; fence: SnapshotFence; finished: readonly SnapshotFinish[] }
  | { type: "event"; event: JobEvent; at: number; epoch?: number; order?: number }
  | { type: "reply"; value: JobStarted; context: JobRequestContext; at: number }
  | { type: "resolved"; job: JobId; epoch: number }
  | { type: "clear_history" }
  | { type: "missing"; job: JobId }
  | { type: "cancel_requested"; job: JobId }
  | { type: "cancel_reply"; snapshot: JobSnapshot; at: number }
  | { type: "request_failed"; job: JobId | null; error: IpcError; at: number };
export const initialLedger: LedgerState = { jobs: [], history: [], seq: 0, observedOrder: 0, retention: initialRetention };
const blank = (job: JobId, kind: JobKind, at: number, epoch: number): LedgerJob => ({
  job, kind, status: "running", cancellable: true, progress: null, error: null,
  epoch, request: null, completionHandled: false, completionOrder: null,
  startedAt: at, stageStartedAt: at, stageStartDone: 0, finishedAt: null,
});
function keep(state: LedgerState, jobs: LedgerJob[]): LedgerState {
  return { ...state, ...retainJobs(jobs, (job) => job.job, state.retention) };
}

type WithoutId<T> = T extends unknown ? Omit<T, "id"> : never;
function push(state: LedgerState, entry: WithoutId<HistoryEntry>): LedgerState {
  const seq = state.seq + 1;
  return { ...state, seq, history: [{ ...entry, id: `h${seq}` } as HistoryEntry, ...state.history].slice(0, 200) };
}

/** Pure lifecycle and retention rules, also used to replay gallery specimens. */
export function ledgerReducer(state: LedgerState, action: LedgerAction): LedgerState {
  if (action.type === "clear_history") return { ...state, history: [] };
  if (action.type === "missing") return { ...state, jobs: state.jobs.filter((job) => job.job !== action.job) };
  if (action.type === "cancel_requested") return { ...state, jobs: state.jobs.map((job) =>
    job.job === action.job && job.status === "running" ? { ...job, status: "cancel_requested" } : job) };
  if (action.type === "cancel_reply") {
    const snapshot = action.snapshot;
    const known = state.jobs.find((job) => job.job === snapshot.job);
    if (!known || !isActiveJob(known.status)) return state;
    const terminal = !isActiveJob(snapshot.status);
    const order = terminal ? state.observedOrder + 1 : state.observedOrder;
    const next = keep({ ...state, observedOrder: order }, state.jobs.map((job) => job.job === snapshot.job ? {
      ...job, status: job.status === "cancel_requested" && snapshot.status === "running" ? job.status : snapshot.status,
      cancellable: snapshot.cancellable, error: snapshot.error ?? job.error,
      finishedAt: terminal ? action.at : null, completionOrder: terminal ? order : null,
    } : job));
    return terminal && !TRAY_HIDDEN.includes(known.kind)
      ? push(next, { at: action.at, job: known.job, kind: known.kind, type: "finished", status: snapshot.status, error: snapshot.error }) : next;
  }
  if (action.type === "request_failed") {
    const kind = state.jobs.find((job) => job.job === action.job)?.kind ?? null;
    const next = { ...state, jobs: state.jobs.map((job) =>
      job.job === action.job && job.status === "cancel_requested" ? { ...job, status: "running" as const } : job) };
    return push(next, { type: "request_failed", at: action.at, job: action.job, kind, error: action.error });
  }
  if (action.type === "snapshots") {
    let jobs = state.jobs;
    for (const snapshot of action.jobs) {
      if (jobs.some((job) => job.job === snapshot.job) || retiredFromSnapshot(state.retention, snapshot.job, action.fence)) continue;
      const observed = action.finished.find(({ event }) => event.job === snapshot.job);
      const finished = observed?.event;
      const status = finished?.status ?? snapshot.status;
      jobs = [...jobs, {
        ...blank(snapshot.job, snapshot.kind, action.at, action.epoch), ...snapshot, status,
        stageStartDone: snapshot.progress?.done ?? 0, completionHandled: !!finished,
        completionOrder: isActiveJob(status) ? null : observed?.order ?? action.fence.sequence,
        finishedAt: isActiveJob(status) ? null : observed?.at ?? action.at,
        ...(finished ? { error: finished.error, cancellable: false } : {}),
      }];
    }
    return keep(state, jobs);
  }
  if (action.type === "resolved") {
    return { ...state, jobs: state.jobs.map((job) => job.job === action.job ? { ...job, epoch: action.epoch } : job) };
  }
  if (action.type === "reply") {
    const known = state.jobs.find((job) => job.job === action.value.job);
    if (known && !isActiveJob(known.status)) return state;
    const job = { ...(known ?? blank(action.value.job, action.value.kind, action.at, action.context.epoch)), epoch: action.context.epoch, request: action.context };
    return keep(state, [...state.jobs.filter((candidate) => candidate.job !== job.job), job]);
  }
  const { event, at } = action;
  const order = Math.max(action.order ?? 0, state.observedOrder + 1);
  const known = state.jobs.find((job) => job.job === event.job);
  if (event.event === "started") {
    if (known) return state;
    return keep({ ...state, observedOrder: order }, [...state.jobs, blank(event.job, event.kind, at, action.epoch ?? 0)]);
  }
  if (!known || (event.event === "finished" ? known.completionHandled : !isActiveJob(known.status))) return state;
  if (event.event === "progress") {
    const newStage = (known.progress?.total ?? 0) !== event.total || event.done < (known.progress?.done ?? 0);
    return { ...state, observedOrder: order, jobs: state.jobs.map((job) => job.job === event.job ? {
      ...job, progress: { stage: event.stage, done: event.done, total: event.total },
      stageStartedAt: newStage ? at : job.stageStartedAt, stageStartDone: newStage ? event.done : job.stageStartDone,
    } : job) };
  }
  if (event.event === "log") {
    if (TRAY_HIDDEN.includes(known.kind)) return state;
    return push({ ...state, observedOrder: order }, { at, job: event.job, kind: known.kind, type: "log", code: event.code, params: event.params });
  }
  if (event.event !== "finished") return state;
  const next = keep({ ...state, observedOrder: order }, [...state.jobs.filter((job) => job.job !== event.job), {
    ...known, status: event.status, error: event.error, cancellable: false, request: null,
    completionHandled: true, completionOrder: known.completionOrder ?? order, finishedAt: known.finishedAt ?? at,
  }]);
  return TRAY_HIDDEN.includes(known.kind) || !isActiveJob(known.status) ? next : push(next, { at, job: event.job, kind: known.kind, type: "finished", status: event.status, error: event.error });
}

/** These jobs have their own screen state rather than a row in the tray. */
export const TRAY_HIDDEN: readonly JobKind[] = ["level_monitor", "status_watch"];
export const trayJobs = (state: LedgerState): LedgerState => ({ ...state,
  jobs: state.jobs.filter((job) => !TRAY_HIDDEN.includes(job.kind)),
  history: state.history.filter((entry) => entry.kind === null || !TRAY_HIDDEN.includes(entry.kind)),
});
export const activeJobs = (state: LedgerState): LedgerJob[] => state.jobs.filter((job) => isActiveJob(job.status));
/** Milliseconds left at the current stage's rate, or null when unknown. */
export function remainingMs(job: LedgerJob, now: number): number | null {
  const { done = 0, total = 0 } = job.progress ?? {};
  if (total <= 0 || done >= total) return null;
  const doneInStage = done - job.stageStartDone;
  if (doneInStage <= 0) return null;
  return (now - job.stageStartedAt) / doneInStage * (total - done);
}
export const elapsedMs = (job: LedgerJob, now: number): number => (job.finishedAt ?? now) - job.startedAt;
export const errorCount = (state: LedgerState): number =>
  state.history.filter((entry) => (entry.type === "finished" && entry.status === "failed") || entry.type === "request_failed").length;

type Observation = { job: JobId; kind: JobKind; finished: { event: FinishedJob; job: LedgerJob } | null };
export type PendingStart = { context: JobRequestContext; kind: JobKind; observation: Observation | null; replied: boolean; afterId: number };
const kindFor = (request: Request): JobKind | null => {
  switch (request.method) {
    case "measure_position": return "measure";
    case "sample_load": case "mdat_import": case "calibrate": case "recommend_xo": case "mimo": case "export": return request.method;
    case "apply_camilla": case "apply_eqapo": return "apply";
    default: return null;
  }
};
const contextFor = (request: Request, epoch: number): JobRequestContext | null => {
  switch (request.method) {
    case "measure_position": case "export": return { ...request, epoch };
    case "calibrate": return { method: request.method, params: { subs_only: request.params.subs_only, mimo: request.params.mimo ?? false }, epoch };
    case "sample_load": case "mdat_import": case "recommend_xo": case "mimo": case "apply_camilla": case "apply_eqapo": return { method: request.method, epoch };
    default: return null;
  }
};

/** Pending promises own early completion payloads; retained jobs never do. */
export function createJobLedger(deps: {
  request: RequestFn; now: () => number;
  started: (job: LedgerJob) => void;
  finished: (job: LedgerJob, event: FinishedJob, request: JobRequestContext | null) => void;
}) {
  let state = initialLedger;
  let epoch = 0;
  let highestSeenId = 0;
  const pending = new Set<PendingStart>();
  const snapshotReads = new Set<{ fence: SnapshotFence; finished: Map<JobId, SnapshotFinish> }>();
  let observationOrder = 0;
  const listeners = new Set<() => void>();
  function dispatch(action: LedgerAction) {
    const next = ledgerReducer(state, action);
    if (next === state) return;
    state = next;
    listeners.forEach((listener) => listener());
  }
  function begin(request: Request): PendingStart | null {
    const context = contextFor(request, epoch);
    const kind = kindFor(request);
    if (!context || !kind) return null;
    const item: PendingStart = { context, kind, observation: null, replied: false, afterId: highestSeenId };
    pending.add(item); return item;
  }
  function reply(item: PendingStart, value: JobStarted) {
    if (item.replied) return;
    item.replied = true;
    highestSeenId = Math.max(highestSeenId, value.job);
    // Same-kind events have provisional owners until the reply identifies them.
    const owner = [...pending].find((candidate) => candidate.observation?.job === value.job);
    if (owner && owner !== item) {
      const observation = item.observation;
      item.observation = owner.observation; owner.observation = observation;
    }
    const known = state.jobs.find((job) => job.job === value.job);
    const finished = item.observation?.job === value.job ? item.observation.finished : null;
    if (finished) {
      if (known) dispatch({ type: "resolved", job: value.job, epoch: item.context.epoch });
      const job = { ...finished.job, epoch: item.context.epoch };
      deps.started(job);
      deps.finished(job, finished.event, item.context);
      return;
    }
    if (known && !isActiveJob(known.status)) return;
    dispatch({ type: "reply", value, context: item.context, at: deps.now() });
    deps.started(state.jobs.find((job) => job.job === value.job)!);
  }
  function event(event: JobEvent) {
    const at = deps.now();
    const order = observationOrder = Math.max(observationOrder, state.observedOrder) + 1;
    const known = state.jobs.find((job) => job.job === event.job);
    if (event.event === "started") {
      if (known) return;
      const owner = [...pending].find((item) => item.kind === event.kind && item.observation === null && event.job > item.afterId);
      highestSeenId = Math.max(highestSeenId, event.job);
      if (owner) owner.observation = { job: event.job, kind: event.kind, finished: null };
      for (const read of snapshotReads) observeSnapshotStart(read.fence, event.job);
      dispatch({ type: "event", event, at, order, epoch: owner?.context.epoch ?? epoch });
      if (!owner) deps.started(state.jobs.find((job) => job.job === event.job)!);
      return;
    }
    if (event.event === "finished" && !known) {
      for (const read of snapshotReads) {
        if (retiredFromSnapshot(state.retention, event.job, read.fence)) continue;
        if (!read.finished.has(event.job)) read.finished.set(event.job, { event, at, order });
        if (read.finished.size > TERMINAL_JOB_LIMIT) read.finished.delete(read.finished.keys().next().value!);
      }
      return;
    }
    if (event.event !== "finished") { dispatch({ type: "event", event, at, order }); return; }
    if (!known || known.completionHandled) return;
    const owner = [...pending].find((item) => item.observation?.job === event.job);
    dispatch({ type: "event", event, at, order });
    const terminal = state.jobs.find((job) => job.job === event.job)!;
    if (!known.request && owner?.observation) owner.observation.finished = { event, job: terminal };
    else deps.finished(terminal, event, known.request);
  }
  /** Startup reads preserve pushed events. External jobs have no UI request
   * channel, so their snapshots also reconcile progress and completion. */
  async function readSnapshots(options?: { reconcile: JobId } | { external: true }) {
    const projectEpoch = epoch;
    const externalKnown = options && "external" in options ? state.jobs.filter((job) => !job.request && isActiveJob(job.status)).map((job) => job.job) : [];
    const read = { fence: snapshotFence(state.retention, state.jobs.map((job) => job.job), observationOrder), finished: new Map<JobId, SnapshotFinish>() };
    snapshotReads.add(read);
    try {
      const jobs = await deps.request({ method: "jobs" });
      for (const job of jobs) highestSeenId = Math.max(highestSeenId, job.job);
      // Startup getters recover current results; old completion payloads only
      // recover lifecycle metadata, never replay side effects into this project.
      dispatch({ type: "snapshots", jobs, at: deps.now(), epoch: projectEpoch, fence: read.fence, finished: [...read.finished.values()] });
      // TD-102: an explicit monitor-stop timeout may reconcile its one job.
      // Ordinary startup reads never overwrite events observed during the read.
      if (options && "external" in options) {
        for (const id of externalKnown) {
          const known = state.jobs.find((job) => job.job === id);
          if (known && !known.request && isActiveJob(known.status) && !jobs.some((job) => job.job === id)) dispatch({ type: "missing", job: id });
        }
        for (const current of jobs) {
          const known = state.jobs.find((job) => job.job === current.job);
          if (!known || known.request || !isActiveJob(known.status)) continue;
          if (!isActiveJob(current.status)) event({ event: "finished", job: current.job, status: current.status, error: current.error, result: null });
          else if (current.progress) event({ event: "progress", job: current.job, ...current.progress });
        }
      }
      if (options && "reconcile" in options) {
        const current = jobs.find((job) => job.job === options.reconcile);
        const known = state.jobs.find((job) => job.job === options.reconcile);
        if (known && isActiveJob(known.status)) {
          if (!current) dispatch({ type: "missing", job: options.reconcile });
          else if (!isActiveJob(current.status)) event({ event: "finished", job: current.job, status: current.status, error: current.error, result: null });
        }
      }
    } catch (error) {
      requestFailed(null, error);
      throw error;
    } finally { snapshotReads.delete(read); read.finished.clear(); }
  }
  /** Refusals are recorded in history and resolve after reverting the optimistic state. */
  async function cancel(job: JobId) {
    dispatch({ type: "cancel_requested", job });
    try {
      const snapshot = await deps.request({ method: "job_cancel", params: { job } });
      // A snapshot has no result payload. Leave completion handling to Finished
      // (notably an export that won the race with cancel), while updating the row.
      dispatch({ type: "cancel_reply", snapshot, at: deps.now() });
    }
    catch (error) { requestFailed(job, error); }
  }
  function requestFailed(job: JobId | null, error: unknown) {
    const value = error instanceof IpcFailure ? error.error : { code: "internal" as const, params: {}, retryable: false, detail: String(error) };
    dispatch({ type: "request_failed", job, error: value, at: deps.now() });
  }
  function settle(item: PendingStart | null) {
    if (!item) return;
    pending.delete(item);
    if (!item.replied && item.observation) {
      const next = [...pending].find((candidate) => candidate.kind === item.kind && candidate.observation === null && item.observation!.job > candidate.afterId);
      if (next) next.observation = item.observation;
    }
  }
  return { getSnapshot: () => state, subscribe: (listener: () => void) => { listeners.add(listener); return () => { listeners.delete(listener); }; },
    begin, reply, settle, event, readSnapshots, cancel, clearHistory: () => dispatch({ type: "clear_history" }), get epoch() { return epoch; }, replaceProject: () => { epoch++; },
    notFound: (job: JobId): IpcError => ({ code: "job_not_found", params: { job: String(job) }, retryable: false, detail: "Job metadata is no longer retained." }),
  };
}
