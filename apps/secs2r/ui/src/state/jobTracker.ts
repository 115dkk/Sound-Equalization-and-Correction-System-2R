import type { IpcError } from "../ipc/bindings/IpcError";
import type { JobEvent } from "../ipc/bindings/JobEvent";
import type { JobKind } from "../ipc/bindings/JobKind";
import type { JobSnapshot } from "../ipc/bindings/JobSnapshot";
import type { JobStarted } from "../ipc/bindings/JobStarted";
import type { Request } from "../ipc/bindings/Request";
import { initialRetention, isActiveJob, retainJobs, TERMINAL_JOB_LIMIT, snapshotFence, observeSnapshotStart, retiredFromSnapshot, type SnapshotFence } from "./jobRetention";

export type FinishedJob = Extract<JobEvent, { event: "finished" }>;
/** Only the request fields that a screen or completion action actually needs. */
export type JobRequestContext = (
  | Extract<Request, { method: "measure_position" | "export" }>
  | { method: "calibrate"; params: { subs_only: boolean } }
  | { method: "sample_load" | "mdat_import" | "recommend_xo" | "mimo" | "level_check" }
) & { epoch: number };
export type SessionJob = JobSnapshot & { epoch: number; completionHandled: boolean; completionOrder: number | null };
type State = { jobs: SessionJob[]; jobRequests: Record<number, JobRequestContext> };
type Observation = { job: number; kind: JobKind; finished: FinishedJob | null };
type SnapshotRead = { epoch: number; fence: SnapshotFence; finished: Map<number, { event: FinishedJob; order: number }> };
type PendingStart = { context: JobRequestContext; kind: JobKind; observation: Observation | null; replied: boolean; afterId: number };
const kindFor = (request: Request): JobKind | null => {
  switch (request.method) {
    case "measure_position": return "measure";
    case "sample_load": case "mdat_import": case "calibrate": case "recommend_xo": case "mimo": case "export": case "level_check": return request.method;
    default: return null;
  }
};
const contextFor = (request: Request, epoch: number): JobRequestContext | null => {
  switch (request.method) {
    case "measure_position": case "export": return { ...request, epoch };
    case "calibrate": return { method: request.method, params: { subs_only: request.params.subs_only }, epoch };
    case "sample_load": case "mdat_import": case "recommend_xo": case "mimo": case "level_check": return { method: request.method, epoch };
    default: return null;
  }
};

/** Retained metadata is bounded; early completions belong to the promise of
 * their own pending start, never to an ever-growing map of external jobs. */
