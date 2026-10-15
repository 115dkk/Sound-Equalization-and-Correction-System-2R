# SECS 2 wizard: what the user can see and do

This is the inventory the M5 screens are checked against ([`M5.md`](M5.md)). It lists every control, display, condition and message of the upstream wizard (`src/secs/wizard/`, SECS 2 Beta 1.2.0 at the commit in `UPSTREAM.toml`), with file:line references. It is an inventory, not a design: the new screens follow `docs/adr/0003` and `docs/ARCHITECTURE.md` §7, and reach every item here, not copy its layout. Model and engine files are `src/secs/model.py`, `src/secs/calibration.py` and `src/secs/correction_engine.py`.

## 0. Flow and screen numbering (`strings.py:14-28`, `window.py:117-121`)

- The progress bar has 7 positions: Devices·Mic, Setup, Level check, Positions, Measure, Room correction, Export.
- Screen numbers follow `FLOW = (1,2,3,4,5,9,8)`. Room correction is screen 9 (`CAL_STEP`); Export is screen 8.
- Every step has a main column and a fixed-width right column (`style.two_columns`), except Export.
- Every step exposes `refresh()`, `can_advance() -> (ok, reason)` and a `changed` signal.

## 1. Steps

### Step 1: Devices and mic (`step1_devices.py`)

Purpose: confirm that REW runs in API mode with a Pro licence, then choose the mic input, the output driver and the sweep length.

REW connection card (`:179-229`)
- Status pill: not connected, or "API 연결됨 · 127.0.0.1:4735" (`:183,:379-381`); a spinner while REW is checked, launched or read (`:191-198,:310-317`).
- Executable path (`find_rew_exe()`, or unknown) (`:185,:409`); version (`:186,:382`); Pro pill: ok / n/a before connecting / not Pro (manual measurement) (`:187,:383-386`); blocking-mode label (`:188,:387`).
- [Launch REW in API mode] / [Check again] (`:214-216,:286-358`): `RewCache.request_refresh`, then `RewCache.launch(wait_s=40)` in a thread if REW does not answer.
- [Read settings] (`:217-219,:485-521`): reads REW's rate, sweep and level into a hint line.
- Hint box for each REW state: off, running without API, exe not found, cannot launch, timed out, launch failed (`:226,:319-398`).

Mic and audio card (`:231-271`)
- Input device (`extra['devices']['input']`, items "name · Nch") (`:235,:441-456`); input channel (`input_channel`, REW channel names) (`:237,:439`).
- Mic calibration pill from REW `GET /audio/input-cal`: unknown / none loaded (warn) / file name (`:240,:410-423`).
- Output driver (`extra['devices']['output']` and `project.config.device`) (`:241,:465-483`); sample rate, read-only (`:243,:160-168`).
- Sweep length 64k…4M shown as "len · s", default 256k (`extra['devices']['sweep_length']`) (`:244-247,:64-78`).
- While REW is up but the device lists are empty, re-read every 2 s up to 15 times (`:145-148,:432-437`).

Advance: blocked unless REW runs ("REW 를 켜 주세요.") and is Pro (`:524-530`). Skip button [장치 확인 건너뛰기] (`window.py:835-837`).

### Step 2: Setup (`step2_config.py`)

Purpose: speakers and roles, output patch, polarity and size, DSP filter resources.

Top row
- Presets 2.0 / 2.1 / 2.2 / 5.1 / 7.1.4 / 9.1.6 and empty/custom (`:137-151,:952-962`): `SpeakerConfig.from_preset`; clears measurements; highlighted when `config.preset` matches (`:418-421`).
- Sub stepper 0–8: `config.set_sub_count`; clears measurements (`:153-175,:964-967`).
- Bi-amp start checkbox, hidden (`BIAMP_UI=False`) (`:177-182,:969-977`).

Output patch sheet (`:204-257`)
- Measurement output device: `config.device`, pill for channel count (unknown / over by N / N channels) (`:317-331,:473-511,:1076-1080`).
- Table grouped as Front, Center, Sub, Surround, Back, Wide, Top, Height, Other (`:32,:540-601`). Columns:
  - Channel: role (`ROLES` "code · Dolby name", or custom) → `speaker.role`; a sub role forces a SUB band (`:742-753,:988-1002`).
  - Size: small (XO applied) / large (full range) → `speaker.size`; subs show a pill (`:756-770,:1004-1010`).
  - Amp config and band chip, hidden with the bi-amp UI (`:220-222,:772-783,:803`).
  - Measurement output: 1..N, "(out of range)" item, "duplicate" pill → `band.out` (`:805-832,:1021-1027`).
  - Polarity toggle normal / inverted → `band.pol` (`:834-841,:1044-1047`).
  - Remove (`:789-796,:1061-1066`).
