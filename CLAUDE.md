# 저장소 작업 규약

바꾸기 전에 [`CONTEXT.md`](CONTEXT.md)의 용어와 불변식, [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md), 관련 [ADR](docs/adr)을 읽습니다. 업스트림과 다르게 동작하는 자리는 [`docs/DEVIATIONS.md`](docs/DEVIATIONS.md)에 있습니다.

## 지킬 것

- 업스트림 파이썬 트리(`src/`, `tests/`, `tools/`, `docs/STRUCTURE.md`, `배포/`, `run_secs.py`, `SECS.spec`, `build_exe.ps1`, `requirements*.txt`, `LICENSE`, `THIRD_PARTY_NOTICES.md`)는 고치지 않습니다. CI가 `UPSTREAM.toml`의 커밋과 비교합니다(ADR 0001). 업스트림 판의 README와 변경 기록은 `docs/upstream/`에 있습니다.
- 업스트림 기능은 모두 옮깁니다. 먼저 골든을 맞추고, 결함 수정은 그다음 PR에서 편차로 넣고 `docs/DEVIATIONS.md`를 고칩니다(ADR 0004). 핵심 아이디어를 바꾸는 변경은 ADR이 먼저입니다.
- unsafe는 격리 크레이트(`secs-sys` 예산 3, ADR 0011의 작업 표시줄 아이콘만; `secs-cuda-sys` 예산 80)에만 둘 수 있습니다. GPU의 좁은 네이티브 경계는 ADR 0008과 `docs/UNSAFE_BOUNDARIES.md`를 따릅니다. 그 밖의 새 크레이트 루트에는 `#![forbid(unsafe_code)]`를 걸고 Cargo.toml에 `[lints] workspace = true`를 둡니다. 숫자를 올리거나 격리 크레이트를 더하려면 ADR이 먼저입니다(ADR 0002).
- 운영체제 의존성(`wasapi`, `cpal`, `windows*`, `libc` 등)은 `secs-sys`만 가지고, `secs-sys`는 `apps/` 아래 앱만 의존합니다.
- GPU 로더 의존성은 `secs-cuda-sys`만 가지고, 이 격리 크레이트는 `secs-gpu`만 의존합니다(ADR 0008).
- 기능을 더하면 `features.toml`에 등록하고, `implemented`로 표시할 때는 실제로 있는 테스트를 가리킵니다. 화면이 부르는 IPC 메서드는 `ipc.<메서드>`로 등록합니다.
- IPC 타입은 `secs-types`에서만 정의합니다. 바꾼 뒤 `cargo test -p secs-types`로 `apps/secs2r/ui/src/ipc/bindings/`를 다시 생성해 함께 커밋합니다.
- 병렬은 map만 쓰고 부동소수점 누적은 원래 순서로 합니다. faer는 `std` 기능 없이, `Par::Seq`로 씁니다(ADR 0007).
- 화면은 make-interfaces-feel-better 규칙을 따릅니다. 문장은 합니다체, 상태와 라벨은 명사구, 효과를 말하는 라벨, 화면마다 채운 기본 동작 하나, UI 글자 14px 하한. 화면의 새 문구는 키를 정하고 `ko`와 `en` 카탈로그를 함께 고칩니다.
- 다른 저장소에서 코드를 가져오면 파일 머리에 출처와 라이선스를 적고 `docs/THIRD_PARTY.md`에 더합니다.
- crates.io 크레이트 16개(ADR 0016)는 자기 디렉터리 밖의 파일을 읽지 않습니다. `crates/secs-service/UPSTREAM.toml`, `crates/secs-mdat/data/template_1.mdat`, 크레이트마다의 `LICENSE`는 원본의 사본이므로, 업스트림을 병합해 원본이 바뀌면 사본도 함께 고쳐야 합니다(`secs-policy` publishing 게이트가 확인합니다). 라이브러리 판은 릴리스 게이트가 올리므로 손으로 고치지 않습니다.

## 작업 분배

설계, ADR, 작업서, 시그니처, 게이트, CI 설계, 디프 검토, 커밋과 PR은 Claude가 맡습니다. 구현은 작업서(`docs/packets/`) 단위로 워커에게 맡깁니다. 수치와 외부 규격이 드는 구현과 화면 연결, 번역은 ASTRA, 저장소·서비스·CLI·격리 크레이트·스크립트는 Daybreak Blue, 화면은 Opus 5.5입니다(`docs/ARCHITECTURE.md` 9절). 워커의 자기 보고는 게이트가 아닙니다. 아래 검사를 다시 돌린 뒤에 커밋합니다.

## 검사

