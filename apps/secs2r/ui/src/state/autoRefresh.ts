import type { LedgerJob } from "./jobLedger";
import type { JobKind } from "../ipc/bindings/JobKind";
import { isActiveJob } from "./jobRetention";

const MEASURING: readonly JobKind[] = ["measure", "sample_load", "mdat_import", "level_monitor"];

/** One attempt per input revision, not a retry loop for a failed calculation. */
export function createAutoRefresh(deps: {
  enabled: () => boolean;
  stale: () => boolean;
  blocked: () => boolean;
  jobs: () => readonly LedgerJob[];
  start: () => void;
  cancel: (job: number) => void;
}) {
  let active = false;
  let enabled = false;
  let measuring = false;
  let revision = 0;
  let attempted = -1;
  let changedAt = Date.now();
  let timer: ReturnType<typeof setTimeout> | undefined;
  const cancelling = new Set<number>();
  const clear = () => { clearTimeout(timer); timer = undefined; };
  function check() {
    clear();
    if (!active) return;
    const next = deps.enabled();
    if (next !== enabled) { enabled = next; revision++; changedAt = Date.now(); }
    const jobs = deps.jobs().filter((job) => isActiveJob(job.status));
    const nowMeasuring = jobs.some((job) => MEASURING.includes(job.kind));
    // Invalidate even before measurement data arrives. A monitor may finish
    // without changing inputs, so this revision also permits a fresh attempt.
    if (nowMeasuring && !measuring) { revision++; changedAt = Date.now(); }
    measuring = nowMeasuring;
    for (const job of jobs) {
      const own = job.request?.autoRefreshRevision;
      if (own !== undefined && (measuring || own !== revision) && !cancelling.has(job.job)) {
        cancelling.add(job.job);
        deps.cancel(job.job);
      }
    }
    for (const id of cancelling) if (!jobs.some((job) => job.job === id)) cancelling.delete(id);
    // status_watch is the always-running notification stream, not user work.
    if (!enabled || !deps.stale() || deps.blocked() || jobs.some((job) => job.kind !== "status_watch") || attempted === revision) return;
    timer = setTimeout(() => {
      timer = undefined;
      // The timer never starts from an old snapshot, even if a request began at its deadline.
      if (!active || !deps.enabled() || !deps.stale() || deps.blocked() || deps.jobs().some((job) => job.kind !== "status_watch" && isActiveJob(job.status))) return;
      attempted = revision;
      deps.start();
    }, Math.max(0, changedAt + 1500 - Date.now()));
  }
  return {
    get revision() { return revision; },
    canContinue: (value: number) => active && deps.enabled() && !measuring && value === revision,
    defer: () => { attempted = -1; check(); },
    changed: () => { revision++; changedAt = Date.now(); check(); },
    check,
    start: () => { active = true; check(); },
    dispose: () => { active = false; clear(); },
  };
}