- Summary: speaker count, outputs and max channel, duplicate pill, sub pill (`:850-872`).
- [+ Add speaker] (first unused role, next free output) (`:237-239,:1049-1059`); [Renumber outputs] → `config.renumber()` (`:240-242,:1068-1070`); [Patch sheet CSV] `secs_patch_sheet.csv`, columns role,name,size,band,out,pol, UTF-8 with BOM (`:243-246,:1129-1141`).
- Error box: first 4 `config.validate()` messages plus "and N more" (`:255,:875-884`).

Speaker layout card (right column) (`:191-202`)
- `RoomLayoutView` 340×340: screen, sofa, a marker per role (circle main, dashed circle top/height, square sub) with code and outputs; unassigned roles on their own row; click selects (`layout_map.py:148-220`, `:1072-1074`).
- Selected speaker detail: name, role chip, kind, one line per band "BAND → ch → DSP input n → amp [inverted]" (`:886-916`).

DSP profile card (`:333-394,:513-537,:1082-1126`)
- Device: CamillaDSP/BruteFIR, Tesira SERVER-IO, Q-SYS Core 8 Flex/110f, miniDSP Flex/2x4HD, custom → `project.dsp = DspProfile.presets()[key]`.
- FIR taps per channel 512…65536 ("{n} taps (linear phase max x ms)") → `dsp.fir_taps`; total FIR budget 0–4,194,304 step 1024 → `dsp.fir_budget`; IIR bands 0–64; all-pass bands 0–64; BM path delays 0–64.
- Manual edits set the profile to custom; note "window ≈ x ms · resolution ≈ y Hz".
- Presets (taps / budget / IIR / AP / BM) (`model.py:493-497`): camilla 65536 / 2,097,152 / 64 / 64 / 64 (new-project default); tesira 2048 / 51200 / 16 / 16 / 16; qsys 16384 / 65536 / 24 / 16 / 16; minidsp 2048 / 4096 / 10 / 0 / 0; custom 2048 / 32768 / 16 / 16 / 16.

Advance: at least one speaker, and `config.validate()` passes (duplicate roles or outputs, output range, bands, polarity, every speaker has a band) (`:1144-1150`, `model.py:378-405`).

### Step 3: Level check (`step3_level.py`)

Purpose: play each channel and check that the mic input lands in −30…−15 dBFS.

- Channel rows in `ordered_speakers` order: play/stop, role chip, name, `LevelMeter`, value; stored values pre-fill (`:140-146,:213-280`).
- Play → `levels.play_pink(role, out, master_db)` (subs: 50 Hz sine); other play buttons disabled; auto stop after 2.5 s (`:323-366,:37-38`).
- Level subscription writes `extra['levels'][role]` and `extra['spl'][role]` (`:290-299,:393-408`); monitor runs while the step is shown (`window.py:752-753,:772-773`).
- Master output: slider −40…0 dBFS step 0.5, default −20; "playing · code" pill (`:148-188`).
- [Play all channels in turn] (`:166-170,:384-390`).
- Mic input label (channel · device) (`:172-174,:190-199`); embedded mic meter: meter, dBFS, peak, state, [clear peak] (`:176-178`, `mic_meter.py:31-166`).

Advance: blocked while a channel lacks a level (`:428-436`). Skip [레벨 건너뛰기 (기본값 적용)] fills −22 − (len(role)·7 mod 6) dBFS (`:410-425`, `window.py:838-841`).

### Step 4: Positions (`step4_layout.py`)

Purpose: mic arrangement and measurement options, including the timing reference.

