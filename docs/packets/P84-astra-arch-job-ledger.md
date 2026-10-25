# P84 (ASTRA): audit #92 architecture C12 — one job ledger, the tray is its view (A-77, with TD-72)

Worktree: `/home/user/wt-ui` (a branch Claude names in the run brief). Work
only there. Do not run cargo, do not push, do not spawn sub-agents. Claude
reviews, re-runs the gates and re-authors the commits.

Scope rule: state, call code, hooks, mock, fixtures, specimens' data and
tests. No markup changes in component bodies beyond swapping where a value
comes from, no CSS, no layout. The tray (`app/jobs/JobTray.tsx`) and every
screen must render the same DOM for the same job states as today.

Read first: `CLAUDE.md`, `docs/adr/0009-compact-result-ipc-and-job-retention.md`
(retention 32, snapshot fence, late start, cancel), `docs/DESIGN.md` §7 (one
`jobs` read plus events, no polling), the architecture review's candidate 12
(`TECH_DEBT_AUDIT.md` does not list it; the problem statement is below),
`TECH_DEBT_AUDIT.md` row TD-72, and these files in full:
`src/state/{jobTracker.ts,jobRetention.ts,session.ts}`,
`src/app/jobs/{jobStore.ts,useJobs.ts,JobTray.tsx,jobText.ts}`,
`src/state/{jobTracker.test.ts,sessionJobs.test.ts,session.test.ts}`,
`src/app/jobs/{jobStore.test.ts,useJobs.test.tsx,JobTray.test.tsx}`,
`src/gallery/frameSpecimens.tsx`, `src/screens/*/specimens.tsx` (their job
states), `src/screens/*/use*Screen.ts` (their `host.jobs` reads),
`src/screens/measure/useMeasureScreen.ts` (`cancelJob`).

## Problem

ADR 0009's job rules are implemented twice over the same event bus:
`state/jobTracker.ts` (session: request/reply matching, project epoch,
completion effects, snapshot fence) and `app/jobs/jobStore.ts` + `useJobs.ts`
(tray: timing, history, tray-hidden kinds, optimistic cancel, its own
snapshot fence and its own `jobs` read). The two lists differ. A cancel from
the measure screen (`useMeasureScreen.ts:20`, a bare `job_cancel` call) never
shows 취소하는 중 anywhere, while the tray's cancel does (TD-72).

## Design (fixed)

One module, `src/state/jobLedger.ts`, owns every job the window knows:

```ts
export type LedgerJob = JobSnapshot & {
  epoch: number;                         // project epoch the job belongs to
  request: JobRequestContext | null;     // today's jobRequests entry
  completionHandled: boolean;
  completionOrder: number | null;
  startedAt: number; stageStartedAt: number; stageStartDone: number;
  finishedAt: number | null;             // the tray's timing fields
};
export type LedgerState = {
  jobs: LedgerJob[]; history: HistoryEntry[];   // HistoryEntry moves here
  seq: number; observedOrder: number; retention: JobRetention;
};
/** Pure ADR 0009 rules; the stateful ledger and the gallery both use it. */
export function ledgerReducer(state: LedgerState, action: LedgerAction): LedgerState;
export function createJobLedger(deps: {
  request: RequestFn;                       // the session's raw ipc.request
  now: () => number;                        // injectable clock
  started: (job: LedgerJob) => void;        // today's tracker `started` callback
  finished: (job: LedgerJob, event: FinishedJob, request: JobRequestContext | null) => void;
}): {
  getSnapshot(): LedgerState; subscribe(listener: () => void): () => void;
  begin(request: Request): PendingStart | null;          // as jobTracker
  reply(start: PendingStart, value: JobStarted): void;  // as jobTracker
  settle(start: PendingStart | null): void;             // as jobTracker
  event(event: JobEvent): void;     // started, progress, log, finished (meter/measure_status ignored)
  readSnapshots(): Promise<void>;   // the one `jobs` read, fenced (startup)
  cancel(job: JobId): Promise<void>; // cancel_requested at once, then job_cancel;
                                     // a refusal reverts to running and records request_failed
  clearHistory(): void;
  replaceProject(): void; readonly epoch: number;
  notFound(job: JobId): IpcError;
};
export const TRAY_HIDDEN, activeJobs, trayJobs, remainingMs, elapsedMs, errorCount; // moved from jobStore
```

