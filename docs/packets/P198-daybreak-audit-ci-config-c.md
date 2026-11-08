# P198 (Daybreak): audit #186, Category C CI and configuration findings

Phase 3 of `/115dkk-skills:resolve-audit` for issue #186. It runs AFTER
Phases 1 and 2 (P183..P192) have been rebased into `fix/audit-186`. Start
from the then-current `fix/audit-186`: Claude re-creates the worktree
`E:\wt\audit-host` on a fresh branch `audit/186-ci-c` from it before
handing you this packet. Work only there. Commit on that branch, one commit
per finding, in the order below. Do not push, do not rebase, do not spawn
sub-agents, do not touch `audit-state.md`.

Category C means: no runtime behaviour change. The gates are the script
unit tests, YAML parsing, and `cargo metadata`. Every workflow edit makes
the PR's CI run every job (`ci.yml` rule in `CLAUDE.md`), which is expected.

Allowed files: `.github/workflows/*.yml`, `.github/scripts/*.py` (only
`manage_e2e.py`, `release_gate.py`, `check_feature_tests.py`,
`test_check_feature_tests.py`, `test_release_gate.py` if it exists),
`.gitignore`, `apps/secs2r/Cargo.toml`, `crates/secs-jos/Cargo.toml`,
`Cargo.lock` only if a pin changes it, `features.toml` (the note of
`app.updater` for C-25). Forbidden: everything else, in particular any
`src/` file, `CHANGELOG.md`, `docs/**`.

Read first: `CLAUDE.md` (검사, 브랜치와 릴리스), the header comments of
`.github/workflows/{ci,release,monthly-audit,upstream-watch,ui-gallery}.yml`,
`.github/scripts/release_gate.py` lines 60-110 and 300-330,
`.github/scripts/check_feature_tests.py`, `.github/scripts/test_check_feature_tests.py`,
and the finding texts quoted below.

## The protocol for every item

1. Announce the item in the report (`── C-n: … ──`).
2. Make the change.
3. Gate (foreground): `python3 -m unittest discover -s .github/scripts -p "test_*.py"`
   after any script change; for every workflow you touched,
   `python3 -c "import yaml,sys; yaml.safe_load(open(sys.argv[1], encoding='utf-8'))" <file>`;
   `cargo metadata --locked --format-version 1 > /dev/null` after a manifest
   change (and `cargo build -p secs2r --locked` with
   `CARGO_TARGET_DIR=C:\cargo-target\audit-host` if `Cargo.lock` changed).
4. Commit with `git add <the files you touched>` and this message shape
   (Korean, 하십시오체, no em-dash, no program-as-subject verbs such as
   묻다/적다/넘기다, "추가하다" not "더하다"):

   ```
   chore(audit): C-n <한국어 한 문장> (#186 TD-xxx)

   <무엇이 어떻게 맞춰지는지 한두 문장. 동작은 바뀌지 않는다는 문장.>

   Co-Authored-By: Claude Mythos 5.1 <noreply@anthropic.com>
   ```

## Items

### C-17 · TD-212 · dependency pins that drift

Finding: "Tauri 플러그인 셋은 주석과 함께 `=`로 고정했는데 `tauri`는
caret(`2.12`)이라 잠금 갱신이 코어만 옮길 수 있습니다. maturin-action이
`rust-toolchain: "1.99.0"`을 글자로 되풀이해 `rust-toolchain.toml`을 올려도
휠은 옛 컴파일러로 남을 수 있습니다. 월간 감사는 `npm install -g
@anthropic-ai/claude-code`를 판 없이 설치하고 … `secs-jos`만 `secs-oracle =
{ path = … }`로 적고 나머지 셋은 `workspace = true`입니다."

Sites: `apps/secs2r/Cargo.toml:29` (`tauri = { version = "2.12", features = [] }`
beside three `=`-pinned plugins), `.github/workflows/ci.yml:309,319` and
`.github/workflows/release.yml` (the two `rust-toolchain: "1.99.0"` lines
of each `maturin-action` use), `.github/workflows/monthly-audit.yml:278`
(`npm install -g @anthropic-ai/claude-code`) and its header comment at
line 48 (the plugin is deliberately unpinned), `crates/secs-jos/Cargo.toml:49`
(`secs-oracle = { path = "../secs-oracle" }`), `rust-toolchain.toml`
(`channel = "1.99.0"`).