export function createJobTracker(get: () => State, set: (state: State) => void,
  complete: (job: SessionJob, event: FinishedJob, context: JobRequestContext | undefined) => void,
  started: (job: SessionJob) => void,
) {
  let epoch = 0;
  let sequence = 0;
  let highestSeenId = 0;
  let retention = initialRetention;
  const pending = new Set<PendingStart>();
  const snapshotReads = new Set<SnapshotRead>();
  function publish(jobs: SessionJob[], contexts = get().jobRequests) {
    const bounded = retainJobs(jobs, (job) => job.job, retention);
    retention = bounded.retention;
    const retained = new Set(bounded.jobs.map((job) => job.job));
    set({ jobs: bounded.jobs, jobRequests: Object.fromEntries(Object.entries(contexts).filter(([id]) => retained.has(Number(id)))) });
  }
  const blank = (job: number, kind: JobKind, projectEpoch: number): SessionJob => ({
    job, kind, status: "running", cancellable: true, progress: null, error: null, epoch: projectEpoch, completionHandled: false, completionOrder: null,
  });
  function begin(request: Request): PendingStart | null {
    const context = contextFor(request, epoch);
    const kind = kindFor(request);
    if (!context || !kind) return null;
    const item: PendingStart = { context, kind, observation: null, replied: false, afterId: highestSeenId };
    pending.add(item); return item;
  }
  function observeStart(job: number, kind: JobKind) {
    const state = get();
    const owner = [...pending].find((item) => item.kind === kind && item.observation === null && job > item.afterId);
    // Allocation order is not delivery order. A previously unseen Started is
    // evidence of a real operation, even inside a retired snapshot-id range.
    if (state.jobs.some((known) => known.job === job)) return;
    highestSeenId = Math.max(highestSeenId, job);
    if (owner) owner.observation = { job, kind, finished: null };
    for (const read of snapshotReads) observeSnapshotStart(read.fence, job);
    const value = blank(job, kind, state.jobRequests[job]?.epoch ?? owner?.context.epoch ?? epoch);
    publish([...state.jobs, value]);
    if (!owner) started(value);
  }
  function reply(item: PendingStart, value: JobStarted) {
    if (item.replied) return;
    item.replied = true;
    highestSeenId = Math.max(highestSeenId, value.job);
    // Concurrent starts can return in a different order from their events.
    // Reassign the observation, not the request's project identity.
    const owner = [...pending].find((candidate) => candidate.observation?.job === value.job);
    if (owner && owner !== item) {
      const observation = item.observation;
      item.observation = owner.observation; owner.observation = observation;
    }
    const state = get();
    const known = state.jobs.find((job) => job.job === value.job);
    const finished = item.observation?.job === value.job ? item.observation.finished : null;
    if (finished) {
      const job = { ...(known ?? blank(value.job, value.kind, item.context.epoch)), epoch: item.context.epoch,
        status: finished.status, error: finished.error, cancellable: false, completionHandled: true };
      // A deferred reply must not resurrect a terminal record already retired.
      if (known) publish(state.jobs.map((candidate) => candidate.job === value.job ? job : candidate));
      started(job);
      complete(job, finished, item.context);
      return;
    }
    if (known && !isActiveJob(known.status)) return;
    // This accepted reply identifies our pending operation. A snapshot fence
    // cannot prove it completed; a captured early finish above can.
    const job = { ...(known ?? blank(value.job, value.kind, item.context.epoch)), epoch: item.context.epoch };
    publish([...state.jobs.filter((candidate) => candidate.job !== value.job), job], { ...state.jobRequests, [value.job]: item.context });
    started(job);
  }
  function finish(event: FinishedJob) {
    const state = get();
    const job = state.jobs.find((candidate) => candidate.job === event.job);
    if (!job) {
      const order = ++sequence;
      for (const read of snapshotReads) {
        if (retiredFromSnapshot(retention, event.job, read.fence)) continue;
        if (!read.finished.has(event.job)) read.finished.set(event.job, { event, order });
        if (read.finished.size > TERMINAL_JOB_LIMIT) read.finished.delete(read.finished.keys().next().value!);
      }
      return;
    }
    if (job.completionHandled) return;
    const context = state.jobRequests[event.job];
    const owner = [...pending].find((item) => item.observation?.job === event.job);
    const terminal = { ...job, status: event.status, error: event.error, cancellable: false, completionHandled: true, completionOrder: ++sequence };
    const contexts = { ...state.jobRequests }; delete contexts[event.job];
    publish([...state.jobs.filter((candidate) => candidate.job !== event.job), terminal], contexts);
    if (!context && owner?.observation) owner.observation.finished = event;
    else complete(terminal, event, context);
  }
  function progress(event: Extract<JobEvent, { event: "progress" }>) {
    const state = get();
    const job = state.jobs.find((candidate) => candidate.job === event.job);
    if (!job || !isActiveJob(job.status)) return;
    publish(state.jobs.map((candidate) => candidate.job === event.job
      ? { ...candidate, progress: { stage: event.stage, done: event.done, total: event.total } } : candidate));
  }
  function beginSnapshots(): SnapshotRead {
    const read = { epoch, fence: snapshotFence(retention, get().jobs.map((job) => job.job), sequence), finished: new Map<number, { event: FinishedJob; order: number }>() };
    snapshotReads.add(read); return read;
  }
  function endSnapshots(read: SnapshotRead) { snapshotReads.delete(read); read.finished.clear(); }
  function snapshots(snapshots: readonly JobSnapshot[], read?: SnapshotRead) {
    const fence = retention;
    let jobs = get().jobs;
    for (const snapshot of snapshots) {
      highestSeenId = Math.max(highestSeenId, snapshot.job);
      const known = jobs.find((candidate) => candidate.job === snapshot.job);
      // Events observed during this read are newer than its response.
      if (known) continue;
      if (retiredFromSnapshot(fence, snapshot.job, read?.fence)) continue;
      const observation = read?.finished.get(snapshot.job);
      const finished = observation?.event;
      const value = { ...snapshot, epoch: read?.epoch ?? epoch, completionHandled: !!finished,
        completionOrder: isActiveJob(finished?.status ?? snapshot.status) ? null : observation?.order ?? read?.fence.sequence ?? sequence,
        ...(finished ? { status: finished.status, error: finished.error, cancellable: false } : {}) };
      jobs = [...jobs, value];
    }
    // Unknown pre-window completions recover lifecycle metadata only. Startup
    // getters read current project data; replaying old XO/level/export payloads
    // here could replace newer results already observed during the read.
    publish(jobs);
  }
  function settle(item: PendingStart | null) {
    if (!item) return;
    pending.delete(item);
    // Kind matching before a reply is provisional: another same-kind request
    // may have succeeded while this one is rejected as busy. Its observation
    // remains owned by the still-pending group until the real reply identifies it.
    if (!item.replied && item.observation) {
      const next = [...pending].find((candidate) => candidate.kind === item.kind && candidate.observation === null && item.observation!.job > candidate.afterId);
      if (next) next.observation = item.observation;
    }
  }
  const notFound = (job: number): IpcError => ({ code: "job_not_found", params: { job: String(job) }, retryable: false, detail: "Job metadata is no longer retained." });
  return { begin, reply, settle, observeStart, finish, progress, snapshots, beginSnapshots, endSnapshots,
    get epoch() { return epoch; }, replaceProject: () => { epoch++; }, notFound,
  };
}