- Arrangement cards 1 single / 9 one chair / 13 couple sofa / 17 family two rows, each with a mini map and "N positions" (`:169-237`, `layout_map.py:48-53`); click → `extra['array'] = n`, clears measurements unless the same card (`:359-366`).
- `ArrayLayoutView` 360×300: screen, 1 m circle, chair or sofa, head, 30 cm scale, numbered markers (position 1 larger; half fill = ±20 cm height; done / current colours) (`layout_map.py:238-403`).
- Options (`:279-352`): start delay off/5/10/15 s → `extra['start_delay']`; sweeps per position; time estimate per position and total from `position_seconds` (`:69-93,:480-491`); timing reference acoustic (reference speaker) / loopback (cable) / none → `extra['devices']['timing_reference']` (`timing.py:13-18`); reference output: speaker (`timing_ref_role`, default R) or channel (`timing_ref_output`) or none (`:383-453`).

Advance: speakers exist (`:493-496`).

### Step 5: Measure (`step5_measure.py`)

Purpose: sweep every channel at each position, or import REW `.mdat`. Results go to `project.measurements` and `<project>.irs/<role>_P<pos>.wav`.

Left card
- Header "position k / N" or "all done", "this position about …" (`:591-611,:1076-1081`).
- [Hide graph] / [Show graph] (map grows up to 1.6×) (`:595-599,:955-998`).
- Selectable `ArrayLayoutView` (click or arrow keys) → `select_position`; ignored while busy (`:612-618,:890-904`).
- Plot with an axes button (§4), legend grid (6 columns, "━ code", subs bold, "bold = position 1"), position chips (toggle per-position curves, hover highlights; hidden for ≤ 1 position) (`:619-648,:1125-1139`, `legend.py:70-145`).

Position list card (`:652-779`)
- Rows "NN  point text" with done / waiting, tooltip with the source file; click selects (`:654-662,:1085-1106`); source note "loaded: a.mdat (positions 1–9) · measured (position 10)" (`:665-670`).
- Progress bar across roles (`:671-674,:1283-1294`).
- [Measure this position] (accent; "again" when done; disabled when all done): optional countdown → `ensure_project_path` (creates `~/Documents/SECS/측정 YYYYMMDD-HHMM.secs` without asking) → `plan.validate()` → `MeasurementSession.run_position` (`:678-680,:1109-1111,:1195-1248,:821-848`).
- [Cancel measurement]: stops the countdown or stops after the current sweep (`:681-684,:1250-1258`).
- [Redo last position]: deletes the last done position (`:685-687,:1343-1356`).
- Options: start delay, sweep length, repeats 1/2/4/8 (forced to 1 with an acoustic timing reference) (`:699-731,:906-934,:1044-1072`).
- [Load mdat] (`:736-740,:1414-1531`), [Manage files] (`:747-750,:1571-1573`), [Demo data] (tutorial only) (`:741-745,:1359-1412`).
- Busy drop and elapsed time (`:765-771`); log panel: idle, progress per role and position, countdown, low S/N warning (`LOW_SNR_DB=20`; a tooltip says 40), errors, new project path, mdat lines (`:772-776,:1187-1341`).

Dialogs
- `MdatAssignDialog` (`:247-358`): per measurement title, detail (unreadable / low only (looks like a sub) / full range), role (skip or "code · name"), position when several; pre-filled by `auto_assign`, `split_position`, `looks_like_sub`; problems disable [Import].
- `MdatFilesDialog` (`:361-437`): position | file (or "measured" / —) | channels have/total | [Import…] (whole file into that position) | [Clear] (`clear_position`).
- `load_mdat` (`:1414-1531`): file dialog `*.mdat`, parsed off the UI thread with an overlay, `session.record` with `source[role] = filename`; only the file's positions and channels are replaced.

Advance: blocked while measuring and when there are no measurements (`:1600-1605`). Skip [측정 건너뛰기 (mdat 불러오기)] (`window.py:842-843`).

### Room correction (screen 9) (`step_calibration.py`)

Purpose: compute the correction (RC, BA, XO, channel filters, optional MIMO) and review the results. Left: result card. Right: mode card, then RC / XO / BA cards by tab, then the MIMO card (`:225-252,:2137-2142`).

