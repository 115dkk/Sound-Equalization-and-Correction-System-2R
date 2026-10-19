import type { JobStatus } from "../ipc/bindings/JobStatus";

export const TERMINAL_JOB_LIMIT = 32;
export const isActiveJob = (status: JobStatus) => status === "running" || status === "cancel_requested";

/** Job ids are allocated monotonically, but events can arrive out of order.
 * These scalars fence old snapshot replies, not fresh Started events or an
 * accepted local start reply. Known active jobs always take precedence. */
export type JobRetention = { firstSeen: number | null; retiredThrough: number };
export const initialRetention: JobRetention = { firstSeen: null, retiredThrough: 0 };
export const wasRetired = (retention: JobRetention, id: number) =>
  retention.firstSeen !== null && id >= retention.firstSeen && id <= retention.retiredThrough;

/** Input order is observation order; a newly finished job moves to the end. */
export function retainJobs<T extends { status: JobStatus; completionOrder: number | null }>(jobs: readonly T[], idOf: (job: T) => number, retention: JobRetention) {
  const ordered = [...jobs].sort((a, b) => isActiveJob(a.status)
    ? isActiveJob(b.status) ? 0 : -1 : isActiveJob(b.status) ? 1 : (a.completionOrder ?? 0) - (b.completionOrder ?? 0));
  let terminal = 0;
  let retiredThrough = retention.retiredThrough;
  let firstSeen = retention.firstSeen;
  const kept: T[] = [];
  for (let i = ordered.length - 1; i >= 0; i--) {
    const job = ordered[i]!;
    const id = idOf(job);
    firstSeen = firstSeen === null ? id : Math.min(firstSeen, id);
    if (isActiveJob(job.status) || terminal++ < TERMINAL_JOB_LIMIT) kept.push(job);
    else retiredThrough = Math.max(retiredThrough, id);
  }
  return { jobs: kept.reverse(), retention: { firstSeen, retiredThrough } };
}

/** One bounded fence belongs to one snapshot request. Jobs unknown when that
 * request began may be older active jobs, not retired local completions. */
export type SnapshotFence = { before: JobRetention; baselineIds: number[]; firstStarted: number | null; sequence: number };
export const snapshotFence = (retention: JobRetention, ids: number[], sequence = 0): SnapshotFence => ({ before: retention, baselineIds: ids, firstStarted: null, sequence });
export function observeSnapshotStart(fence: SnapshotFence, id: number) {
  fence.firstStarted = fence.firstStarted === null ? id : Math.min(fence.firstStarted, id);
}
export function retiredFromSnapshot(retention: JobRetention, id: number, fence?: SnapshotFence) {
  if (!fence) return wasRetired(retention, id);
  return wasRetired(fence.before, id) || (wasRetired(retention, id)
    && (fence.baselineIds.includes(id) || (fence.firstStarted !== null && id >= fence.firstStarted)));
}
