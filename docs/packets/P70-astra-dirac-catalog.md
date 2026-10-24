# P70 (ASTRA): catalog copy for Dirac Live project import

Worktree: `E:\Sound-Equalization-and-Correction-System-2R\.claude\worktrees\dirac-liveproject-support-2e310d`
(branch `claude/dirac-liveproject-support-2e310d`). Work only there. Do not
commit, do not push, do not spawn sub-agents. Claude reviews the diff and
commits. Another worker builds `crates/secs-dirac` at the same time; do not
touch Rust files.

Read first: `docs/adr/0013-dirac-liveproject-import.md`,
`docs/DIRAC_LIVEPROJECT.md` (sections 'IR의 성질' and '가져오기 규약'),
`CLAUDE.md` (screen copy rules: 합니다체 sentences, noun-phrase labels and
statuses, labels that name the effect, 14 px floor), `docs/UI-TERMS.md`, and
the two catalogs `apps/secs2r/ui/src/i18n/ko.json` and `en.json`.

## What changes in the product

The import that used to take only REW `.mdat` files now also takes Dirac Live
projects (`.liveproject`). It is the same flow: the file picker accepts both,
the same assignment dialog lists one row per measurement (for a Dirac project:
one row per position and channel, titled like `Left P1`, `Subwoofer 2 P3`),
and the job is the same. Facts the copy may state:

- A Dirac project carries every measured position; row positions come from
  Dirac's measurement order and position 1 is Dirac's main listening position.
- Channels are suggested from Dirac's speaker names; subwoofers become SW1,
  SW2 … in Dirac's order. Unrecognised names stay on 건너뜀.
- Dirac's stored IRs already include its microphone calibration. Do not load
  the calibration file again in REW when opening an mdat exported from these
  measurements.
- Dirac aligns each position on its own, so arrival differences between
  positions are not kept; differences between channels at one position are.
- In 파일 관리 (`measure.files.*`), importing a Dirac project into position n
  brings in only Dirac's position n (an mdat still brings every measurement in
  the file into that position).
- Connection and level checks are skipped for either file type.

## Keys to rewrite (ko and en together)

Rewrite so that the text no longer says the import is mdat-only. Use
'측정 파일' / 'measurement file' where both formats are meant; name the formats
('REW mdat', 'Dirac Live 프로젝트' / 'Dirac Live project', with `.mdat` and
`.liveproject` where file extensions help) where the user has to know which
files work. Keep each string as short as the original allows.

- `frame.step.measure.purpose`, `frame.status.path.mdat`,
  `jobs.kind.mdat_import`, `gallery.sample.empty.body`,
  `measure.graph.empty.body`, `measure.import.title`,
  `measure.import.description`, `measure.mdat.load`, `measure.files.title`,
  `measure.files.intro` (state both behaviours of 파일 관리, see above),
  `measure.files.load.label`, `connect.method.mdat`,
  `connect.method.mdat.needs`, `connect.status.skipped`, `connect.mdat.body`.
- Help: `help.tab.mdat`, `help.measure.mdat.start.1`,
  `help.measure.mdat.start.2`, `help.connect.steps.goal`,
  `help.measure.steps.goal`, `help.measure.steps.7.do`,
  `help.measure.real.3`, `help.calibrate.trouble.1.fix` only where the
  sentence would otherwise tell a Dirac user there is no way in, or quotes a
  button label you renamed (`measure.mdat.load`). Leave REW-specific help
  (naming measurements in REW, Acoustic timing reference, the channel table)
  and every export string as they are.
- Tutorial: `tutorial.q.connect.connection.text` and
  `tutorial.q.measure.mdat.title`/`.text` only if they quote a label you
  renamed; otherwise leave the tutorial alone.

## Keys to add (ko and en)

A help section for Dirac projects in the mdat help tab:
`help.measure.mdat.s7.head` and `help.measure.mdat.s7.1` …
`help.measure.mdat.s7.N` (N at most 5), covering the facts above that a user
needs: what the rows are, how positions and channels are suggested, the
microphone calibration caveat, the timing caveat. Then append to the `mdat`
entry of `apps/secs2r/ui/src/screens/help/content.ts`, after the s6 bullets,
`{ kind: "heading", text: "help.measure.mdat.s7.head" }` and a `bullets` block
with the new keys. That data entry is the only change allowed in
`content.ts`.

## Writing rules

Korean: 합니다체 for sentences, noun phrases for labels and statuses, no
em-dash, no colon at the end of a sentence followed by a list, single quotes
for terms, no '~의 경우', no '~될 수 있다', no passive '읽힙니다/보입니다' for
what the user does, no dressed-up phrasing. English: plain, short, sentence
case as the catalog already does. Both catalogs must end with identical key
sets and identical `{placeholder}` sets per key.

## Tests that quote the old copy

Update the expectations that quote rewritten strings, at least
`apps/secs2r/ui/src/app/AppFrame.test.tsx` (`frame.status.path`),
`apps/secs2r/ui/src/screens/help/HelpPanel.test.tsx` (test title mentions
'mdat 가져오기'), and any other test that fails because of a string you
changed. Change only the quoted strings, not test logic.

## Gates (foreground; all must pass, then report)

```sh
cd apps/secs2r/ui
npm ci
npm run typecheck
npm test
npm run build
```

If the font subset gate in the tests or build complains about new glyphs,
follow its message (regenerate the subset with the repository's script) and
report which files changed.

## Allowed files

`apps/secs2r/ui/src/i18n/ko.json`, `apps/secs2r/ui/src/i18n/en.json`,
`apps/secs2r/ui/src/screens/help/content.ts` (the one data entry above), test
files whose quoted strings changed, and font subset files only if the gate
requires it.

## Report

Every key changed or added with its old and new ko/en text (a table), the
tests you edited, gate results, and any string where the rules forced a
choice you are unsure of.
