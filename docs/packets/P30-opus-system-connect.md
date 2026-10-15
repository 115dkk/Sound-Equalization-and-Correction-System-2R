# P30 (Opus 5.5): the 시스템 and 연결·점검 screens

Worktree: `/home/user/wt/p30` (branch `m5/p30`). Follow
[`M5-screens-common.md`](M5-screens-common.md) for the shape of a screen,
the rules, the checks and the report. This packet owns
`apps/secs2r/ui/src/screens/system/` and `src/screens/connect/`, the
`system.*`, `connect.*` and its `notice.*` keys, and the reach-table sections
1–4 (Step 1 devices and mic, Step 2 setup, Step 3 level check, Step 4
positions).

Upstream: `src/secs/wizard/step1_devices.py`, `step2_config.py`,
`step3_level.py`, `step4_layout.py`, `layout_map.py`, `mic_meter.py`, and
`src/secs/model.py` (`SpeakerConfig`, `DspProfile`, `validate`).

## 시스템: what is in the room and how it will be measured

Upstream steps 2 (setup) and 4 (positions) in one screen: speakers, outputs,
the export DSP and the measurement arrangement are decided together, before
anything is connected.

- **구성** (the widest card, first): preset choice (2.0, 2.1, 2.2, 5.1,
  7.1.4, 9.1.6, and 사용자 지정 when the config is custom), the sub count
  stepper (0–8); the output patch table grouped by `ConfigResult.groups`
  (front, center, sub, surround, back, wide, top, height, other) with
  channel (role, "code · Dolby name"), size (small = XO applied / large =
  full range; subs show a pill), measurement output (with out-of-range and
  duplicate states), polarity (normal / inverted) and remove; the summary
  line; actions to add a speaker, renumber outputs and save the patch-sheet
  CSV (a save dialog, then `patch_sheet_csv`); every `config_validate`
  notice with cause and next step (first four, the rest behind "외 N개").
  The measurement output device is chosen on 연결·점검; here it shows as a
  pill with its channel count and the "N개 초과" state.
- **방 배치도** (beside 구성): `RoomMap` with the selected speaker synced to
  the table row, and the selected speaker's detail (name, role, kind, one
  line per band "BAND → 출력 n → DSP 입력 n → 앰프 [반전]").
- **측정 배치**: the four arrangements (1 한 자리, 9 의자 하나, 13 2인
  소파, 17 가족 두 줄) as choices with `ArrangementThumb` and "N개 위치";
  the `ArrangementMap` of the chosen one; options: start delay
  (끔/5/10/15초), timing reference (음향: 기준 스피커 / 루프백: 케이블 /
  없음) with the reference speaker or output; the estimate per position and
  in total from `position_estimate`.
- **내보낼 DSP** (U2): the device preset (CamillaDSP·BruteFIR, Tesira
  SERVER-IO, Q-SYS Core, miniDSP, 사용자 지정) visible; the five fields
  (FIR taps per channel with "선형 위상 최대 x ms", total FIR budget, IIR
  bands, all-pass bands, BM path delays) and the note "창 ≈ x ms · 해상도
  ≈ y Hz" (`DspResult.window_ms`, `resolution_hz`) under 고급. Say where the
  values are used: the FIR budget feeds the correction's FIR-budget warning
  and the MIMO FIR support budget; the rest is written into the export.
  Editing a field makes the profile 사용자 지정.
- Preset, sub count and arrangement changes clear measurements: when the
  view says the project has measurements, ask first (`useConfirmAndUndo`)
  and send `confirm: true` (U5).
- Header: the primary action goes to 연결·점검 (`frame.step.system.primary`).
  Step state: blocked with `no_speakers` or `config_invalid` (count), else
  done.

IPC and types: `config_get`, `config_apply_preset`, `config_set_sub_count`,
`config_set_speaker`, `config_add_speaker`, `config_remove_speaker`,
`config_renumber`, `config_validate`, `patch_sheet_csv`, `dsp_get`,
`dsp_set`, `layout_get`, `layout_set`, `position_estimate`, `undo`, and the
save-file dialog from P28 (`ConfigResult`, `ConfigMutationResult`,
`DspResult`, `LayoutResult`, `LayoutMutationResult`,
`PositionEstimateResult`, `Notice`).

## 연결·점검: choose how to measure, then check every channel's level

Upstream steps 1 (devices and mic) and 3 (level check). People who only
import REW `.mdat` files skip this step (the rail marks it skipped).

- **측정 방식** first: 자체 측정 (native audio), REW, 모의 측정 (the sample
  room), and "mdat 파일만 가져오기" which skips the step. Each choice says
  what it needs and whether it is available now (`MeasureMethodsResult`:
  methods, native devices and the enumeration error, REW status).
- **REW** (when chosen): status pill (연결 안 됨 / API 연결됨 ·
  127.0.0.1:4735, checking), executable path, version, Pro state (ok / 연결
  전 / Pro 아님: manual measurement), blocking mode; launch REW in API mode
  / check again (`rew_launch`); read REW's settings into a hint line; a
  hint for each REW state (off, running without API, executable not found,
  cannot launch, timed out, launch failed); mic calibration from REW
  (unknown / none loaded / file name).
- **장치와 마이크** (native, and the REW device fields): input device
  ("name · N채널"), input channel, output device (this is also the
  measurement output device 시스템 shows), sample rate (read-only), sweep
  length 64k…4M shown as "길이 · 초" (default 256k), the loopback input
  channel when the timing reference is a cable (`measure_configure`,
  `MeasureConfigResult`).
- **레벨 점검**: one row per channel in measurement order: play/stop
  (`level_play`; subs play 50 Hz; one at a time; stops after 2.5 s), role,
  name, `LevelMeter`, value; stored values pre-fill. Master output −40…0
  dBFS step 0.5 (default −20) and a "재생 중 · code" pill; play all channels
  in turn; the mic input (channel · device) with its live meter (dBFS,
  peak, state, clear peak) fed by `Meter` events while
  `level_monitor_start` runs; skip with default levels (upstream fills
  −22 − (len(role)·7 mod 6) dBFS; the service applies them, the screen only
  asks). Also export the mic meter as a small panel component the frame can
  float (upstream Ctrl+M); P33 wires the shortcut.
- Header: the primary action plays all channels in turn
  (`frame.step.connect.primary`). Step state: done when a path is chosen
  and ready and every channel has a level; skipped for mdat-only; blocked
  (`no_path`, or a new `no_levels`) otherwise.

IPC and types: `measure_methods`, `measure_configure`, `rew_launch`,
`level_check` (job), `level_apply`, and P28's `level_monitor_start`,
`level_monitor_stop`, `level_play`, the `Meter` and `MeasureStatus` events,
`status_watch_start`/`stop` (`MeasureMethodsResult`, `MeasureDevice`,
`MeasureConfigResult`, `RewStatusResult`, `LevelResult`).

`RewStatusResult` today has only running / reachable / pro. Executable
path, version, blocking mode, mic calibration and REW's own settings are
upstream items the service does not report yet: put them in the view as
optional fields, draw them, and list each as a service gap in the report
(Claude adds the methods).

## States to draw

시스템: new project (2.0, nothing measured), the sample 7.1.4 with two subs,
a config with validation problems (duplicate output, out of range, a
speaker without a band), the confirm dialog for a preset change with
measurements, the expert view, 1024×700.

연결·점검: nothing chosen, REW not running, REW running without API, REW
connected but not Pro, native with devices, native enumeration error, level
check half done with one channel playing, all levels done, mock, skipped
(mdat only), tutorial accent, 1024×700.
