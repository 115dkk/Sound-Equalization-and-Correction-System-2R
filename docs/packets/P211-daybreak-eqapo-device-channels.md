# P211 (Daybreak): Equalizer APO channel numbers from the speaker layout and the system-stage outputs

Worktree `E:\wt\forum-eqapo`, branch `fix/forum-eqapo-channels` (Claude
created it from `main` d469cca4; `git status` shows only this packet when you
start). Every cargo command runs with
`CARGO_TARGET_DIR=C:\cargo-target\forum-eqapo`: export it once per shell and
check it with `echo $CARGO_TARGET_DIR` before the first build. You are the
only one building in that target. Never run `cargo clean`. Per-package tests
only (`-p secs-filters`, `-p secs-service`, `-p secs-apply`), plus one
`cargo clippy --workspace --all-targets --locked -- -D warnings` and
`cargo fmt --all -- --check` at the end. Run every build and test in the
foreground and end your turn only after the report. One commit (leave this
packet file out of it), title
`fix: Equalizer APO 채널 번호가 Windows 스피커 배치와 시스템 단계의 출력을 따릅니다`,
trailer `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`. Do not
push, do not touch `docs/**`, `CHANGELOG.md`, `features.toml`, the UI or
`crates/secs-service/src/mdat_import.rs` / `calibration.rs` (Claude edits
those on another branch).

## The defect

`secs_filters::windows_channels` (crates/secs-filters/src/lib.rs, about
line 236; a port of upstream `export_filters.py::windows_channels`) numbers
device channels by compacting the Windows positions the plan uses. For a 2.2
plan (L, R, SW1, SW2, LFE input) it gives L=1, R=2, SW1=3 and LFE input=3,
SW2=4, and the guide calls channel 3 "LFE". No Windows speaker setup holds
FL FR LFE without FC: a 2.2 user runs the device as 5.1 or 7.1, where 3 is
FC and 4 is LFE. So the LFE input reads the centre channel (a film's
dialogue goes +10 dB through the 120 Hz low-pass into the subs) and the
table contradicts its own header. Separately, the export ignores the output
each speaker was measured on (`Band.out` in the project config, which
`crates/secs-service/src/measure/settings.rs:146-160` sends to native and
REW-API sweeps), so a speaker can be corrected on a channel other than the
one it was measured on.

## The rule

