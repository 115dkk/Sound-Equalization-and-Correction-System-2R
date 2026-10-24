# P83 (Opus 5.5): audit #92 — screens for the Medium findings (U2b)

Worktree: `/home/user/wt-ui` (branch `wip/u2b`, created by Claude after
P82's commits). Work only there. Do not run cargo, do not push, do not spawn
sub-agents. Commit once per finding (subjects below); Claude reviews, re-runs
the gates and re-authors the commits.

Read first: `CLAUDE.md` (screen rules), `docs/DESIGN.md`, `docs/UI-TERMS.md`,
`docs/skins/README.md`, ADR 0010 (three skins, one DOM), `docs/ARCHITECTURE.md`
§7 (P55, P57), `docs/DEVIATIONS.md` F5, `TECH_DEBT_AUDIT.md` rows TD-65,
TD-67, TD-70, and P82's report. Load the `anthropic-skills:make-interfaces-feel-better`
skill before drawing. Tokens: `styles/tokens.css` and `styles/skins/*`;
components in `ui/`.

## Findings, in order

1. **TD-65** — project migration warnings (`ProjectInfo.migration_warnings`)
   have no consumer on screen. When a project opens with warnings (for
   example F5's rejected IR paths, P74's out-of-range outputs), show them in
   the project status area (`app/ProjectStatus.tsx`) as a persistent,
   dismissible notice listing each warning with its target (role/position or
   file) and, where the app can fix it, the action (e.g. re-measure the
   position) — otherwise just the state. Translate warning codes with keys
   `project.warning.<code>` (read the codes the service emits in
   `crates/secs-project/src/project.rs` / `record.rs`); add every key to
   `ko.json` and `en.json`. Test: a project with two warnings renders both;
   dismissing hides them until the next open. Commit:
   `fix(audit): A-60 프로젝트 이주 경고를 화면에 보입니다 (#92 TD-65)`
2. **TD-67** (screen part) — `screens/calibrate/ResultGraph.tsx:300-310`:
   on a failed target or XO save (P82's `targetEdit`/XO state), drop the
   local draft back to the service value and show the failed state next to
   the graph with a 다시 저장 action that resends the last draft; while
   saving, show the saving state. Test: failing save → draft reverted, state
   shown; retry sends the draft again. Commit:
   `fix(audit): A-62 타겟과 XO 저장이 실패하면 그래프를 되돌리고 다시 저장할 수 있게 합니다 (#92 TD-67)`
3. **TD-70** — `ui/plot/AxisRange.tsx:65`: Enter or 적용 with an invalid
   field (`abc`, out of range) closes the popover using the last valid
   values. The form must know each `NumberField`'s draft validity (the field
   already validates its draft; expose it to the form), refuse to submit,
   keep the popover open and move focus to the first invalid field. Test:
   type `abc` → Enter → popover still open, focus on that field, no
   `onApply`. Commit:
   `fix(audit): A-64 축 범위 입력이 틀리면 적용하지 않고 그 칸으로 갑니다 (#92 TD-70)`

## Gates (foreground; do not end the turn before they finish)

```sh
cd apps/secs2r/ui && PATH=/opt/node24/bin:$PATH npm run typecheck && PATH=/opt/node24/bin:$PATH npm test && PATH=/opt/node24/bin:$PATH npm run build
```

## Report (final answer)

Per finding: commit hash, files, test names, red failure, green result, new
keys with ko/en text. Then the last gate output tail and anything left
undone.
