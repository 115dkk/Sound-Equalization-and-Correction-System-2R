# P129 (Opus 5.5): the level check says why it stopped (#120)

Worktree: `E:\Sound-Equalization-and-Correction-System-2R\.claude\worktrees\macos-audio`
(branch `claude/macos-audio`, from main at v0.35.0). Do not commit, do not
push. Two other workers edit `crates/secs-sys/src/cpal.rs`, `CHANGELOG.md`,
`.github/**` and `apps/secs2r/src/self_test.*` in the same worktree; work
only under `apps/secs2r/ui/`.

Read first: `CLAUDE.md` (screen rules), `docs/DESIGN.md`, load the
`make-interfaces-feel-better` skill, then `apps/secs2r/ui/src/state/session.ts`
(the level monitor: `monitor`, `startMonitor`, `completeJob`, the `host`
fields `monitor`, `meter`, `playing`, `playingAll`), the level check panel
under `apps/secs2r/ui/src/screens/connect/` (find where "재생 중" and
`connect.mic.offline` are drawn), how other screens show a failed job's
typed error (look for an existing helper that turns an `IpcError` into a
sentence and a next step, for example in the measure flow), and the
bindings `JobEvent.ts`, `IpcError.ts`, `ErrorCode.ts`.

## What happened

Issue #120 (screenshot in https://m.dcinside.com/board/speakers/582638): on
a Mac the level check showed "재생 중 · L" while nothing played, and the
microphone panel read "응답 없음" with the REW hint. The native level
monitor job had failed (the output stream could not open; P127 fixes that
cause), and `completeJob` only set `monitor: "offline"`: `playing` stayed
set, so the row kept claiming it was playing, and the job's error never
reached the panel.

## Rules (fixed)

1. When the level monitor job finishes as failed, clear `playing` and
   `playingAll`, keep `monitor: "offline"`, and keep the job's `IpcError`
   in the session state (a new field, for example `monitorError`), cleared
   when the monitor starts again or is switched off.
2. The level check panel shows that failure in place of the generic
   offline hint: one sentence of what happened and one of what to do, from
   the error's code through the existing error-text helper if one fits, and
   the backend's own detail in a collapsed detail (as other failures do).
   For the native method the REW hint ("REW라면 API 모드로…") does not show.
3. A play button pressed while the monitor is offline restarts the monitor
   first (if that is not already the behaviour, say what it is and keep
   it if it is reasonable).
4. New strings: keys in both catalogs with first ko/en versions (합니다체,
   noun-phrase states, no em-dash); list them.
5. Specimens: the level check with a failed native monitor (an
   `unsupported_format`-like error with a detail), in the gallery. Tests:
   the failure clears the playing row, shows the reason, hides the REW
   hint for native, and a restart clears the error.

## Files

You may edit: `apps/secs2r/ui/src/state/**`, `apps/secs2r/ui/src/screens/connect/**`,
`apps/secs2r/ui/src/ipc/mock.ts` (a way for specimens/tests to fail the
monitor), `apps/secs2r/ui/src/i18n/ko.json`, `en.json`,
`apps/secs2r/ui/src/gallery/**` (registration).

## Verification (foreground; do not end the turn before they finish)

```sh
cd apps/secs2r/ui
npm run typecheck
npm test
npm run build
```

Node 24; run `npm ci` first only if `node_modules` is missing or stale.

## Report

Files changed, the state field and rules as built, keys with ko/en,
specimens, tests, and the commands' last lines.