Change, four parts in one commit:

1. `tauri = { version = "=2.12.<patch in Cargo.lock>", features = [] }`
   with the same two-line comment style the plugins carry (read the exact
   patch from `Cargo.lock`'s `name = "tauri"` entry; the lock must not
   change).
2. The maturin steps read the toolchain from the file: add a small step
   before them in both workflows,
   `- id: toolchain\n  run: echo "channel=$(python3 -c "import tomllib;print(tomllib.load(open('rust-toolchain.toml','rb'))['toolchain']['channel'])")" >> "$GITHUB_OUTPUT"`
   (`shell: bash` on the Windows and macOS legs), and use
   `rust-toolchain: ${{ steps.toolchain.outputs.channel }}` in every
   `maturin-action` `with:` block. Keep the pinned action SHAs.
3. In `monthly-audit.yml`, pin the CLI: `npm install -g @anthropic-ai/claude-code@<version>`
   where `<version>` is the one the last successful audit run installed
   (read it from run 42187728915's "Install Claude Code" step log with
   `gh run view --job <id> --log`; it prints the installed version), and
   add two header lines saying the CLI is pinned while the plugin is not,
   and that the pin moves by a `ci:` commit.
4. `crates/secs-jos/Cargo.toml`: `secs-oracle.workspace = true` under
   `[dev-dependencies]` like `secs-mdat` and `secs-project`.

Gate: the YAML parse of three workflows, `cargo metadata --locked`,
`git diff --stat Cargo.lock` must be empty.

Commit subject: `chore(audit): C-17 tauri와 maturin 툴체인, 감사 CLI, secs-jos의 고정 방식을 맞춥니다 (#186 TD-212)`.

### C-25 · TD-239 · the registry check runs on Linux only

Finding: "`check_feature_tests.py`가 Linux에서만 돌아 `cfg(windows)` 시험
(파이프 왕복 `host.rs:1181-1217`, 링크 거부 `rules.rs:1034-1083`)을 기능
근거로 적을 수 없고 … `IGNORED_CI_TESTS`의 잡 이름은 `ci.yml`과 비교하는
곳이 없어 잡 이름이 바뀌어도 유효해 보입니다."

Sites: `.github/workflows/ci.yml:249-251` (`if: runner.os == 'Linux'`),
`.github/scripts/check_feature_tests.py:43-49` (`IGNORED_CI_TESTS` with job
names `eqapo-playback`, `reaper-playback`, `camilla-playback`),
`.github/scripts/test_check_feature_tests.py`, `features.toml:18` (the
`app.updater` note saying the Linux check cannot list the two Windows
tests).

Change:

