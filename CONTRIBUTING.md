# 기여 안내

풀 리퀘스트와 이슈를 받습니다. 한국어와 영어 모두 됩니다.

## 이슈

- 버그는 **버그 신고** 양식으로 올려 주세요. 운영체제, SECS 2R 버전(앱의 정보 또는 `secs2r-cli bootstrap`), 재현 순서가 있으면 고치기 쉽습니다. 프로젝트 파일(`.secs`와 `.irs/` 폴더)이나 mdat를 붙일 수 있으면 함께 붙여 주세요.
- 기능 제안은 **기능 제안** 양식으로 올려 주세요.
- 업스트림 SECS 2에서 동작이 다른 것을 발견했다면, 업스트림 판과 SECS 2R 판을 함께 적어 주세요. 의도한 차이는 [`docs/DEVIATIONS.md`](docs/DEVIATIONS.md)에 있습니다.
- 보안 문제는 공개 이슈 대신 [`SECURITY.md`](SECURITY.md)의 방법으로 알려 주세요.

## 풀 리퀘스트

1. 바꾸기 전에 [`CONTEXT.md`](CONTEXT.md), [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md), 관련 [ADR](docs/adr)을 읽어 주세요. 구조를 바꾸는 제안은 ADR 초안과 함께 올리면 검토가 빠릅니다.
2. 업스트림 파이썬 트리(`src/`, `tests/`, `tools/` 등, ADR 0001)는 고치지 않습니다. CI가 확인합니다.
3. 새 기능은 `features.toml`에 등록하고 테스트를 함께 넣어 주세요. '구현됨' 표시에는 실제로 있는 테스트가 있어야 합니다.
4. unsafe 코드는 받지 않습니다. 꼭 필요하면 이슈에서 먼저 의논합니다(ADR 0002).
5. 업스트림과 결과가 달라지는 수정은 `docs/DEVIATIONS.md`에 항목을 더하고, 골든 방에서 어느 채널도 성적표 지표가 나빠지지 않는다는 것을 보여 주세요(ADR 0004).
6. 화면 문구는 키를 정하고 `apps/secs2r/ui/src/i18n/`의 한국어와 영어 카탈로그를 함께 고쳐 주세요.
7. 커밋 제목은 `feat:`, `fix:`, `docs:`, `ci:`, `chore:`, `refactor:`, `test:` 같은 접두어로 시작합니다. 출하물을 바꾸면 `CHANGELOG.md` 맨 위 절에 항목을 더해 주세요.

## 검사

PR을 올리기 전에 아래가 통과하는지 확인해 주세요. 같은 검사를 CI가 세 운영체제에서 돌립니다.

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
python3 -m unittest discover -s .github/scripts -p "test_*.py"
(cd apps/secs2r/ui && npm ci && npm run typecheck && npm test)
```

Linux에서는 Tauri용 패키지가 필요합니다(README의 '소스에서 빌드'). 화면 의존성은 Node 24로 설치해 주세요.

## 라이선스

기여한 코드는 이 저장소의 라이선스로 배포됩니다. 앱과 게시하지 않는 크레이트는 GPL-3.0-only, crates.io에 올리는 크레이트 16개와 PyPI 패키지는 AGPL-3.0-only입니다(ADR 0023). 다른 곳에서 가져온 코드를 넣을 때는 출처와 라이선스를 파일 머리와 `docs/THIRD_PARTY.md`에 적어 주세요.
