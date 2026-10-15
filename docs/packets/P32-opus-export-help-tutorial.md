# P32 (Opus 5.5): 내보내기, help, the tutorial engine and the project dialogs

Worktree: `/home/user/wt/p32` (branch `m5/p32`). Follow
[`M5-screens-common.md`](M5-screens-common.md) for the shape of a screen,
the rules, the checks and the report. This packet owns
`apps/secs2r/ui/src/screens/export/`, `src/screens/help/`,
`src/screens/tutorial/`, `src/screens/project/`, the `export.*`, `help.*`,
`tutorial.*`, `project.*` and its `notice.*` keys, and the reach-table
sections 9 (export), 10 (cross-cutting; P29 rows too, by the frame's ids)
and the engine rows of 11 (tutorial). The quests themselves come later
(P35), once every screen exists.

Upstream: `src/secs/wizard/step8_mdat.py`, `window.py`, `help_dialog.py`,
`tutorial_text.py`, `tutorial_overlay.py`, `tutorial_quests.py`,
`src/secs/wizard/__init__.py` (new and open project).

## 내보내기: write what the DSP and REW need

- **결과**: "<모드> · 총 지연 x ms · N채널 전후 · 서브 N개 측정대로 · MIMO
  적용 N", the role codes and the RC summary; or stale with the reason; or
  why there is nothing to export (with the way to 보정).
- **CamillaDSP 설정** (the primary, `export { kind: "camilla_dsp" }`) and
  **REW mdat 전후** (`before_after_mdat`): what each writes; the folder
  line with 바꾸기… (P28 folder dialog, `export_folder_set`) and 폴더 열기
  (`open_folder`); after a run "N개 파일 저장 → 경로", or the failure with
  cause and next step; in the tutorial, "튜토리얼에서는 저장하지 않습니다".
- **고급: 단계별 mdat** (U3, `staged_mdat`): upstream built the card and
  never showed it.
- MIMO is included only when on, computed and with ≥ 3 positions; say so.
- No 완료 button (U4). Buttons are disabled without a result, while
  computing or while an export runs.
- Header: primary "CamillaDSP 설정 내보내기" (`frame.step.export.primary`).
  Step state: blocked with `no_result` or `computing`; done after an export
  of the current result.

IPC and types: `export` (job; `ExportParams`, `ExportKind`,
`ExportResult`), `results`, P28's `export_folder_get`/`set`,
`dialog_pick_folder`, `open_folder`.

## 도움말

A help panel that does not block the screen (upstream: non-modal 640×720
window): title "도움말 · <단계>", the step chip "k / 5", search (Ctrl+F;
Enter next match; Esc closes), tabs: 처음이라면 (system), 따라 하기, 확인할
것, 문제 해결, 실제 REW, plus mdat 가져오기 on 측정·가져오기 and CamillaDSP on
내보내기; it follows the current step. Rewrite upstream `tutorial_text.py`
for the new five steps and screens (Korean first; no screenshots), into
`help.<step>.<tab>.*` keys. Keep upstream's facts (limits, REW settings,
troubleshooting causes); drop what the new screens made unnecessary. The SNR
threshold is 20 dB everywhere (U6).

## 튜토리얼 engine

The machinery upstream's overlay provides, ready for P35's quests:

- A quest type: id, step, kind (`look` / `act`), target ids (`data-ui`),
  text, why, hint, optional glossary terms; completion decided by a
  predicate P33/P35 supply (`done(view)`), an `auto` action with its note,
  `blocked` and `busy` texts.
- The overlay: dims all but the targets; look targets dashed and not
  clickable (wheel passes); act targets in the tutorial accent with a
  pulsing ring (still under `prefers-reduced-motion`) and clickable;
  `apart` outlines each target; scrolls targets into view; clicking
  elsewhere shakes the coach card with a toast.
- The coach card (about 340 px, draggable, placed away from the target, the
  rail and the header): "<단계> · k / n", pause, text, glossary terms, 왜?,
  hint, 이전 and 계속. Act quests: the first 계속 shakes the hint; the
  second runs `auto` with its note; blocked shows the reason; busy its text.
- The quest panel (top right, folds into a pill), pause (hides everything,
  docks a resume button), a toast at each step's end, restart or finish at
  the end.
- The glossary: upstream's 25 terms (`tutorial_quests.py:462-511`) as
  `tutorial.glossary.*`.
- Show it working in the gallery on the frame's ids (`frame.*`) with three
  or four sample quests; the real quest list is P35.

## Project dialogs

What the frame's project menu and closing need (the frame has the menu;
these are the dialogs behind it):

- **새 프로젝트** (upstream had no button): preset and sub count, start
  from the sample room or empty, then the save location (P28 save dialog,
  `project_new`).
- **저장 안 한 변경**: before closing, opening another project and starting
  the tutorial: 저장 / 저장 안 함 / 취소.
- **튜토리얼 시작**: what the tutorial does (a separate demo project, mock
  measurement, saving and exporting blocked) and its start.

IPC and types: `project_new`, `project_open`, `project_save`,
`project_info`, P28's project dialogs, `app_settings_get`/`set` (recent
projects).

## States to draw

Export: no result, stale, ready, exporting, done with files, failed, the
advanced staged-mdat open, tutorial; help on each step and with a search
hit; the tutorial engine on a look quest, an act quest, paused, the quest
panel open and folded, the end; the three project dialogs; 1024×700 for
export and help.