1. `check_feature_tests.py` gains `--allow-missing-on <os>` semantics the
   simple way: a test that Cargo does not list on this OS is accepted when
   it is listed in a sidecar file `.github/feature-tests.<os>.txt`?
   No: keep it stateless. Instead the script accepts an argument
   `--union-with <json>` where a previous leg's listing is merged. CI does:
   on every `cargo test` matrix leg, run the script with
   `--emit-listing "$RUNNER_TEMP/feature-tests-${{ runner.os }}.json"`
   (writes the Cargo listing, exits 0 without judging), upload it as an
   artifact `feature-tests-<os>`; a new small job `feature registry
   (all OSes)` with `needs: test` downloads the three artifacts and runs
   `check_feature_tests.py --union <three files>` which judges the registry
   against the union of the three listings. Add the new job's name to the
   list of required checks in the repository ruleset later (report it; a
   ruleset change is the maintainer's, not yours). Keep the existing Linux
   step until the new job is green in the PR, then remove it in the same
   commit series? No: one commit, replace the step with the artifact
   emission on all three legs and the union job; the PR's CI proves it.
2. `test_check_feature_tests.py`: a test `ignored_ci_jobs_exist_in_ci_yml`
   that parses `.github/workflows/ci.yml` with `yaml` and asserts every
   job name in `IGNORED_CI_TESTS.values()` is a key of `jobs`; a test for
   the union logic (two listings, a name present in one only is accepted).
3. `features.toml` `app.updater` note: replace "that the Linux registry
   check cannot list" with the fact that the Windows leg now lists them,
   and add the two test names to `tests = [...]` so the registry pins them.

Gate: the script unit tests, YAML parse of `ci.yml`.

Commit subject: `chore(audit): C-25 기능 등록부 검사가 세 운영체제의 시험 목록을 합쳐 판정합니다 (#186 TD-239)`.

### C-26 · TD-242 · fixed 20 s waits and a tauri-driver source build per run

Finding: "`dismiss_offer`가 대화상자가 안 뜨면(설정 뒤 보통의 경우) 고정
20 s를 기다리고 실행마다 두 번(3단계와 9단계) 치릅니다 … `cargo install
tauri-driver --locked --version 2.1.0`을 Windows 잡 셋에서 매번 소스에서
컴파일합니다(`rust-cache`는 `~/.cargo/bin`을 보관하지 않음)."

Sites: `.github/scripts/manage_e2e.py:878-897` (`dismiss_offer`),
`.github/workflows/ci.yml` (three `cargo install tauri-driver --locked --version 2.1.0`
sites near lines 847 and 930 and the third one; find them with
`grep -n "cargo install tauri-driver" .github/workflows/*.yml`),
`.github/workflows/release.yml:329` (the update-e2e leg).

Change:

1. `dismiss_offer(self, timeout=20)`: wait for whichever comes first, the
   offer dialog (`[data-ui="project.new.cancel"]`) or the frame's menu
   being clickable without a backdrop (`document.querySelector('.ui-dialog-backdrop') === null && document.querySelector('[data-ui="frame.project"]') !== null`),
   using one `wait_for` on a combined predicate that returns which one it
   saw; cancel only when the dialog was seen. Keep the log line. The
   method's callers do not change. Add a unit test in the scripts' test
   module if `manage_e2e.py` has pure helpers under test (search
   `test_manage_e2e.py`); if the predicate is inline JavaScript, test the
   Python branch with a fake driver that answers "menu" first.
2. tauri-driver: replace the three `cargo install` steps with
   `taiki-e/install-action@1e29709c0ec63babbe8f266397a465f29a825f91 # v2.87.31`
   (the SHA `ci.yml` already pins for cargo-fuzz) and `tool: tauri-driver@2.1.0`
   if that action's manifest knows `tauri-driver` (check
   https://github.com/taiki-e/install-action/blob/main/TOOLS.md with
   WebFetch); if it does not, cache the built binary instead:
   `actions/cache` keyed on `tauri-driver-2.1.0-${{ runner.os }}` over
   `~/.cargo/bin/tauri-driver.exe`, and run `cargo install` only on a cache
   miss (`if: steps.cache.outputs.cache-hit != 'true'`). Use the same SHA
   `actions/cache` the repository already pins (grep `actions/cache@`); if
   none, pin the current v4 SHA you fetch from the GitHub API and write the
   `# vX.Y.Z` comment like the other actions.

Gate: script unit tests; YAML parse of `ci.yml` and `release.yml`.

Commit subject: `chore(audit): C-26 e2e의 고정 대기를 조건 대기로, tauri-driver를 캐시로 바꿉니다 (#186 TD-242)`.

### C-27 · TD-244 · the host shell's generated schemas

Finding: "`apps/secs2r/gen/schemas/`만 무시하고 셸은 스키마 넷을 커밋했는데,
Linux 빌드가 `linux-schema.json`을 만들어 untracked로 남습니다."

Site: `.gitignore:17` (`apps/secs2r/gen/schemas/`), `apps/secs2r-host-shell/gen/schemas/`
(committed `acl-manifests.json`, `capabilities.json`, `desktop-schema.json`,
`windows-schema.json`).

Change: ignore the shell's folder like the app's: add
`apps/secs2r-host-shell/gen/schemas/` next to line 17 and
`git rm --cached` the four committed files (they are regenerated by every
`cargo build` of the shell; nothing reads them from the repository: confirm
with `git grep -n "gen/schemas"` and say what you found). If something does
read them, commit `linux-schema.json` instead and say so.

Gate: `git status --short apps/secs2r-host-shell/gen` is empty after a
`cargo check -p secs2r-host-shell` (Windows; the Linux file cannot be
produced here, which is fine: the ignore covers it).

Commit subject: `chore(audit): C-27 호스트 셸의 생성 스키마를 앱처럼 무시합니다 (#186 TD-244)`.

### C-28 · TD-245 · `|| true` hides API failures; packaged READMEs wait for an app release

