# P96 (ASTRA): wire 마이크 보정 파일 into 연결·점검, and its catalogue

Worktree: `E:\Sound-Equalization-and-Correction-System-2R\.claude\worktrees\dirac-liveproject-support-2e310d`
(branch `claude/mic-calibration`). Work only there. Do not commit, do not
push, do not spawn sub-agents. Claude reviews the diff and commits. Nobody
else edits this tree while you work.

Read first: `docs/adr/0015-mic-calibration-in-calculation.md`, `CLAUDE.md`
(screen rules: 합니다체 sentences, noun-phrase states and labels, ko and en
together), `C:\Users\32170336\.claude\CLAUDE.md` (the Korean writing guide;
it applies to every Korean string), P95
(`docs/packets/P95-daybreak-mic-calibration-service.md`, merged: the IPC
methods and types), the bindings `MicCalibrationStatus.ts`,
`MicCalibrationFile.ts`, `MicCalibrationSetParams.ts`, `OpenFileKind.ts`
under `apps/secs2r/ui/src/ipc/bindings/`.

Claude has already drawn the screen. Do not change markup or styles:
`apps/secs2r/ui/src/screens/connect/{ConnectScreen.tsx,MicCalibrationField.tsx,connect.css,types.ts,fixtures.ts,specimens.tsx,ConnectScreen.test.tsx}`
are Claude's. `types.ts` already declares what you fill:

```ts
export type MicCalibrationView = {
  status: MicCalibrationStatus | null;          // mic_calibration_get; null before the first answer
  busy: "pick" | "clear" | "attach" | null;
  error: IpcError | null;                        // last failure of those three until the next attempt
};
// ConnectView.micCalibration: MicCalibrationView
// ConnectActions: pickMicCalibration, clearMicCalibration, attachMicCalibration
```

## Wiring

1. `state/session.ts`: a resource `micCalibration: read({ method: "mic_calibration_get" })`.
   Host state for the field: `micCalBusy: MicCalibrationView["busy"]` (null)
   and `micCalError: IpcError | null` (null), cleared when the project is
   replaced.
2. `state/effects.ts` reloads: `micCalibration` on `mic_calibration_set`,
   `mic_calibration_clear`, `mic_calibration_attach`, every `measurements`
   mutation, and the `recorded` jobs. `project` also on
   `mic_calibration_set` and `mic_calibration_clear` (they make the project
   dirty). `mic_calibration_attach` changes what the calculation sees, so it
   also reloads `results`, `resultCurves`, `lines` and `curves`. Follow the
   existing tables; update whatever test enumerates methods.
3. `screens/connect/useConnectScreen.ts`: load `micCalibration` when the
   screen is active on the native path; build `view.micCalibration` from the
   resource and the host state. Actions:
   - `pickMicCalibration`: busy "pick", error null; `dialog_open_file
     { kind: "mic_calibration" }`; null (cancelled) ends with nothing
     changed; else `mic_calibration_set { path }`.
   - `clearMicCalibration`: busy "clear"; `mic_calibration_clear`.
   - `attachMicCalibration`: busy "attach"; `mic_calibration_attach`.
   An `IpcFailure` goes to `micCalError` and is not rethrown (the field shows
   it in place; no second toast). Any other error is rethrown as now. Busy
   returns to null in `finally`. A later success clears the error.
4. `ipc/mock.ts`: the four methods and the new dialog kind, keeping a small
   state (no file at first; `set` gives the UMIK-1 fixture file of
   `screens/connect/fixtures.ts` `umikFile` with the chosen path's name;
   `clear` removes it; `attach` moves `attachable` into `attached`; the mock
   sample measurements count as attachable sweeps). `dialog_open_file` with
   `mic_calibration` returns
   `C:/Users/demo/Documents/UMIK-1/7000001_90deg.txt`. Keep `mockWiring`
   and `mock` tests passing and add cases for these.

## Catalogue (`i18n/ko.json`, `i18n/en.json`)

Keys the screen uses (Claude's drafts in Korean; keep the meaning, fix the
Korean if it reads badly against the writing guide, write natural English):