Result card (`:254-350`)
- Channel combo (role codes; before the first run, the roles measured at position 1; disabled on BA; on XO it switches the XO group) (`:256-264,:1568-1583,:2106-2124`).
- Tabs SPL / GD / XO / BA (BA with ≥ 2 subs) (`:42,:266-277,:2179-2188`).
- SPL tab target editing: drag a point, double-click adds, right-click a point removes, right-click empty space opens the target menu (`:1042-1109`). XO tab: draggable XO line 40–200 Hz (`:297-300,:1447-1462`).
- Legend per tab: before / after / target / MIMO applied / main / sub / sum / before alignment / after alignment / sub sum (`:302-321,:2199-2207`).
- Metrics "±1 dB (20–200 Hz) a % → b %" · "GD spread (20–100 Hz) a → b ms" (`:2283-2288`); filters "PEQ n · FIR n taps | no FIR · XO n Hz · MIMO supports n" (`:2289-2295`); "total delay x ms" and the engine notes (`:2300-2301`).
- BA tab: "sub sum flatness (lo–hi Hz) a → b dB" · "seat spread a → b dB" or "one position"; table sub | mode | gain | delay | polarity | all-pass (names coloured like the curves); fixed filters and layouts line; red warnings from `sub_filter_warnings`, `sub_gain_warnings` (`:2340-2371`).
- Before the first run: position-1 preview and hints (`:2230-2255`).

Mode card (`:391-430`)
- Modes (engine key, filters, latency) (`correction_engine.py:43-48`): minimum delay `zero` (IIR/PEQ, 0 ms); low latency `mixed` (default; FIR 2048, ≤ 10 ms); standard `linear` (FIR 2048, ≤ 30 ms); ultra `ultra` (hidden; FIR up to 16,384, 30–200 ms; unlocked by clicking the spec line 5 times in 3 s in standard mode; "latency limit" 30–200 ms step 10 default 200 → `ultra_ms`). Spec line from `cal.mode_spec`.
- [Compute] → `compute_clicked()`: first run without an XO recommendation runs `recommend_xo()` first; otherwise `cal.compute(project_copy, irs, settings, progress)` on a low-priority thread (`:1762-1770,:1628-1650,:1801-1834`).
- [Cancel] discards; XO and MIMO cards have their own cancel (`:1836-1847`).
- Status line: computing (k/n, seconds left), XO recommending, failed, cancelled, waiting, no measurements, measurements changed, settings changed, done (`:2020-2042`).

Target menu (`:1065-1086`, `target_editor.py:14-34`)
- Presets flat / Harman / cinema → `project.target.points`, `project.bc['target_preset']`; "custom" indicator; save / load target JSON.
- Points clamped to 10–24000 Hz and ±30 dB; at least 2 points; any edit clears the preset (`:1017-1040`).

Result lifecycle
- Results cached next to the project and restored (`:943-971`); a setting change only marks them stale (`:1610-1621`); finishing a run clears the MIMO and subs-only results (`:1873-1882`).
- A hidden export card exists in this step (`:232-236,:352-389,:2404-2473`); only the Export step's buttons are visible.

Advance: always.

### Export (screen 8) (`step8_mdat.py`)

Purpose: write a before/after REW `.mdat` and a CamillaDSP configuration from the Room correction result.

- Result card: "<mode> · total delay x ms · N channels before/after · N subs as measured · MIMO applied N", stale note, or the reason there is nothing to export; role codes and RC summary (`:62-72,:128-167`).
- REW mdat card (`:74-112`): folder label; [Change…] → `project.extra['export']['folder']` (`:178-189`); [Open folder] (`:191-195`); [Save REW mdat] → `cal.export_before_after_mdat(run, target, mimo, cal.project_spl_offset(project))` into `<folder>/SECS_mdat_<RC_BA|RC_BA_MIMO>_<mode>_<YYYYmmdd-HHMMSS>/`, off the UI thread, opens the folder (`:97-101,:200-237`); [CamillaDSP export] → `cal.export_camilladsp(run, target/'camilladsp', mimo)` (`:103-107`).
- Buttons disabled without a result, while computing or exporting; note "saved N files → path" / failed / not in the tutorial.
- Default folder `<project dir>/SECS_export/<YYYY-MM-DD> <stem>/`, or `~/Documents/SECS_export/<date>` (`step_calibration.py:119-126`).
- MIMO included only when on, computed and ≥ 3 positions (`:123-126`).
- The nav button reads "Finish" and does nothing (`window.py:819-822,:854-855`).

## 2. Cross-cutting (`window.py`, `__init__.py`)

