# Sol PR 리뷰

`.github/workflows/sol-review.yml`은 기본 활성화입니다. 공식 Codex GitHub 앱이나 Codex Action이
아닌 **Responses API + GitHub Actions 봇**이며 작성 주체는 `github-actions[bot]`입니다.
기본 모델은 `gpt-6.1-sol`, reasoning effort는 `high`입니다. 병합 코드는 없습니다.

## 판정과 실제 게시

- 완료된 리뷰는 막는 지적이 없으면 `APPROVE`입니다. 지적이 없으면 그렇다고 적고, 의견이 있으면 본문에 모두 적어 함께 Approve합니다.
- `REQUEST_CHANGES`는 좁은 경우에만 냅니다. 지적마다 분류(`security`, `data_loss`, `build`, `invariant`, `behavior`, `other`)가 있고,
  `blocking`이면서 분류가 `security`, `data_loss`, `build`, `invariant` 가운데 하나이고 이 PR이 추가하거나 바꾼 줄을 가리킬 때만 막습니다.
  그 밖의 `blocking`은 '병합 차단 범위 밖'이라고 적어 의견으로 남깁니다. 이 규칙은 정책 문장뿐 아니라 게시 코드가 강제합니다.
- 부분 리뷰, 출력 누락, 문맥 누락/잘림, 시간 초과, API 오류, 잘못된 스키마: 이유를 설명하는 `COMMENT`.
  검토 인프라 문제는 코드 결함이나 병합 차단 요청으로 바꾸지 않고, 끝나지 않은 리뷰는 Approve하지 않습니다.
- SHA 변경: 이전 결함 결과를 적용하지 않고 최신 커밋 재검토가 필요하다는 `COMMENT`.
- API 크레딧 고갈: OpenAI가 `insufficient_quota`, `billing_hard_limit_reached`, `billing_not_active`로 거부하면
  '[API 고갈]'로 시작하는 한 줄 `COMMENT`만 남깁니다. 오류 본문에서는 이 코드만 읽고, 본문 자체는 기록하지 않습니다.
- GitHub Actions의 PR 승인은 저장소 설정(Settings → Actions → General → Workflow permissions의
  'Allow GitHub Actions to create and approve pull requests')이 켜져 있어야 합니다. 꺼져 있으면 GitHub가 422로 거부하고,
  봇은 같은 본문에 그 사실을 덧붙여 `COMMENT`로 한 번 남깁니다. 다른 오류는 숨기지 않습니다.

APPROVE/COMMENT/REQUEST_CHANGES 제출은 기본 켜짐입니다. `SOL_REVIEW_PUBLISH=false`는 명시적인 dry-run 옵션입니다.
준비/API 단계 실패 후에도 게시 잡이 실행되고, 누락된 결과 아티팩트도 미완료 안내로 처리합니다.
GitHub API 접근이나 게시 잡 자체가 실패하면 안내조차 게시되지 않을 수 있습니다. 이때 실행 로그에
게시 미확인 상태를 남기며 RC/Approval로 바꾸지 않습니다. 닫히거나 draft로 바뀐 PR에는 게시하지 않습니다.

## 신뢰 경계

기본 브랜치의 정확한 base SHA에서 실행 코드와 정책을 가져옵니다. PR 코드·설치 스크립트·AGENTS
지시는 실행하지 않습니다. PR diff, base/head 전체 변경 파일, 주변 Cargo.toml/package.json을
데이터로만 보냅니다. 모델에는 tools를 주지 않으며 자율 실행 루프가 없습니다.
fork/draft PR은 대상에서 제외합니다. 최대 100개 파일, 추가 문맥 조회 40개, 정책 포함 입력 300 KB입니다.
한도를 넘거나 파일 내용이 없으면 조용히 자르지 않고 미완료 COMMENT로 안내합니다.

전체 저장소를 탐색하지 않으므로 추가 문맥이 필요한 구조 변경은 `complete=false`로 반환하도록 합니다.
reviewed_files가 전체 변경 목록과 정확히 일치하는지, 지적 줄 번호가 파일 범위에 있는지 확인합니다.
이 검증은 모델의 범위 선언을 확인하며 모델 판단의 정확성을 증명하지는 않습니다.
중복 JSON 키, 비표준 상수, 빈 근거, 잘린 전체 파일 문맥도 정상 리뷰 결과로 사용하지 않습니다.

모델 잡은 읽기 권한만, 게시 잡은 PR 쓰기 권한을 사용하며 OpenAI 키를 받지 않습니다.
같은 실행의 아티팩트만 읽으며 식별자·크기·스키마를 검증합니다. 게시 직전 SHA를 다시 확인합니다.
GitHub 리뷰 API에는 SHA compare-and-swap이 없어 마지막 조회와 POST 사이 경쟁은 완전히 제거할 수 없습니다.
정확한 commit_id와 본문에 검토 대상 SHA를 남깁니다. 실제 병합 차단에는 별도의 ruleset 검토가 필요합니다.

PR별 concurrency와 bot identity + SHA + 결과 종류 marker로 중복 게시를 줄입니다.
미완료 안내가 나중의 정상 리뷰를 가리지 않습니다. POST 응답 유실 시 자동 재전송하지 않으며,
다음 실행은 기존 리뷰 목록부터 확인합니다.

## 설정

필수 secret은 `OPENAI_API_KEY` 하나입니다. 키 값이나 기존 secret 목록은 코드에서 조회하지 않습니다.
OIDC 계정/연결, secret 등록, 보안 규칙 변경은 이 코드의 작업이 아닙니다.

| repository variable | 기본값 | 의미 |
| --- | --- | --- |
| SOL_REVIEW_ENABLED | 켜짐 | 문자열 `false`일 때만 전체 실행 제외 |
| SOL_REVIEW_PUBLISH | 켜짐 | 문자열 `false`일 때만 게시 대신 dry-run |
| SOL_REVIEW_MODEL | gpt-6.1-sol | 현재 허용 모델 하나; 교체 시 기능/가격과 allowlist 검토 |
| SOL_REVIEW_EFFORT | high | low/medium/high |
| SOL_REVIEW_MAX_OUTPUT_TOKENS | 16000 | 1024–32000, reasoning/output API 한도 |
| SOL_REVIEW_TIMEOUT_SECONDS | 600 | 30–900초, 별도 Actions 단계 16분/리뷰 잡 25분 한도 |

실행당 유료 Responses 호출은 **1회**입니다. tools, 이전 응답 연결, 자동 재시도가 없습니다.
입력 byte 제한은 정확한 토큰 또는 달러 상한이 아닙니다. 무료 잔액이나 무료-only를 보장하지 않습니다.
출력 한도 도달 등 불완전 응답은 COMMENT로 알립니다. 재실행은 추가 API 비용이 생길 수 있습니다.

## 검증

```sh
python3 -m unittest discover -s .github/scripts -p 'test_*review*.py' -v
```

오프라인 테스트는 판정, 좁은 RC 범위와 바뀐 줄 계산, Approve와 거부 시 COMMENT, 미완료 COMMENT, stale 결과, 게시 오류,
문맥/JSON 검증, 중복, 기본 활성화/opt-out, 자격 증명 분리를 검사합니다.
실제 모델 품질·인증·과금·리뷰 제출은 오프라인 테스트로 검증되지 않습니다.

- https://developers.openai.com/api/reference/resources/responses/methods/create
- https://developers.openai.com/api/docs/models/gpt-6.1-sol
- https://developers.openai.com/api/docs/guides/chatgpt-api-credits
