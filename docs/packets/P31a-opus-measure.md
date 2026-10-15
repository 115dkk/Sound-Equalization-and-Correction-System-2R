# P31a (Opus 5.5): the 측정·가져오기 screen

Worktree: `/home/user/wt/p31a` (branch `m5/p31a`). Follow
[`M5-screens-common.md`](M5-screens-common.md) for the shape of a screen,
the rules, the checks and the report. This packet owns
`apps/secs2r/ui/src/screens/measure/`, the `measure.*` and its `notice.*`
keys, and the reach-table section 5 (Step 5 measure) and the measurement
plot rows of section 8 (graphs).

Upstream: `src/secs/wizard/step5_measure.py` (with `MdatAssignDialog`,
`MdatFilesDialog`, `load_mdat`), `legend.py`, `axis_range.py`, `plot.py`,
`layout_map.py`, and the measurement session in `src/secs/measure*.py`.

## 측정·가져오기: sweep every channel at each position, or import REW files

The step where the room gets measured. One position at a time: the map
says where the mic goes, the primary action measures it, the curves show
what was measured. Importing `.mdat` files is the other way in and sits on
the same screen.

- **Where the mic goes**: "위치 k / N" or "모두 측정함", "이 위치 약 …"
  (from the view's estimate); the `ArrangementMap` with done / current
  positions, selectable by click and arrow keys (ignored while a job runs).
- **What was measured**: the measurement plot (`Graph`/`LogPlot`, 5–22000
  Hz, −35…+20 dB, the 280-point 1/12-octave curves of
  `measurement_curves`, normalised by the service); legend with channel
  chips (subs bold) and per-position chips (toggle, hover highlight;
  hidden for ≤ 1 position), "굵은 선: 위치 1"; the axis-range popover; a
  control to hide or show the graph (the map grows when hidden).
- **Positions**: the list "NN 위치 설명" with done / waiting, the source
  file in a tooltip, click selects; the source note ("불러옴: a.mdat (위치
  1–9) · 측정함 (위치 10)"); progress across roles for the running
  position.
- **Actions**: measure this position (the primary; "다시 측정" when done;
  disabled when all done or while any measuring-group job runs); cancel
  measurement (stops the countdown, or after the current sweep; the job
  tray has it too, U8); redo the last position (clears it: confirm and undo,
  U5); options: start delay, sweep length, repeats 1/2/4/8 (forced to 1
  with an acoustic timing reference, say why).
- **Import**: load `.mdat` (the P28 open dialog, then `mdat_scan`, then the
  assignment dialog, then `mdat_import` as a job): the assignment dialog
  lists each measurement title with its detail (unreadable / low only, looks
  like a sub / full range), a role (skip or "code · name") and a position
  when the file holds several, pre-filled from the scan
  (`MdatScanMeasurement`), problems disable 가져오기. **Manage files**:
  position | file (or 측정함 / —) | channels have/total | import a whole
  file into that position | clear (`position_clear`, confirm and undo).
  **Demo data** (tutorial accent only) loads the sample measurements
  (`sample_load`).
- **What happened**: no log panel; progress per role and position, the
  countdown, low S/N warnings (20 dB, U6: the help text says 20 too),
  errors with cause and next step, the new project path, mdat lines: all
  arrive as job events in the tray. The screen shows the running job's
  stage beside the primary action and the last warning for this position.
- New projects: upstream creates `~/Documents/SECS/측정 YYYYMMDD-HHMM.secs`
  silently before the first sweep; here the view says whether the project
  has a path and the primary action asks for one first (save dialog) when
  it does not.
- Header: primary "이 위치 측정" (`frame.step.measure.primary`). Step state:
  blocked with `no_path` (no measurement method, unless importing) or
  `measuring`; done when every position is measured; ready otherwise.

IPC and types: `measure_position` (job), `job_cancel`, `position_clear`,
`undo`, `mdat_scan`, `mdat_import` (job), `sample_load` (job),
`measurement_curves`, `project_info`, `layout_get` (arrangement geometry),
`position_estimate`, P28's open-file dialog and the `MeasureStatus` event
(`ProjectInfo`, `MeasurementCurvesResult`, `MeasurementCurve`,
`MdatScanResult`/`MdatScanMeasurement`, `MdatAssignment`, `LayoutResult`,
`PositionEstimateResult`).

## States to draw

Nothing measured (sample 7.1.4, 9 positions), three of nine measured with
curves, measuring position 4 (busy primary, role progress), all done, the
graph hidden, the mdat assignment dialog with a problem row, the manage
files dialog, a low-S/N warning on the current position, imported from mdat
(skipped 연결·점검), tutorial accent with the demo-data action, 1024×700.
