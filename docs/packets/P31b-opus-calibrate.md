# P31b (Opus 5.5): the 보정 screen

Worktree: `/home/user/wt/p31b` (branch `m5/p31b`). Follow
[`M5-screens-common.md`](M5-screens-common.md) for the shape of a screen,
the rules, the checks and the report. This packet owns
`apps/secs2r/ui/src/screens/calibrate/`, the `calibrate.*` and its
`notice.*` keys, and the reach-table sections 6 (room correction), 7
(correction settings) and the result-plot rows of section 8 (graphs).

Upstream: `src/secs/wizard/step_calibration.py`, `target_editor.py`,
`axis_range.py`, `legend.py`, and `src/secs/calibration.py`
(`CalibrationSettings`, `mode_spec`, `take_sub_values`, the sub warnings).

## 보정: compute the correction and judge it

The step people come back to. Results first (`ARCHITECTURE.md` §7): what
the correction achieved, then per channel, then the graphs; the settings
that change it sit beside or below, grouped by what they affect.

- **판정** (first): one card that says whether the correction is good and
  current: mode, "±1 dB (20–200 Hz) a % → b %", "GD 산포 (20–100 Hz) a → b
  ms", filters "PEQ n · FIR n탭 | FIR 없음 · XO n Hz · MIMO 서포트 n",
  total delay, MIMO state; stale (settings or measurements changed since
  the run) with the reason; before the first run, what the run will do and
  the position-1 preview.
- **채널별 성적표**: one row per channel from the result (before → after
  per metric), the worst ones easy to find, a row selects the channel the
  graph shows.
- **그래프**: tabs SPL / GD / XO / BA (BA only with ≥ 2 subs); the channel
  choice (role codes; disabled on BA; on XO it picks the XO group); curves,
  axes and ranges exactly as `result_curves` gives them (the service shifts
  levels and smooths; the screen draws); legends per tab (before, after,
  target, MIMO applied, main, sub, sum, before / after alignment, sub sum);
  the RC lower and upper lines with labels and shading outside the band on
  SPL and GD; target editing on SPL (drag, double-click adds, right-click a
  point or Delete removes, right-click empty space opens the target menu;
  10–24000 Hz, ±30 dB, at least two points, any edit makes the preset
  사용자 지정) with `TargetHandles`; the draggable XO line (40–200 Hz) on
  XO with `XoLine`; the BA lines from `result_lines` (sub-sum flatness,
  seat spread or "한 위치", the table sub | mode | gain | delay | polarity |
  all-pass coloured like the curves, fixed filters and layouts, red
  warnings).
- **Target menu**: presets flat / Harman / cinema, the 사용자 지정
  indicator, save and load target JSON (P28 dialogs, `target_save`,
  `target_load`).
- **Settings**, as sections in this order, each saying what it changes:
  - **모드**: 최소 지연 (`zero`, IIR/PEQ, 0 ms), 낮은 지연 (`mixed`,
    default, FIR 2048, ≤ 10 ms), 표준 (`linear`, FIR 2048, ≤ 30 ms); 초고지연
    (`ultra`, FIR up to 16384, 30–200 ms) only with 전문가 표시, with its
    latency limit 30–200 ms step 10 (U9: no hidden gesture). The spec line
    comes from `mode_spec`.
  - **보정 범위** (affects SPL, GD): lower 10–200 Hz step 5 (default 20);
    upper auto (Schroeder, max 500 Hz) or 100–20000 Hz step 50; room volume
    0–100000 m³ or W×D×H 0–100 m step 0.1 (all three set the volume); boost
    limit 0–10 dB (default 10).
  - **크로스오버** (only with subs; "서브가 없어 XO가 없습니다" otherwise):
    group, XO 40–200 Hz step 5 (default 80), XO PEQ on/off with lower and
    upper 0–1000 Hz (0 = auto: XO ½ and XO × 4); "추천 N Hz" and the XO
    recommendation job (`recommend_xo`) with its own state.
  - **서브 정렬 (BA)** (≥ 2 subs): infra sub (none or a sub) with its XO
    10–300 Hz (25 when first set), type LR24/LR12/LR48/BW12/BW24 and max
    boost 0–24 dB; per sub: mode 자동 / 고정 / 묶음 A / 묶음 B, for groups the
    layout (relative-fixed / DBA / multi-sub), DBA row, rear delay trim
    −20…20 ms; values for fixed and relative-fixed (gain −60…+20 dB step
    0.5, delay 0–500 ms step 0.1, polarity, all-pass 1–2 with f 0–500 Hz and
    Q 0.1–20); fixed filters for every mode (HP and LP 0–300 Hz with type,
    picking a type sets HP 20 or LP 80; PEQ 1–3 with f, Q, gain −30…+12 dB);
    main extra delay 0–100 ms (only when a sub is fixed); use as MIMO
    support (default on); 서브만 계산 (`calibrate { subs_only: true }`) and
    결과 값 가져오기 (`sub_take_values`); the per-sub summary and red
    warnings. Most of this is 고급; the per-sub mode and the two actions are
    visible.
  - **MIMO**: use MIMO (off by default; disabled below 3 positions with the
    reason); support upper limit 50–150 Hz (default 150), lower auto or
    15–150 Hz (default 20), max per input 0–12 (0 = no limit), FIR supports
    (off: IIR, PEQ 9 + AP 3; on: 8000-tap FIR); compute MIMO (`mimo`, needs
    a correction); its state ("FIR|IIR 서포트 N · 입력당 ≤ cap · FIR 예산
    a / b").
- Every setting goes through `calibration_settings_set` and marks the
  result stale; nothing recomputes on its own. Long work is a job (U8):
  the screen shows which of calibrate / XO recommendation / MIMO runs and
  disables what would start a second one of the group.
- Header: primary "보정 계산" (`frame.step.calibrate.primary`; the first
  run without an XO recommendation recommends first, that is the
  service's business). Step state: blocked with `no_measurements` or
  `measuring`; done when a current result exists.

IPC and types: `calibrate` (job, `subs_only`), `recommend_xo` (job),
`mimo` (job), `results`, `result_curves`, `result_lines`,
`sub_take_values`, `calibration_settings_get`/`set`, `mode_spec`,
`target_get`, `target_set_points`, `target_apply_preset`, `target_save`,
`target_load`, `job_cancel` (`ResultsSummary`, `CalibrationSummary`,
`ChannelScore`, `SignalChain`, `MetricsSummary`, `ResultCurvesResult`,
`ResultTab`, `ResultLinesResult`, `SubResultLines`, `CalibrationSettings`,
`ModeSpecResult`, `TargetResult`, `XoRecommendation`, `MimoSummary`).

## States to draw

No result yet (preview and hints), computing (busy primary), a current
result on each tab (SPL with target handles, GD, XO with the line, BA with
the table and a warning), stale after a settings change, a failed run
(cause and next step), subs-only result on BA, MIMO computed, no subs (XO
section says why), the ultra mode in the expert view, tutorial accent,
1024×700.
