# P73 (Daybreak Blue): audit #92 — CI helper scripts and oracle launchers (G1)

Worktree: `/home/user/wt-g1` (branch `wip/g1`, created by Claude from the
same base). Work only there. Do not push, do not spawn sub-agents, do not run
cargo (another worker is building in the main tree). Commit once per finding
(subjects below); Claude reviews, re-runs the gates and re-authors the
commits.

Read first: `CLAUDE.md`, `TECH_DEBT_AUDIT.md` (rows TD-26, TD-59, TD-62,
TD-63, TD-64, TD-81), `docs/adr/0006-releases.md`, `.github/workflows/release.yml`
(how `release_gate.py` is called), `.github/scripts/{release_gate.py,check_feature_tests.py,upstream.py}`
and their `test_*.py`, `oracle/{run.py,common.py,export_engine_mimo_iir.py,export_engine_xo.py,README.md}`,
`docs/ENGINE.md`.

Method: test first. For the CI scripts the seam is the script's functions
under `python3 -m unittest discover -s .github/scripts -p "test_*.py"`, with
`subprocess`/git calls replaced by the fakes those tests already use. Red
(confirm the failure is the finding's), minimum green, full script suite.

## Findings, in order

### 1. TD-26 — release gate: push and fetch failures are not "main moved"

`release_gate.py:257` treats every failed bump push as "main moved on" and
returns `None` (→ `should_release=false`, exit 0); `:271` ignores the return
codes of the tag and main fetches and reads a possibly stale `FETCH_HEAD`.

Design (fixed): after a failed push, fetch `origin main` again (checking the
return code) and compare its head with the commit the gate started from: if
main advanced, keep today's skip; otherwise print the push's stderr and exit
non-zero. Every fetch must succeed (non-zero → print stderr, exit non-zero);
read the remote head by name (`git rev-parse origin/main` after
`git fetch origin main:refs/remotes/origin/main` or the equivalent the script
already uses), never `FETCH_HEAD` left by an earlier fetch. Update the test
that currently pins the tolerated failure (`test_release_gate.py:145-146`).
Tests: push fails + main unchanged → exit non-zero; push fails + main
advanced → skip; fetch fails → exit non-zero.

Commit: `fix(audit): A-26 릴리스 게이트가 push·fetch 실패를 건너뜀으로 바꾸지 않습니다 (#92 TD-26)`

### 2. TD-59 — feature-test gate sees ignored tests

`check_feature_tests.py:53` checks `cargo test -- --list` output only, so an
`#[ignore]` test satisfies an `implemented` feature. Fix: also collect
`-- --list --ignored`; a feature whose evidence is only ignored tests fails,
unless the test is in an explicit allow list in the script that names the CI
job running it with `--ignored` (today: the Equalizer APO playback tests run
by `ci.yml`'s job — read `ci.yml` to name it). Tests with fake listings.

Commit: `fix(audit): A-56 기능 테스트 게이트가 ignored 테스트를 구별합니다 (#92 TD-59)`

### 3. TD-62 — upstream identity check fails on git errors

`upstream.py:181` ignores the `git diff` exit code and treats empty stdout as
"matches". Fix: non-zero exit → print stderr, fail the gate. Tests: match,
difference, command failure.

Commit: `fix(audit): A-57 업스트림 동일성 검사가 git 실패를 일치로 보지 않습니다 (#92 TD-62)`

### 4. TD-63 — `oracle/run.py engine_mimo_iir` works

`run.py:58` passes no arguments; `export_engine_mimo_iir.py:310` reads
`sys.argv[1]`. Fix in the exporter: default the scratch directory when no
argument is given (a subdirectory of the system temp dir named after the
family), keep the explicit argument working; document in `oracle/README.md`.
Verify by creating a venv (`python3 -m venv /tmp/oracle-venv && /tmp/oracle-venv/bin/pip install -r oracle/requirements.txt`)
and running `python -c "import ast,sys; ast.parse(open('oracle/export_engine_mimo_iir.py').read())"`
plus a dry argument-parsing check; do not regenerate goldens (they need the
pinned upstream environment and take long).

Commit: `fix(audit): A-58 오라클 런처로 engine_mimo_iir를 돌릴 수 있게 합니다 (#92 TD-63)`

### 5. TD-64 — XO resume cache carries a fingerprint

`export_engine_xo.py:92` reuses `label.json` whenever it exists. Fix: write a
fingerprint next to it (upstream commit from `UPSTREAM.toml`, SHA-256 of the
exporter file, the case name, `numpy.__version__`, `scipy.__version__`) and
reuse only when it matches; otherwise recompute (or refuse with a clear
message if recomputation is not possible in that code path). Same
verification approach as finding 4.

Commit: `fix(audit): A-59 XO 재개 캐시가 지문이 맞을 때만 다시 씁니다 (#92 TD-64)`

### 6. TD-81 — remove the unused oracle recorder (refactor)

`oracle/common.py:201`: `Recorder`, global `REC`, `interpose` are unused.
Delete them and make `docs/ENGINE.md:35` describe the observation the
exporters actually use (read two exporters to describe it in one sentence).
No new tests (pure refactor); `python3 -c "import ast; ..."` parse checks for
every `oracle/*.py` that imported anything you removed.

Commit: `refactor(audit): B-5 쓰이지 않는 오라클 Recorder를 지웁니다 (#92 TD-81)`

## Allowed files

`.github/scripts/**`, `oracle/*.py`, `oracle/README.md`, `docs/ENGINE.md`.
Nothing else (in particular not `.github/workflows/**`, not `oracle/goldens/`,
not the upstream Python tree).

## Gates (foreground; do not end the turn before they finish)

```sh
python3 -m unittest discover -s .github/scripts -p "test_*.py"
python3 .github/scripts/upstream.py check-tree
```

## Report (final answer)

Per finding: commit hash, files, test names, red failure message, green
result. Then the last gate output, anything left undone.
