# P117 (ASTRA): text, mock rules and records for the output rate (ADR 0017)

Worktree: `E:\Sound-Equalization-and-Correction-System-2R\.claude\worktrees\output-rates`
(branch `claude/output-rates`; P114–P116 are committed). Do not commit, do
not push. Work only in this worktree. Do not change markup, styles or layout.

Read first: `CLAUDE.md`, `docs/UI-TERMS.md`, `docs/adr/0017-output-sample-rate.md`,
`docs/packets/P116-opus-output-rate-screen.md`, the new keys P116 added
(`system.dsp.output_rate*`, `calibrate.mode.fir_over*`,
`calibrate.mimo.state.budget*.rate`, `export.go_system`,
`export.extra_rates*`, `export.rate.*`) and the four `export.kind.*.body`
keys it changed, in `apps/secs2r/ui/src/i18n/ko.json` and `en.json`;
`apps/secs2r/ui/src/ipc/mock.ts` (`dsp_set`, `export`);
`crates/secs-service/src/project_edit.rs` (`dsp_set`) and
`crates/secs-service/src/export.rs` (`extra_rates_for`).

## Tasks

1. **Catalog.** Rewrite every key above in both languages. Korean sentences
   are 합니다체; instructions end in -십시오 (some first versions use -세요);
   labels and states are noun phrases; no em-dash; plain and short; one term
   per thing (출력 레이트, 추가 레이트, 내보낼 DSP). Placeholders identical.
2. **Help text.** `help.export.steps.2.see`, `help.export.camilla.s7.1`,
   `s7.8`, `s13.4` (and any other help key that states 48 kHz or 48000 as
   the export rate; search both catalogs) still assume a fixed 48 kHz.
   Rewrite them to say the files are written at the export DSP's output
   rate (48 kHz unless the DSP or the user chose another), keeping each
   key's role. Measurement and REW mdat stay at 48 kHz; do not change text
   about those.
3. **Mock rules.** Make `ipc/mock.ts` follow the service as it is now:
   `dsp_set` with only `output_rate` changed keeps the profile (camilla stays
   camilla); any other field makes it custom. `export` with `Auto` drops an
   extra rate equal to the resolved rate instead of refusing it; a fixed
   main rate repeated as an extra rate is still refused. Update the tests
   that pin the old behaviour (`ipc/mock.test.ts`, `state/writers.test.ts`
   and the system screen tests) and the specimen that showed 자동 turning
   the device into 사용자 지정.
4. **Records.** Add the data-ui ids P116 introduced
   (`system.dsp.output_rate`, `export.kind.<kind>.rate`, `export.extra_rates`,
   `export.extra_rates.summary`, `export.extra_rates.list`,
   `export.extra_rates.<rate>`, `export.outcome.<kind>.rate_fallback`,
   `export.outcome.<kind>.go_system`, `export.outcome.<kind>.rate`,
   `calibrate.mode.fir_over`) and the eight new specimen ids to
   `docs/DESIGN.md` §11 and §12 and `docs/M5-reach.md`. Register the screen
   in `features.toml` the way other screen features are registered (point
   `implemented` entries at real tests). Add one `CHANGELOG.md` sentence for
   the screen under the next version (merge with P115's entries if that
   reads better).

## Files

You may edit: `apps/secs2r/ui/src/i18n/ko.json`, `en.json`,
`apps/secs2r/ui/src/ipc/mock.ts`, `ipc/mock.test.ts`,
`apps/secs2r/ui/src/state/writers.test.ts`,
`apps/secs2r/ui/src/screens/system/*.test.ts(x)`,
`screens/system/fixtures.ts`, `screens/system/specimens.tsx` (fixture choice
only), `docs/DESIGN.md`, `docs/M5-reach.md`, `features.toml`,
`CHANGELOG.md`.

## Verification (foreground; do not end the turn before they finish)

```sh
cd apps/secs2r/ui
npm run typecheck
npm test
npm run build
cd ../../..
python3 .github/scripts/check_feature_tests.py   # Windows lists one known secs-sys gap; report anything else
```

## Report

Each key's final ko and en text, the help keys changed, the mock changes,
files changed, and the commands' last lines.
