# P41 (ASTRA): read-only audit of the M5 screens

Repository: `/home/user/Sound-Equalization-and-Correction-System-2R`, commit
`7d38df9` (release 0.15.1). This is a **read-only** audit: you have Read,
Grep and Glob only. Do not try to edit, build or run anything, and do not
spawn sub-agents. Finish your turn only after the report below is written in
full.

## Why

M5 ends when the screen review finds no HIGH (`docs/M5.md` §6). The screens
were drawn by Opus 5.5. Claude has reviewed the gallery
(`docs/M5-review.md`, "Claude 리뷰"); you are the second, independent
reviewer, and you read the code, which Claude's review did not. Findings
Claude already recorded and P40 already fixed are listed in that file; do
not report them again unless the fix is wrong.

## Read first

- `CLAUDE.md` (the screen rules line: 합니다체 sentences, noun-phrase states
  and labels, labels that name the effect, one filled primary action per
  screen, 14 px floor for UI text, `ko` and `en` catalogs together).
- `docs/M5.md` §2 (screen map), §6 (verdict), `docs/M5-review.md`,
  `docs/UI-TERMS.md`, `docs/M5-reach.md` (only to look up an element id).
- The screen rules this project follows, in
  `/root/.claude/skills/synced/1cf9b684-1b33-4e6b-b907-5ff120c804a0_4a388c23-0aa8-44f9-bc2f-de4a3001644a/make-interfaces-feel-better/`:
  `SKILL.md`, then `typography.md`, `surfaces.md`, `color.md`,
  `interruptions.md`, `write.md`, `icons.md`. Skip `qt.md`, `game-write.md`.

## What to read

Everything under `apps/secs2r/ui/src/` except `*.test.*` and
`ipc/bindings/` (generated): `app/`, `screens/*`, `ui/`, `ipc/`, `i18n/`
(`ko.json`, `en.json`, `index.ts`, `react.tsx`), `ui/tokens.css` and the
other `.css` files. About 21 500 lines. Open `ipc/bindings/` only to check a
type a screen relies on.

## What to look for

1. **Wrong or misleading result on screen.** A number, unit, band or state
   shown that does not match the data the service sent (field read from the
   wrong place, unit lost, rounding that hides a failure, a stale result
   shown as current after the project or settings changed).
2. **Flow that cannot finish or loses work.** A button enabled while a job
   runs and starting a second one; a job that cannot be cancelled; a
   response from an earlier project applied after switching projects;
   unsaved edits dropped without a prompt; an error that leaves the screen
   stuck in "running".
3. **Wiring.** IPC called without the generated binding types (casts to
   `any`, hand-written payloads), event listeners not removed on unmount,
   effects with missing dependencies that read stale state, promises whose
   rejection is never shown to the user.
4. **States.** For each step screen (system, connect, measure, calibrate,
   export) and the project, help and tutorial screens: is there a defined
   look for empty, running, done, stale, error, and tutorial? Name any state
   that falls through to a blank card or a raw error string.
5. **Interruptions and keyboard.** Every dialog and overlay has a visible
   way out, Esc, and a sensible first focus; no focus trap that cannot be
   left; icon-only buttons have an accessible name; destructive actions say
   what they destroy.
6. **Screen rules.** UI text below 14 px (look at tokens and any literal
   `font-size`); more than one filled primary action on a screen; labels
   that name the mechanism instead of the effect; status shown as a
   sentence; numbers in tables without tabular figures; colour used for
   decoration rather than state.
7. **Catalog language.** Sentences not in 합니다체, labels that are not noun
   phrases, decorative or emotional wording, English that is not natural,
   `ko` and `en` that say different things, placeholders whose meaning
   differs between the two (the key sets themselves are already tested).

## Severity

- **HIGH:** the user is shown a wrong result, loses work, cannot finish a
  step, or is trapped (keyboard or dialog); or an action does something
  other than its label says.
- **MEDIUM:** a visible defect or rule break that confuses but does not
  mislead or block (missing state, wrong first focus, unclear label).
- **LOW:** polish.

Report only what you can show from the code. Each finding needs the file
and line, the concrete path that triggers it (which state, which click or
response), and what the user sees. If you cannot trace the path, put it
under "Unsure" instead of giving it a severity. Do not report style
preferences the rules above do not cover.

## Report

Write in Korean (합니다체), in this shape, and nothing else:

```
## ASTRA 감사 (읽기 전용)

- 범위: <what you read; what you skipped and why>

### 찾은 것

| Severity | Location | Path | User sees | Fix |
| --- | --- | --- | --- | --- |
| HIGH | `apps/secs2r/ui/src/...:LINE` | ... | ... | ... |

(HIGH first, then MEDIUM, then LOW; at most 30 rows.)

### 확신하지 못한 것

| Location | Suspicion | What would confirm it |
| --- | --- | --- |

### 보고 문제없던 것

<short list of the areas from "What to look for" you checked and found clean>

- 판정(ASTRA): HIGH <n>, MEDIUM <n>, LOW <n>.
```
