# P34 (ASTRA): the screen catalogs

Worktree: `/home/user/wt/p34` (branch `m5/p34`). Do not commit; Claude
reviews the diff and commits.

The screens, their wiring and the tutorial quests are in. Every screen
string is a key in `apps/secs2r/ui/src/i18n/ko.json` and `en.json`
(about 1,700 keys each). The Korean was written screen by screen by
different workers, and the English is a first draft. This packet makes both
catalogs read as one product and closes the M5 catalog conditions
(`docs/M5.md` §6 문구, §8).

Read first: `CLAUDE.md` (the screen rule: 합니다체 sentences, noun-phrase
labels and states, labels that say their effect, 14 px floor),
`docs/DESIGN.md` (voice and the glossary of on-screen terms, if present),
`docs/M5.md` §3 (U7: English from the start), `apps/secs2r/ui/src/i18n/{index.ts,i18n.test.ts}`,
and load the skill `anthropic-skills:make-interfaces-feel-better` with the
Skill tool (its UX-writing and Korean-register parts) before editing.

Upstream's own English is in `src/secs/wizard/en_g*.py` and its glossary in
`src/secs/wizard/tutorial_quests.py` (`GLOSSARY`); use them where a key
carries upstream's text, so terms match the upstream manual. The upstream
tree is read-only.

## What you do

1. **One vocabulary.** Make a term table (Korean, English, where used) of
   the domain words on screen: 측정 위치, 청취 위치, 측정 출력, 스윕, 보정,
   룸 보정(RC), 서브 정렬(BA), 크로스오버(XO), 군지연(GD), 타겟, 프리셋,
   묶음, 인프라 서브, 전후 비교, 내보내기 … Pick one word per idea and use
   it in every key in both languages. Put the table at the top of a new
   `docs/UI-TERMS.md`.
2. **Korean.** Sentences in 합니다체; labels, buttons, tabs, column heads
   and states as noun phrases; buttons say what happens (보정 계산, not
   확인); no cheerleading (no "~볼까요?", "멋져요", "!"), no doubled
   politeness, no English where a Korean word is standard. Keep sentences
   short, one idea each. Keep every placeholder.
3. **English.** Rewrite the draft as plain product English: sentence case,
   imperative for actions, no "please", no exclamation marks, the same
   terms as the table. Match Korean meaning, not word order.
4. **Unused and missing keys.** Add a test to `i18n.test.ts` that every
   catalog key is referenced from `src/` (string literals, template
   prefixes the code builds with — read how keys are built, e.g.
   `jobs.log.${code}`, `error.${code}`, and allow those prefixes
   explicitly) and remove keys nothing uses. Keep the existing tests
   (same keys and placeholders, register, no exclamation marks).
5. **Length.** Long English strings can break layouts the Korean fits.
   After editing, run the gallery shots and look at every specimen in
   English at 1024×700 for clipped or wrapped labels; shorten the string,
   not the layout. List any you could not fix by wording.

## Rules

- Change only the catalogs, `i18n.test.ts`, and `docs/UI-TERMS.md`. Do not
  change keys' names, screen code, markup or styles. A string that cannot
  be right without a code change (a sentence assembled from pieces, a
  plural the code does not pass): leave it and list it.
- Do not change meaning. Where the Korean is wrong about what the app does,
  check the code and list it rather than guessing.

## Verification (foreground; finish before you report)

```sh
cd /home/user/wt/p34/apps/secs2r/ui
export PATH=/opt/nvm/versions/node/v24.22.0/bin:$PATH
npm ci --no-audit --no-fund
npm run typecheck
npm test
npm run build
npm run build:gallery
CHROMIUM_PATH=/opt/pw-browsers/chromium-1194/chrome-linux/chrome node scripts/gallery-shots.mjs --out /tmp/claude-0/-home-user/aee2471c-3739-50c5-a4c5-ffe157de8460/scratchpad/shots/p34
```

Delete the shots folder when you are done; disk is tight.

## Report

The term table, counts (keys changed in ko, in en, removed), the strings
you left with the reason, the layout problems you found and fixed or
could not, and the checks with their results.