- Title "<version> · 측정 위저드", "[튜토리얼]" prefix in the tutorial (`:159,:612-614`); 1280×800; light scheme forced (`:159-171`).
- Step bar: 7 clickable steps, done / current / not reached (`:54-114,:290`).
- Sidebar (232 px, `:476-573`): brand and version; sections setup ("preset · subs N · outputs N"), measurements ("k / N positions"), correction ("—" / computing / done), REW (connected / not), project (file / unsaved / tutorial); [Help], [Tutorial] / [End tutorial], [Restart tutorial], [Save project ▾ Save as…], [Open project]; language row hidden (`ENGLISH_UI=False`).
- Nav bar: [← Back], centre message (blocked reason, skip notes), skip button on steps 1, 3, 5, [Next →] / [Finish] (`:701-726,:851-863`).
- `go(n)`: monitor start/stop, context handover, REW refresh when stale, "preparing the screen…" if the last visit was slow (`:746-783`).
- New project: no button; default 2.0, CamillaDSP profile, curtain 20–20 kHz, phase mixed (`__init__.py:54-87`). Save (Ctrl+S): current path or a dialog (default "측정.secs"); copies `.irs/` on a path change and rewrites IR paths (`:966-992`); fills SPL offsets (`:939-964`); writes the side `.mdat` (`:915-937`). Save as (Ctrl+Shift+S). Open (Ctrl+O) → `open_project` (bi-amp to single-amp) (`__init__.py:90-138`, `:994-1012`).
- Dirty prompt before closing and before the tutorial: save / don't save / cancel (`:846-849,:1034-1052`); close stops monitors, measurement (waits 3 s), computation, MIMO workers (`:1054-1068`).
- Language switch re-titles and refreshes everything (`:687-699`); strings via `t()` and the `en_g*` catalogs (`strings.py:105-145`); tooltips live (`tooltips.py`).
- No expert mode and no About dialog upstream.
- REW watch every 3 s, paused on steps 3, 5, 8 while REW is up (`:351-389`, `rew_cache.py:58-266`).
- Tutorial mode: new window with FakeREW and the demo project, pink theme, saving and exporting blocked, fast level playback, stored demo result (`:578-661`, `style.py:371-372`).
- Busy indicators (polled every 250 ms, `:181-252`): loading overlay for mdat/sample loading and slow screen preparation (its cancel button is never wired) (`loading.py:218-338`); bottom-right busy panel "what · elapsed · estimate (left)" for measuring, computing, BA, MIMO, XO recommendation, exporting (`busy.py:25-160`); startup splash (`splash.py:30-124`).
- Messages appear in the nav centre line, step 1 hint box, step 2 error box, step 5 log, correction status lines, export notes, tutorial toasts and message boxes; there is no notification centre.
- Shortcuts (`:333-348`): Alt+← / Alt+→, Ctrl+1…7, Ctrl+S, Ctrl+Shift+S, Ctrl+O, F1 help, Ctrl+M floating mic meter; help: Ctrl+F, Enter next match, Esc; map arrows (`layout_map.py:303-312`); axis menu Enter applies (`axis_range.py:189-193`).
- Tooltips: about 150 keys, 800 ms delay, 420 px wide (`tooltips.py:30-191,:314-381`). Mouse wheel over a closed combo scrolls the page (`wheel_guard.py:14-30`).
- Help window (`help_dialog.py:490-753`, text `tutorial_text.py`): non-modal 640×720, title "도움말 · <pos> · <step>", "k / 7" chip, search; tabs first time (step 1), follow along, check, troubleshooting, real REW, plus "mdat import" on step 5 and "CamillaDSP" on export (`tutorial_text.py:40-46,:507-510`); screenshots open an image viewer; follows the current step.

## 3. Tutorial (`tutorial_quests.py`, `tutorial_overlay.py`)