- **Inputs** (where Windows delivers each role's content) are numbered by
  the first layout in this list that contains every `plan.inputs` role
  (`"LFE"` included when present), at the role's 1-based index in it:

  | layout | roles in device order | Windows names | `name_en` / `name_ko` |
  |---|---|---|---|
  | stereo | L R | FL FR | `stereo` / `스테레오` |
  | quad | L R Ls Rs | FL FR BL BR | `quadraphonic` / `쿼드` |
  | quad | L R Lb Rb | FL FR BL BR | `quadraphonic` / `쿼드` |
  | 5.1 | L R C LFE Ls Rs | FL FR FC LFE SL SR | `5.1` / `5.1` |
  | 5.1 | L R C LFE Lb Rb | FL FR FC LFE BL BR | `5.1` / `5.1` |
  | 7.1 | L R C LFE Lb Rb Ls Rs | FL FR FC LFE BL BR SL SR | `7.1` / `7.1` |
  | 7.1.4 | L R C LFE Lb Rb Ls Rs Tfl Tfr Tbl Tbr | FL FR FC LFE BL BR SL SR TFL TFR TBL TBR | `7.1.4` / `7.1.4` |

  Checked results: 2.0 → stereo; 2.1 and 2.2 → 5.1 with LFE 4; 3.0 → 5.1
  with C 3; 4.0 → quad (Ls 3, Rs 4, as today); 4.1 → 5.1 (LFE 4, Ls 5,
  Rs 6); 5.1, 7.1 and 7.1.4 → the same numbers `windows_channels` gives
  today.
- **Outputs** are the system-stage outputs: a table `role → Band.out` that
  the caller passes. When the largest output number exceeds the chosen
  layout's channel count, keep scanning the list for the first later layout
  that also contains the inputs and has at least that many channels.
- **Fallback.** Use `windows_channels` exactly as today (numbers, config
  text and guide text byte-identical) when any of these holds: no output
  table was passed; a `plan.outputs` role is missing from it; a number is
  below 1, above 256 or used twice; no layout contains the inputs (9.1.6
  and anything with Lw, Rw, Tml, Tmr); no layout is wide enough for the
  outputs. The upstream goldens (`crates/secs-filters/tests/golden.rs`,
  `eqapo_playback.rs`, `optimization_tests.rs`) call the old path and must
  pass unchanged; if one of them would change, stop and report instead of
  editing a golden.

## API (additive; `secs-filters` is a published crate)

Keep every existing public function and its output unchanged. Add, with
doc comments:

- a public layout description (name in English and Korean, the role list,
  the Windows names) and
  `pub fn device_channels(plan: &FilterPlan, outputs: &[(String, usize)]) -> Result<Option<DeviceChannels>, FiltersError>`
  returning the chosen layout plus the input and output `ChannelNumbers`,
  or `None` for the fallback cases above;
- `pub fn export_eqapo_observed_with_channels(run, folder, options, outputs: Option<&[(String, usize)]>) -> Result<Installed, FiltersError>`;
  `export_eqapo_observed_with_options` becomes a call to it with `None`.

Names may differ if the crate's conventions suggest better ones; say so in
the report. Read `cargo doc` for any API you are unsure of.

## Text when a layout is chosen

The fallback keeps today's text. With a layout, change only these lines.

`config.txt` (ASCII):

- line 4: `# Device format: {rate} Hz (the FIR wav files are {khz} kHz). Speaker setup: {name_en}.`
- the line `# Default channel numbers (1 = first channel of the device, Windows order FL FR FC LFE BL BR SL SR ...):`
  becomes `# Default channel numbers (1 = first channel of the device): inputs follow the Windows {name_en} speaker setup, outputs the outputs set in the SECS system step.`

Guide (`EqAPO 적용 방법.txt`, Korean):

- step 2 becomes
  `2. 윈도우 소리 설정에서 그 장치의 형식을 {rate} Hz로, 스피커 구성을 {name_ko}로 맞추십시오.`
  for every rate (the existing other-rate branch already uses this voice;
  keep the line after it as the branch has it today);
- the table header line becomes
  `■ 채널 번호(장치 채널, 1부터. 입력은 윈도우 {name_ko} 배치의 채널, 출력은 SECS 시스템 단계에서 정한 출력)`;
- the column line becomes `   역할    입력        출력` and each row
  `format!("   {role:<6} {input:>4} {in_name:<5} {output:>4} {out_name}")`
  where a missing number prints `   -` with an empty name, `in_name` is the
  layout's Windows name of the input channel, and `out_name` the layout's
  Windows name of the output channel, or `-` past the layout's width.
  For a 2.2 project with default outputs the rows carry
  `L 1 FL 1 FL`, `R 2 FR 2 FR`, `LFE 4 LFE -`, `SW1 - 3 FC`, `SW2 - 4 LFE`
  (with the padding above).

Keep the rest of both files as they are, including the edit-the-Eval-lines
paragraph.

## The service

`crates/secs-service/src/export.rs` builds the EQ APO writer
(`EqapoWriter::prepare`, about line 1105); the EQ APO apply path
(`crates/secs-service/src/apply.rs`, `kind: ExportKind::EqualizerApo` about
line 263) should go through the same writer; check it does. Build the output
table from the open project's `config.speakers`: a speaker with exactly one
band gives `(role, band.out)`; a speaker with more than one band makes the
whole table `None` (fallback), because the EQ APO plan has one output per
role. Pass it to the new export function.

## Tests

- `secs-filters` unit tests for `device_channels`: 2.0, 2.1, 2.2 (the rows
  above), 3.0, 4.0, 4.1, 5.1, 7.1, 7.1.4 inputs; a 2.2 table with SW1 on 5;
  every fallback case (no table, missing role, 0, duplicate, 9.1.6, outputs
  wider than 7.1.4).
- a `secs-filters` test that writes the 2.2 export to a temp folder and
  asserts `Eval: inLFE = 4`, `Eval: outSW1 = 3`, `Eval: outSW2 = 4`, the
  two changed `config.txt` lines and the guide rows.
- a `secs-service` test (next to the existing EQ APO export tests; find
  them with `grep -rn EqualizerApo crates/secs-service/tests`) that sets SW1
  of a 2.2 project to output 5 through the config edit the screen uses and
  checks `Eval: outSW1 = 5` and `Eval: inLFE = 4` in the exported
  `config.txt`. Update any service test that pinned the old 2.x numbers and
  name each one in the report.

## Report

Files changed, the final API names, each test added or changed with its
result, the clippy and fmt results, and the commit hash.
