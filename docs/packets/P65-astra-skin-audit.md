# P65 (ASTRA, read-only): differentiation audit of the three skins

Checkout: `/home/user/Sound-Equalization-and-Correction-System-2R`. You
have read-only tools; write nothing. Your whole output is the report, in
Korean, in the format below. Claude stores it as Superloopy evidence.

SECS 2R now has three skins (ADR 0010): the default Instrument skin
(`src/styles/tokens.css`, no `data-skin`), Hardware Rack
(`src/styles/skins/rack.css`) and Signal Matrix
(`src/styles/skins/matrix.css`). Their constitutions are in
`docs/skins/` (`README.md` with the shared articles, then `default.md`,
`rack.md`, `matrix.md`). The gate you audit is ADR 0010 §3: for each of
eight items, two skins that differ in palette alone fail.

Read first: `docs/adr/0010-skin-sets.md`, the four files in `docs/skins/`,
`docs/DESIGN.md` §2 and §8, then the stylesheets: `src/styles/tokens.css`,
`src/styles/skins/rack.css`, `src/styles/skins/matrix.css`, and for the
default's as-is form `src/ui/ui.css`, `src/app/app.css`,
`src/ui/plot/plot.css`. The rendered screenshots are under
`/tmp/claude-0/-home-user/ac5d27f7-127b-55fa-93b4-77862e05bb47/scratchpad/shots/`
(`p63-gallery/*.rack.png`, `*.matrix.png`, `*.light.ko.png`,
`*.dark.ko.png`; `p63-tour/*.dark.ko.rack.png`, `*.dark.ko.matrix.png`,
`*.dark.ko.png`): read the CSS first, then confirm with the pictures
where a claim depends on rendering.

## What you judge

1. **The eight gate items** (단계 레일, 주 동작, 카드 chrome, 상태 표기,
   hover와 focus, 비활성, 숫자 판독, 그래프 바탕). For each, name what
   the default does, what Rack does, what Matrix does, citing the CSS
   selectors, and give a verdict: 구조로 다름 / 팔레트만 다름 / 답 없음.
2. **"하지 말 것" of each constitution.** Search the skin CSS for
   violations (images or SVG assets, scale on press in Rack/Matrix, cold
   greys in Rack, radius or shadow or glow in Matrix, accent used
   decoratively, colour in type identification, hidden or reordered
   elements, `display: none`, `order`, font sizes under the floor, raw
   colours outside token blocks).
3. **Shared articles** (`docs/skins/README.md`): DOM untouched (the skins
   are CSS only; check `git diff --stat` for component files), 14 px
   floor, 40 px hit areas, visible focus, reduced-motion zero, status
   colours used only for status, tutorial accent kept out of the skins.
4. **Token discipline.** Dark blocks identical, light/dark colour parity,
   `--skin-*` naming, no `tokens.css` value changes beyond the audited
   default (compare with `git diff src/styles/tokens.css`).
5. **Anything that would embarrass the skin in front of the user**: a
   screen where Rack or Matrix is illegible, cut off, overlapping, or
   where a state (blocked step, stale card, running job, error) stopped
   reading as that state.

## Report format

```
# 스킨 차별화 감사 (P65)

## 판정 요약
HIGH: n건, MEDIUM: n건, LOW: n건. 차별화 게이트: 통과 / 실패(항목 번호).

## 차별화 게이트
| # | 항목 | Instrument | Rack | Matrix | 판정 |
(여덟 줄; 각 칸은 선택자를 든 구조 한 구절)

## 결함
### HIGH
- [파일:줄] 무엇이 어떤 조문과 어긋나는가, 재현(어느 견본·장면), 고칠 방향 한 줄
### MEDIUM
...
### LOW
...

## 확인한 것(결함 없음)
- ...
```

HIGH is a gate failure or a constitution's "하지 말 것" violation or an
illegible/broken state; MEDIUM is a weaker answer than the constitution
asks for; LOW is polish. Cite files and selectors; do not propose
rewrites longer than a line.