- Starts with `set_trial(True)`; `plant_wrong_out` sets SW3's output to 16 for a later quest (`tutorial_quests.py:144-162`); quests with a false `when()` are dropped (BA quests need ≥ 2 subs) (`overlay:556-563`); the end offers restart or finish (`overlay:756-774`).
- Quest fields: `screen`, `kind` (look / act), `target`, `text`, `why`; completion `signal`, `done`, `base`; behaviour `auto`+`note`, `busy`+`busy_text`, `keep`, `ok`, `blocked`, `animate`, `apart` (`:47-67`).
- Quests (`QUESTS :286-456`):
  - Devices: look sidebar; look step bar (pink = tutorial, dashed = look, solid pink = act); look REW card; look mic card; act [Check again]; act [Next].
  - Setup: look presets; look patch sheet; look DSP card; act change SW3's output from 16 to 6 (auto `_set_out`); act [Next] (auto-renumber if duplicates block).
  - Level: look master card; act [Play all] (done when every role has a level; auto `apply_defaults`); act [Next].
  - Positions: look arrangement card; look options; look [Single] (why: more positions, MIMO needs ≥ 3); act [Next].
  - Measure: look position list; act [Demo data] (auto `load_sample`); look plot; look [Load mdat]; act [Next].
  - Room correction: look mode card and [Compute]; look result bar and plot; look tabs plus the RC/XO/BA cards (tabs cycle every 3.6 s); act [Compute] (auto `_compute`, no XO recommendation); look SPL; look metrics, filters and summary; look GD; act XO tab; look XO plot, legend and card; act BA tab (≥ 2 subs); look BA graph; look BA metrics and table; look BA settings; act set the infra sub to SW3 (auto `set_infra(last sub, 25 Hz, LR24)`); look fixed HP/LP/PEQ; act [Compute subs only]; look the new BA result; act [Next].
  - Export: look [Save REW mdat]; look [CamillaDSP export] (coach button reads [End tutorial]).
- Glossary of 25 terms explained under the first card that mentions them or under [Why?] (`:462-511`).
- Overlay (`overlay:87-233`): dims all but the targets; look targets dashed and not clickable (wheel passes); act targets solid pink with a pulsing ring and clickable; `apart` outlines each target; tutorial buttons always clickable; clicking elsewhere shakes the coach card with a toast (`:1269-1279`); targets scrolled into view first (`:882-903`).
- Coach card (340 px, draggable) (`overlay:236-405`): "<step> · k / n", pause, text, glossary, [Why?], hint, [← Back] and [Continue →]; placed away from the target, controls, sidebar and nav (`:1052-1132`).
- Act quests (`:716-754`): first [Continue] press shakes the hint; second runs `auto()` with its note; blocked shows the reason; busy shows its text.
- Tick every 150 ms (`:1216-1267`): auto-advance when done; skip an act quest whose target is missing > 0.45 s; toast at each step end.
- Quest panel (top-right, folds into a pill) (`overlay:408-477`); pause hides everything and docks a resume button; resume rewinds to a blocked screen (`overlay:575-661`).

## 4. Graphs

- Step 5 measurement plot (`step5_measure.py:619-648,:1141-1184,:501-515`): log x 5–22000 Hz with fixed ticks 5…20k (`plot.py:20`); y −35…+20 dB; no pan/zoom; one curve per (role, position); 1/12-octave Gaussian smoothing on a 280-point log grid; normalised to the 200–2000 Hz mean; colours per channel, subs 3 px, mains 2 px, position 1 bold, others 1 px at alpha 0.38, hover highlight (`legend.py:33-58`, `style.py:67-82`); axes popover: frequency and level limits, [Auto], [Apply], "lower must be below upper", remembered for the run (`axis_range.py:43-203`).
- Room correction plot (`step_calibration.py:279-300,:2144-2401`): 300–380 px high, log x, no pan/zoom.

| Tab | x (Hz) | y | Curves |
|---|---|---|---|
| SPL | 5–20000 | dB re target, target median −30/+15 | before, after, target (dashed), MIMO applied |
| GD | 5–5000 | ms, 1st–99th percentile | before, after |
| XO | 5–500 | dB re target, −30/+12 | target, main, sub, sum from `cal.xo_preview(run, group, hz, role)` |
| BA | 5–300 | dB re target, −40/+6 | per sub before (dashed) and after; sub sum before and after; target |

- SPL: curves shifted by `_level_offset` so the target sits at 0 dB; target handles; RC lower and upper lines with labels and shading outside the band; before a run, the position-1 measurement and the target; display smoothing 1/24 octave (`calibration.py:66`). GD shares the RC lines. XO smoothing 1/12 octave. BA data from `cal.sub_combine(sub_run or run)`, 8-colour cycle matching the table.
- Level meter (`step3_level.py:25-91`): −60…0 dBFS, ticks every 10 dB, −30…−15 shaded, red above −15 or clipping.
- Layout drawings: `RoomLayoutView` (step 2), `ArrayLayoutView` (steps 4 and 5), `MiniArrayMap` (`step4_layout.py:97-166`).