| key | ko draft |
|---|---|
| `connect.mic_cal.pick` | 파일 고르기 |
| `connect.mic_cal.replace` | 파일 바꾸기 |
| `connect.mic_cal.clear` | 선택 해제 |
| `connect.mic_cal.help` | 마이크와 함께 받은 보정 파일(.txt, .cal)을 고르세요. 고른 뒤에 재는 측정마다 이 파일이 붙고, 보정 계산과 측정 그래프에는 마이크의 응답을 뺀 값이 쓰입니다. UMIK-1은 마이크를 천장 쪽으로 세우면 90도 파일, 스피커 쪽으로 향하게 두면 0도 파일을 고르세요. 선택을 해제해도 이미 잰 측정에 붙은 파일은 그대로입니다. |
| `connect.mic_cal.rew_help` | REW에 불러온 보정 파일입니다. REW로 잰 측정마다 이 파일이 붙고, 보정 계산과 측정 그래프에는 마이크의 응답을 뺀 값이 쓰입니다. 파일은 REW의 Preferences > Cal Files에서 바꾸세요. |
| `connect.mic_cal.summary` | 시리얼 {serial} · {low}–{high} Hz · 주파수 {points}개 |
| `connect.mic_cal.summary.no_serial` | {low}–{high} Hz · 주파수 {points}개 |
| `connect.mic_cal.attached` | 측정 {count}개에 붙어 있음 |
| `connect.mic_cal.attachable` | 마이크 보정 없이 잰 측정이 {count}개 있습니다. 같은 마이크로 쟀다면 이 파일을 붙이세요. 보정을 다시 계산하면 결과에 반영됩니다. |
| `connect.mic_cal.attach` | 측정 {count}개에 붙이기 |
| `connect.mic_cal.error.unreadable` | {file} 파일을 열지 못했습니다. |
| `connect.mic_cal.error.unreadable.next` | 파일을 옮기거나 지우지 않았는지 확인한 뒤 다시 고르세요. |
| `connect.mic_cal.error.too_large` | {file} 파일이 4 MB를 넘습니다. 보정 파일은 보통 수십 KB입니다. |
| `connect.mic_cal.error.not_text` | {file} 파일은 글자로 된 파일이 아닙니다. |
| `connect.mic_cal.error.header` | {file} 파일의 {line}번째 줄에서 감도 값(Sens Factor, Sensitivity)을 읽지 못했습니다. |
| `connect.mic_cal.error.row` | {file} 파일의 {line}번째 줄이 주파수와 dB 숫자가 아닙니다. |
| `connect.mic_cal.error.order` | {file} 파일의 {line}번째 줄 주파수가 앞 줄보다 높지 않습니다. |
| `connect.mic_cal.error.range` | {file} 파일의 {line}번째 줄 값이 ±200 dB를 넘습니다. |
| `connect.mic_cal.error.too_few_points` | {file} 파일에 주파수와 dB가 적힌 줄이 둘보다 적습니다. |
| `connect.mic_cal.error.both_conventions` | {file} 파일에 USB 마이크 감도(Sens Factor)와 아날로그 감도(Sensitivity)가 함께 적혀 있습니다. |
| `connect.mic_cal.error.file.next` | 마이크 제조사가 준 보정 파일을 고르세요. |
| `connect.mic_cal.error.no_file` | 붙일 보정 파일이 없습니다. |
| `connect.mic_cal.error.no_file.next` | 먼저 파일을 고르세요. |

`connect.mic_cal` and `connect.mic_cal.none` exist; keep them. Placeholders
must match between ko and en. Run the font-subset gate the i18n tests have;
if a new Korean character is missing from the shipped subset, follow the
repository's procedure for the subset (do not reword around it silently;
report what you did).

## Verify (foreground; from `apps/secs2r/ui`, Node 24; do not end your turn before they finish)

```sh
npm ci
npm run typecheck
npm test
npm run build
```

`npm test` includes `ConnectScreen.test.tsx`; if it fails only because of
something in Claude's files, report it instead of editing them.

## Report

Changed files, the command output tails, the final ko and en strings you
wrote (a table), and anything unfinished.