- `jobRetention.ts` stays as the pure helper the reducer uses.
- `session.ts` creates one ledger and exposes it as `session.jobs` (the
  ledger object). `HostState.jobs` and `HostState.jobRequests` go away; the
  session's event handler forwards job events to `ledger.event`, its start
  calls `ledger.readSnapshots()` instead of its own `jobs` read, and its
  completion effects stay in `session.ts` as the `finished` callback (same
  code as `completeJob` today). The 5 s monitor reconciliation (TD-102) uses
  the ledger.
- A small hook `useJobLedger()` in `state/` returns the snapshot
  (`useSyncExternalStore`). Screen hooks read jobs from it instead of
  `host.jobs`; `useMeasureScreen.cancelJob` and the tray both call
  `session.jobs.cancel(job)`.
- `app/jobs/useJobs.ts` becomes a thin hook over the ledger that keeps the
  `JobsApi` shape `JobTray` takes today (`{ state, cancel, clearHistory, now }`,
  with `state` the tray projection), so `JobTray.tsx` needs no markup change.
  It no longer subscribes to events or reads `jobs` itself.
- `state/jobTracker.ts` and `app/jobs/jobStore.ts` are deleted. `IpcProvider`
  and `useIpc` are deleted if nothing else uses them after the tray change
  (the gallery specimens build their states with `ledgerReducer` or a ledger
  over the mock).

Behaviour to keep, exactly (each is a test today; keep one test per rule at
the ledger's interface): retention of 32 terminal jobs and the retired-id
fence; out-of-order Started/Finished; a reply that arrives before or after its
Started; two same-kind starts whose replies swap; a busy rejection that must
not steal another start's observation; a finish observed during the startup
`jobs` read; a late reply after a project replace (no completion effects for
the old project); tray-hidden kinds (`level_monitor`, `status_watch`) present
in the ledger but not in the tray projection; history capped at 200; refused
cancel reverts; elapsed and remaining time from the stage rate.

## Runs and commits

Run 1 (commit 1): `jobLedger.ts` with `ledgerReducer` and `createJobLedger`,
its test file `state/jobLedger.test.ts` (the rules above, written against the
ledger's interface first: red until the module exists), and `session.ts`
switched to it (`jobTracker.ts` and its tests deleted, `sessionJobs.test.ts`
and `session.test.ts` cases kept where they test session behaviour, moved to
the ledger test where they test job rules). The tray still uses its own store
in this commit. Subject:
`refactor(audit): A-77 세션의 잡을 장부 하나로 셉니다 (#92 C12)`

Run 2 (commit 2): the tray, the screens and the gallery read the ledger;
`jobStore.ts` and its tests deleted (cases already covered by the ledger test
are listed in the report, not rewritten); TD-72: a test that a cancel from the
measure screen shows `cancel_requested` in both the measure view and the tray
(session + mock, a measurement that is running). Subject:
`fix(audit): A-77 작업 트레이와 화면이 같은 잡 장부를 봅니다 (#92 C12, TD-72)`

## Gates (foreground; do not end the turn before they finish)

```sh
cd apps/secs2r/ui && PATH=/opt/node24/bin:$PATH npm run typecheck && PATH=/opt/node24/bin:$PATH npm test && PATH=/opt/node24/bin:$PATH npm run build && PATH=/opt/node24/bin:$PATH npm run build:gallery
```

Report the test counts before and after each run, and list each deleted test
with the ledger test that now covers its rule.

## Report (final answer)

Per run: commit hash, files, the ledger interface as built (if it differs
from the design above, say where and why), tests added and deleted with their
mapping, red failure (run 1: the ledger test before the module; run 2: the
TD-72 test before the screen switch), counts before/after, the last gate
output tail, and anything left undone.
