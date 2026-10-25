# P85 (ASTRA): audit #92 architecture C15 — the effects of requests in one rule table (A-79)

Worktree: `/home/user/wt-ui` (the branch Claude names in the run brief). Work
only there. Do not run cargo, do not push, do not spawn sub-agents. Claude
reviews, re-runs the gates and re-authors the commits.

Scope rule: state, call code, hooks, mock and tests. No markup, CSS or layout
changes; `App.tsx` and screen hooks change only where they call the session.
Every screen renders the same DOM for the same state as today.

Read first: `CLAUDE.md`, `docs/adr/0003-stack-and-ipc.md` (staleness is the
service's; the wire shape is fixed), `docs/DESIGN.md` §7, and in full:
`src/state/{triggers.ts,session.ts,jobLedger.ts,store.ts}`, `src/App.tsx`
(the tutorial preparation effect and the tutorial restart action),
`src/screens/project/useProjectScreen.ts` (`offerTutorial`),
`src/screens/connect/useConnectScreen.ts`, the other `screens/*/use*Screen.ts`,
and their tests.

## Problem

What a request, a finished job or a job log changes is decided in five
places: the reload table `TRIGGERS` (`state/triggers.ts`), `HOST_TRIGGERS`,
the response handling inside `session.call` (measurement settings and path,
project replacement reset, sweep result clearing on `measure_configure` and
`rew_driver_set`, low-SNR warning pruning on `measure_position`,
`position_clear`, `mdat_import`, `sample_load`), the job completion handler
(`completeJob`: failure state, selection, sweep result, XO recommendation,
runs, export result, XO recalibration) and the log handler (low-SNR
warnings). Some screen hooks also reload what a rule already reloads. Tests
override `session.run` to know when the effects are done. Preparing the
tutorial is written three times (the `project_new` tutorial branch in
`session.call`, the preparation effect in `App.tsx`, the restart action in
`App.tsx`), and they configure the measurement method differently.

## Design (fixed)

1. **One rule table**, `src/state/effects.ts`:

   ```ts
   export type Trigger =
     | { request: Request["method"] }                      // a successful response
     | { finished: JobKind; status: JobStatus }            // a finished job
     | { log: string };                                    // a job log code
   export type EffectContext = { /* session pieces a rule may use: host, stores,
     jobs, call, configure, refreshCurves, the request/response or event */ };
   export type Rule = {
     name: string;                       // stable, used by the tests
     when: (trigger: Trigger) => boolean;
     reload?: readonly ResourceKey[];    // today's TRIGGERS rows, as data
     apply?: (ctx: EffectContext) => void | Promise<void>; // today's inline code, moved as is
   };
   export const RULES: readonly Rule[];
   export function effectsOf(trigger: Trigger): { reload: ResourceKey[]; rules: Rule[] };
   ```

   `TRIGGERS`, `HOST_TRIGGERS`, `PROJECT_REPLACED` and `triggeredBy` move into
   rules (keep `TRIGGERS.project.mutations` reachable for the TD-101 dirty
   guard, or give the rule table a `projectEdits` list that it uses). The
   session's `call`, its event handler and `completeJob` each build a
   `Trigger` and run `effectsOf(trigger)`: reloads first, then the rules'
   `apply` in table order, with the project-epoch checks exactly where they
   are today (an old project's response or job changes nothing). Order
   matters where code today depends on it (for example a project replacement
   resets host state before the reloads); keep today's order and say in the
   table where it matters.

2. **`session.settled(): Promise<void>`** resolves when no `call`, reload or
   rule effect started by the session is still running (a counter around
   them; it resolves at once when idle). Tests that override `session.run` or
   poll to learn when effects finished use `settled()` instead; remove those
   overrides.

3. **One tutorial start**, `session.startTutorial({ quest?: string }): Promise<void>`:
   stop the monitor, `project_new` with the tutorial flag (keep today's
   params), configure the measurement method as today's preparation effect
   decides from the starting quest (`mock` unless the start is a system quest
   or `connect.retry`, then `none`), set the deliberate wrong SW3 output when
   the start is a system quest or none (today's `wrongOutput` step), and
   leave the session ready for the tutorial layer. The `project_new` branch
   in `session.call` no longer configures anything itself; `App.tsx`'s
   preparation effect and its restart action, and `offerTutorial`, call
   `startTutorial`. The tutorial layer still shows only after the
   preparation finished (today's `tutorialReady`).

4. **Hooks** drop their own reloads where a rule already reloads the same
   resource for the same trigger. User-initiated refreshes (a 새로 고침 or
   다시 확인 button) stay.

## Tests

- `src/state/effects.test.ts`, table-driven: for each rule, the trigger it
  answers and the reloads and host changes it makes (one row per rule; use
  the mock service and `settled()`). This is written first (red until
  `effects.ts` exists).
- Keep every existing session, hook and flow test passing; where a test only
  checked that a hook reloads something a rule now reloads, move it to the
  table test and say so.
- Tutorial: starting from each kind of quest (a system quest, `connect.retry`,
  a later quest) and the restart action produce the same project, method and
  SW3 output as today (read today's behaviour from the code and the tour
  scenes in `src/gallery/tour/scenes.ts`).

## Runs and commits

Run 1: items 1 and 2, commit
`refactor(audit): A-79 요청이 일으키는 효과를 규칙 표 한 곳에 모읍니다 (#92 C15)`.
Run 2: items 3 and 4, commit
`refactor(audit): A-79 튜토리얼 시작을 한 곳에서 합니다 (#92 C15)`.
Both are refactors: the existing tests are the contract; report the counts
before and after and list each test you moved or deleted with its
replacement.

## Gates (foreground; do not end the turn before they finish)

```sh
cd apps/secs2r/ui && PATH=/opt/node24/bin:$PATH npm run typecheck && PATH=/opt/node24/bin:$PATH npm test && PATH=/opt/node24/bin:$PATH npm run build && PATH=/opt/node24/bin:$PATH npm run build:gallery
```

## Report (final answer)

Per run: commit hash, files, the rule table as built (rule names with their
triggers), tests added, moved and deleted with their mapping, counts
before/after, the last gate output tail, and anything left undone.
