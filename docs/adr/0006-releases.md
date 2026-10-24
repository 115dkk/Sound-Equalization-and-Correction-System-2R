# 0006. main에 push할 때마다 릴리스하고, 0.x는 사전 출시, 정식 출시는 1.0.0부터

상태: 채택, 2026-10-13

## 배경

릴리스를 손으로 내면 받아 볼 빌드가 늦고, 잊으면 확인할 빌드가 없습니다. 반면 등가와 화면이 끝나기 전의 빌드가 '정식'으로 보여서는 안 됩니다. 업스트림은 Windows용 폴더째 실행하는 zip만 냅니다.

## 결정

- 버전은 꼬리표 없는 `v<major>.<minor>.<patch>`입니다. major가 0인 동안의 모든 릴리스는 GitHub 사전 출시(prerelease)이고 'latest'를 차지하지 않습니다. 정식 출시는 1.0.0부터이고, 1.0.0은 손으로 올립니다.
- `release.yml`은 main에 push할 때마다 돕니다. 게이트(`.github/scripts/release_gate.py`)는 다음처럼 정합니다.
  - 워크스페이스 버전에 태그가 없으면 그 버전 그대로 릴리스합니다(첫 릴리스, 또는 손으로 올린 버전).
  - 태그가 있고 그 뒤로 출하 파일이 바뀌었으면 버전을 올립니다. 출하 파일을 건드린 커밋 중에 `feat` 제목이나 `BREAKING CHANGE`가 있으면 minor, 아니면 patch입니다. 1.0 전에는 호환을 깨는 변경도 minor로 올립니다.
  - 출하 파일이 바뀌지 않았으면 릴리스하지 않습니다.
- 출하 파일은 `crates/`, `apps/`, `Cargo.toml`, `Cargo.lock`, `rust-toolchain.toml`, `LICENSE`, `UPSTREAM.toml`에서 테스트, 벤치, 퍼즈, 문서, 화면 단위 테스트, `secs-policy`를 뺀 것입니다. 업스트림 파이썬 트리는 출하하지 않습니다.
- 버전을 올릴 때 게이트가 `Cargo.toml`과 `Cargo.lock`의 워크스페이스 버전을 고쳐 `chore(release): v0.x.y`로 main에 커밋합니다. 이 push는 저장소 비밀 `RELEASE_DEPLOY_KEY`에 든 쓰기 배포 키로 합니다. main에는 PR 검사를 모두 필수로 거는 'Required checks' 룰셋이 있고, 검사 결과가 없는 bump 커밋은 배포 키 우회로만 들어갈 수 있습니다(GitHub은 개인 저장소 룰셋에 GitHub Actions를 우회 주체로 받지 않습니다). 배포 키 push는 워크플로를 시작하므로 커밋 본문에 `[skip ci]`를 넣어 이 커밋이 release.yml과 ci.yml을 다시 부르지 않게 합니다. 앱의 버전은 `tauri.conf.json`이 아니라 크레이트 버전을 따르므로 두 파일만 고치면 됩니다.
- 릴리스 자산의 이름은 인터페이스이므로 고정합니다(`collect_bundles.py`).
  - Windows: `SECS2R-<버전>-windows-x64-setup.exe`(NSIS, 사용자 단위 설치), `SECS2R-<버전>-windows-x64-portable.zip`(설치 없이 폴더째 실행)
  - macOS: `SECS2R-<버전>-macos-universal.dmg`
  - Linux: `SECS2R-<버전>-linux-x86_64.AppImage`, `SECS2R-<버전>-linux-amd64.deb`(오래된 glibc와 맞도록 Ubuntu 22.04에서 빌드)
  - 모든 자산의 `SHA256SUMS.txt`
- Windows 릴리스 exe는 GUI 서브시스템이어야 합니다(콘솔 창 없음). 빌드 잡이 PE 헤더로 확인합니다.
- 빌드 잡은 패키징 전에 release 모드로 전체 테스트를 돌립니다. 릴리스 잡은 캐시를 쓰지 않고, 손으로 실행하는 트리거도 두지 않습니다. 둘 다 다른 브랜치의 코드가 main의 신뢰를 얻는 경로가 되기 때문입니다(CodeQL actions/cache-poisoning). 실패한 릴리스는 그 실행을 다시 돌립니다.
- 릴리스 본문은 `CHANGELOG.md`의 해당 버전 절이고, 절이 없으면 커밋 제목을 씁니다.
- 자동 업데이트와 코드 서명은 M6에서 정합니다. 그전 판은 업데이트를 스스로 하지 않습니다.

## 결과

- 머지가 곧 릴리스 결정입니다. 문서나 CI만 바꾼 머지는 릴리스를 만들지 않습니다.
- main에 보호 규칙을 걸어 봇의 push를 막으면 버전 올리기가 실패합니다. 그때는 github-actions에 우회를 허락하거나 이 ADR을 바꿉니다.
- 작업 브랜치는 머지 전에 main을 받아야 합니다. 버전 줄이 바뀌어 있을 수 있습니다.
- PR마다 CI의 `bundle` 잡이 같은 설치본을 만들어 실행 아티팩트로 남기므로, 릴리스 전에 설치해 볼 수 있습니다.