## 5. Room correction settings (`CalibrationSettings`, `project.extra['calibration']`, `calibration.py:33-150`)

RC range card (SPL and GD tabs) (`:443-501,:1147-1191`)

| Field | Range / default | Writes |
|---|---|---|
| Lower | 10–200 Hz, step 5, default 20 | `rc_lo` |
| Upper auto | default on (Schroeder, max 500 Hz) | `rc_hi = None` |
| Upper | 100–20000 Hz, step 50, when auto is off | `rc_hi` |
| Room volume | 0–100000 m³ (0 = none) | `room_volume_m3` |
| Dimensions W×D×H | 0–100 m, step 0.1 | `room_dims_m` (all three set the volume) |
| Boost limit | 0–10 dB, default 10 | `boost_max_db` |

XO card (XO tab; only with subs) (`:503-556,:1410-1490`)

| Field | Range / default | Writes |
|---|---|---|
| Group | | |
| XO | 40–200 Hz, step 5, default 80 (or `project.xo`) | `xo[group]` |
| XO PEQ | default on | `xo_peq_on` |
| XO PEQ lower | 0–1000 Hz, 0 = auto (XO ½) | `xo_peq_lo` |
| XO PEQ upper | 0–1000 Hz, 0 = auto (XO × 4) | `xo_peq_hi` |

- "recommended N Hz" label; [Recommend XO] → `cal.recommend_xo` (60/80/100/120 Hz) writes `xo` and `xo_rec`, then recomputes; its own cancel and status; without subs "no subs, no XO" and disabled.

BA card (BA tab, ≥ 2 subs) (`:558-860,:1230-1408`)
- Infra sub (none or a sub); then infra XO 10–300 Hz (25 when first set), type LR24/LR12/LR48/BW12/BW24 (default LR24), infra max boost 0–24 dB (default 24) → `infra_boost_max_db`; choosing it writes a LP on that sub and a matching HP on the others.
- Per sub: mode auto / fixed / Group A / Group B → `subs_manual[role].mode`, `bundle`; for groups, layout relative-fixed / DBA / multi-sub → `bundle_layouts[b].kind`; DBA row front/rear → `dba_row`; rear delay trim −20…20 ms → `rear_trim_ms`.
- Values (fixed, or group with relative-fixed): gain −60…+20 dB step 0.5; delay 0–500 ms step 0.1; polarity; all-pass 1–2 (f 0–500 Hz, Q 0.1–20, default 0.71).
- Fixed filters (every mode): HP and LP 0–300 Hz with type off/LR24/LR12/LR48/BW12/BW24 (picking a type sets HP 20 or LP 80); PEQ 1–3 (f 0–500 Hz, Q 0.1–20, gain −30…+12 dB).
- Main extra delay 0–100 ms (only if a sub is fixed) → `sub_main_delay_ms`; [Use as MIMO support] default on → `mimo_support`.
- [Compute subs only] → `cal.compute(..., subs_only=True)` into `sub_run` (BA tab only); [Take result values] → `settings.take_sub_values(run)`; summary per sub; red warnings.

MIMO card (`:879-940,:1193-1227,:1778-1799`)
- [Use MIMO] → `mimo_on` (default off; disabled below 3 positions with the reason).

| Field | Range / default | Writes |
|---|---|---|
| Support upper limit (Fsiso) | 50–150 Hz, step 5, default 150 | `fsiso` |
| Lower auto | default on | `mimo_low = None` |
| Lower | 15–150 Hz, default 20 | `mimo_low` |
| Max per input | 0–12, 0 = no limit | `mimo_max_supports` |
| FIR supports | default off (IIR: PEQ 9 + AP 3); on = 8,000-tap FIR | `mimo_fir` |

- [Compute MIMO] → `cal.compute_mimo(...)` (needs a correction result); cancel; status: computing (k/n), compute first, not computed, settings changed, done "FIR|IIR supports N · ≤ cap per input · <FIR budget vs DSP budget>".

Other inputs: the target curve (`project.target`, `project.bc['target_preset']`), the DSP profile (`project.dsp`), and the new-project curtain and phase defaults (`__init__.py:81-87`).