Finding: "`gh issue list ... || true`가 API 실패를 '열린 이슈 없음'으로
바꿔 다음 단계가 이식 이슈를 중복으로 엽니다. `release_gate.py`의 주석은
패키지된 크레이트 파일이 모두 출하 파일이라 하지만 `*.md`는 `NOT_SHIPPED`이면서
`api_files`는 크레이트 `README.md`를 세므로, 게시 크레이트의 README만 바꾼
변경은 다음 관계없는 앱 릴리스까지 기다립니다(`api_action`은 `action != "none"`일
때만 계산)."

Sites: `.github/workflows/upstream-watch.yml:41`, `.github/scripts/release_gate.py:77`
(`NOT_SHIPPED` starts with `"*.md"`), `:309-319` (`api_files`), `:525-532`
(`if action != "none":`).

Change:

1. `upstream-watch.yml`: drop `|| true`; the step already runs under
   `set -euo pipefail`, so a failing `gh issue list` fails the step (the
   duplicate issue is worse than a red run). Keep the `exit 0` for the
   found case.
2. `release_gate.py`: count a published crate's packaged `README.md` as
   shipped by adding the pattern to `SHIP_ALWAYS` (`"crates/*/README.md"`
   and the bindings' READMEs that `BINDINGS` packages, if `api_files`
   counts them) so a README-only change to a published crate triggers the
   library bump on its own; update the docstring near `NOT_SHIPPED` to say
   why. Add a unit test in `test_release_gate.py` (it exists if
   `.github/scripts/test_release_gate.py` is present; else create it in
   the style of the other `test_*.py`): a change set with only
   `crates/secs-types/README.md` makes `decide` return a library action
   and an app action of `none`. If the gate's structure makes that
   impossible without changing release behaviour beyond the finding, do
   the docstring-only alternative the finding allows and say so.

Gate: `python3 -m unittest discover -s .github/scripts -p "test_*.py"`,
YAML parse of `upstream-watch.yml`.

Commit subject: `chore(audit): C-28 업스트림 감시가 API 실패를 숨기지 않고 게시 크레이트의 README를 출하로 셉니다 (#186 TD-245)`.

### C-30 · TD-247 · paths-filter permissions declared on two of four jobs

Finding: "`dotnet-changes`와 `scope`는 `pull-requests: read`를 선언하는데
`python-changes`와 갤러리 `changes` 잡은 선언하지 않습니다 … 관례의
갈림입니다."

Sites: `.github/workflows/ci.yml` jobs `scope` (line ~99, declares),
`python-changes` (line ~255, does not), `dotnet-changes` (line ~378,
declares); `.github/workflows/ui-gallery.yml` job `changes` (line ~47,
does not).

Change: declare on all four, the same two lines and the same comment the
`dotnet-changes` job carries (`# paths-filter lists the pull request's
files, as the scope job does.` with `permissions: contents: read,
pull-requests: read`).

Gate: YAML parse of both workflows.

Commit subject: `chore(audit): C-30 paths-filter 잡 넷이 같은 권한을 선언합니다 (#186 TD-247)`.

## Covered elsewhere (do not touch)

- C-5 (TD-174 hand-copied UI constants): ADR 0052 / P190 and P192.
- C-13, C-21, C-22, C-23, C-24 (UI comments, tsconfig flags, specimens,
  effect typing, CSP reasons): P192.
- C-1, C-2, C-8, C-9, C-10, C-12, C-14, C-18, C-19, C-20, C-32 (tests and
  fuzz): P197.
- C-3, C-4, C-6, C-7, C-11, C-15, C-16, C-29, C-31 (documents): P196.

## Verification (foreground; do not end the turn before they finish)

```
python3 -m unittest discover -s .github/scripts -p "test_*.py"
for f in .github/workflows/*.yml; do python3 -c "import yaml,sys; yaml.safe_load(open(sys.argv[1], encoding='utf-8'))" "$f" || exit 1; done
cargo metadata --locked --format-version 1 > /dev/null
git diff --stat fix/audit-186..audit/186-ci-c -- Cargo.lock
git log --oneline fix/audit-186..audit/186-ci-c
```

## Report

Per item: the commit hash, the files changed, the gate output tail. Then:
anything you could not decide (the ruleset's required-check name for C-25,
the install-action support for tauri-driver in C-26), anything narrowed and
why, and the final `git log --oneline` of the branch.
