# P38 (Opus 5.5): kit and frame follow-ups from the step screens

Worktree: `/home/user/wt/p38` (branch `m5/p38`). Follow
[`M5-screens-common.md`](M5-screens-common.md) for the rules, the checks and
the report, except that this packet **may** edit the kit (`src/ui/`), the
frame (`src/app/`), the gallery (`src/gallery/`) and the four screen areas
(`src/screens/*`) for the items below. Do not edit `src/ipc/`, Rust or the
catalogs except to add keys for new kit text.

The four screen packets (P30, P31a, P31b, P32) each worked around a gap in
the kit or the frame. This packet closes the gaps once, in the kit, and
moves the screens onto them, so P33 wires one consistent surface.

## Items

1. **Primary action can be off.** `PrimaryAction` gains `disabled?: boolean`;
   `StepHeader` disables the button when the step is blocked or the action
   says so. Then:
   - P31b `calibrateHeader` shows the primary disabled (not missing) while
     XO recommendation, subs-only or MIMO runs;
   - P32 `exportHeader` shows it busy only while the CamillaDSP export runs
     and disabled while another export runs;
   - P31a `measureHeader` keeps busy for its own jobs.
2. **A place for the help panel.** The frame gets a slot for a panel that
   sits between the step header and the job tray without covering either
   (P32's `HelpPanel` positions itself against the frame's content root;
   read `src/screens/help/HelpPanel.tsx` and give it the root it expects,
   e.g. an `aside` prop on `AppFrame`). Show it in a gallery specimen of the
   frame.
3. **Graph fixes and features** (`src/ui/plot/`):
   - a double click on a target handle must not reach LogPlot's own
     double-click listener (today it adds a point and moves it);
   - `TargetHandles` and `XoLine` report the end of a drag or a key nudge
     (`onCommit`), so screens commit once per gesture; replace P31b's
     pointerup/keyup wrapper with it;
   - a shaded band with only a lower edge (the RC band before the automatic
     upper limit is known); drop P31b's overlay work-around;
   - `Graph` takes a controlled axis range (`range`, `onRangeChange`) and
     keeps the uncontrolled default, so P33 can keep the measurement plot's
     range for the run (P31a gap 7).
4. **Select and Menu.** The kit `Select` gets an inline-label layout
   (replace P31b's local `InlineSelect`); `Menu` items get a checked state
   (replace P31b's check icon and disabled 사용자 지정 work-around).
5. **Specimens through `FrameBox`.** `FrameBox` already takes a screen's
   `header`. Move P31a's specimens off their hand-built `AppFrame`, and
   P31b's and P32's specimens onto `header`, so every screen specimen
   shows its real primary action and status.
6. **Stale results export.** The service will export a stale result as
   upstream does (P36), so the 내보내기 step is not blocked by
   `stale_result`: show the stale state as a warning on the result card
   and in the header status, keep the export enabled, and remove
   `stale_result` from `BLOCKED_REASONS` and the catalogs if nothing else
   uses it.

## Checks

As in the contract (typecheck, tests, both builds, the gallery shots), plus
look at the changed specimens in light and dark, Korean and English, and at
1024×700. Existing tests keep passing; add tests for the new kit behaviour
(the handle double click, `onCommit`, the controlled range, the checked menu
item, the disabled primary).

## Report

Per item: what changed, which screens moved onto it, the tests, and the
screenshot folder.