커밋 전에 아래를 통과시킵니다. CI(`ci.yml`)가 같은 것을 Linux, Windows, macOS에서 돌립니다. Linux에서는 Tauri용 패키지(`libwebkit2gtk-4.1-dev` 등, `ci.yml`의 `LINUX_DEPS`)가 필요합니다.

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo clippy --workspace --exclude secs2r --exclude secs2r-host-shell --all-targets --locked --target x86_64-pc-windows-msvc -- -D warnings
cargo test --workspace --all-targets --locked
cargo deny --all-features check
python3 -m unittest discover -s .github/scripts -p "test_*.py"
python3 .github/scripts/upstream.py check-tree
python3 .github/scripts/check_feature_tests.py
(cd apps/secs2r/ui && npm ci && npm run typecheck && npm test && npm run build)
```

화면 의존성은 Node 24(npm 11)로 설치합니다. npm 10은 이 의존성 묶음에서 피어 해석이 실패합니다.

crates.io 크레이트의 매니페스트나 의존을 고치면 `cargo publish --workspace --dry-run --locked --allow-dirty`도 돌려야 합니다. 그 전에 `python3 .github/scripts/api_dry_run.py`로 공개 크레이트의 판을 `<판>-dryrun`으로 바꿔야 합니다. 이미 crates.io에 있는 판과 이름이 같고 내용이 다르면 Cargo가 검증을 거부하기 때문입니다. 이 스크립트는 작업 트리를 고치므로 버릴 사본(`git worktree add --detach`)에서 돌려야 합니다. 이때 `CARGO_HOME`은 매번 새 빈 폴더로 주어야 합니다. Cargo는 같은 이름과 판의 패키지를 `CARGO_HOME\registry\src`에 풀어 둔 것이 있으면 그것을 다시 쓰므로, 두 번째 게시 연습부터는 옛 소스로 검증합니다. CI는 매번 빈 러너라 이 문제가 없습니다.

신뢰할 수 없는 파일을 읽는 크레이트(`secs-jos`, `secs-mdat`, `secs-dirac`, `secs-project`)를 고치면 CI의 `miri`·`fuzz` 잡과 같은 것을 돌립니다. `secs-project`는 파일 시스템을 쓰지 않는 파서 테스트만 Miri로 돌립니다. nightly는 `ci.yml`의 `NIGHTLY`로 고정하고, fuzz 대상마다 넣는 seed는 `ci.yml`의 fuzz 잡에 있습니다(새 대상은 seed를 정해야 CI가 통과합니다).

```sh
cargo +nightly-2026-10-01 miri test --locked -p secs-jos -p secs-mdat -p secs-dirac
cargo +nightly-2026-10-01 miri test --locked -p secs-project --lib
cargo +nightly-2026-10-01 miri test --locked -p secs-project --test golden -- wav_reader truncated_wav_data
(cd fuzz && cargo +nightly-2026-10-01 fuzz run --target x86_64-unknown-linux-gnu wav_read -- -max_total_time=60)
```

## 브랜치와 릴리스

- 기본 브랜치는 `main`입니다. 별도 지시가 없으면 변경 작업의 기본 완료 범위는 브랜치 구현, 검증, PR 생성과 머지입니다.
- main에 머지하면 `release.yml`이 릴리스를 냅니다(ADR 0006). 머지가 곧 릴리스 결정입니다. 0.x는 모두 사전 출시이고 정식 출시는 1.0.0부터입니다.
- 커밋 제목은 `feat:`, `fix:`, `docs:`, `ci:`, `chore:`, `refactor:`, `test:` 같은 접두어 뒤에 한국어로 씁니다. 출하 파일을 건드린 `feat`는 다음 릴리스에서 minor를, 나머지는 patch를 올립니다.
- 출하물을 바꾸는 PR은 `CHANGELOG.md` 맨 위의 다음 버전 절에 항목을 더합니다. 릴리스 본문이 그 절입니다.
- push한 뒤에는 PR의 CI를 끝까지 따라가고, 실패하면 같은 PR에 수정 커밋을 올립니다.
- 유지보수자의 상시 승인에 따라, 에이전트가 연 PR은 CI가 모두 통과하고 충돌과 열린 리뷰 스레드가 없으면 사람의 리뷰를 기다리지 않고 머지 커밋으로 머지합니다. 머지한 뒤에는 `release.yml`의 실행(게이트의 판정, bump 커밋, 세 운영체제 설치본, 릴리스 자산)까지 확인합니다.
- 화면이나 카탈로그를 바꾼 PR에서는 `ui-gallery.yml`이 리뷰 이슈(`화면 갤러리 리뷰: PR #N`)를 열어 화면 전체와 견본을 보여 줍니다. 머지를 막지 않지만, 그 링크를 사용자에게 알려 화면을 따로 볼 수 있게 합니다.
- 업스트림에 새 커밋이 생기면 `upstream-watch.yml`이 이슈를 엽니다. 병합할 때 `README.md`·`CHANGELOG.md` 충돌은 우리 쪽을 남기고 업스트림 판을 `docs/upstream/`에 옮깁니다.
