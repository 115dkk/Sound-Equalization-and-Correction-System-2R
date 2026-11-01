# Sol PR 리뷰 준비안

현재 활성화되지 않은 로컬 구현입니다. API 호출, GitHub 리뷰 제출, 병합은 하지 않았습니다.
`.github/workflows/sol-review.yml`은 `SOL_REVIEW_ENABLED=true`일 때만 실행됩니다.
공식 Codex GitHub 앱이나 Codex Action이 아니라 **Responses API + GitHub Actions 봇**입니다.
리뷰 작성 주체는 `github-actions[bot]`이고 모델 기본값은 `gpt-6.1-sol`입니다.

## 판정

- 완료된 리뷰에 지적 없음: `APPROVE` 판정.
- 사소하지만 근거 있는 결함만 있음: `COMMENT`.
- 보안 경계, 데이터 손실, 핵심 동작 또는 계산 오류 등 병합을 막아야 할 결함: `REQUEST_CHANGES`.
- 부분 리뷰, 출력 누락, 오류, 시간 초과, 알 수 없는 스키마, SHA 불일치: 실패. 승인으로 대체하지 않습니다.

현재 게시 함수의 기본값은 dry-run입니다. 워크플로의 `REVIEW_PUBLISH`는 문자 그대로 `false`이며,
API 사용을 켜더라도 리뷰 제출은 켜지지 않습니다. `APPROVE` 제출은 코드에서 별도로 차단합니다.
이 경계는 검토 후 활성화할 대상이며, 검사 통과와 실제 GitHub Approval은 같은 상태가 아닙니다.
병합 코드, 키 생성, secret 등록, 권한 변경은 포함하지 않습니다.

## 신뢰 경계

기본 브랜치의 정확한 base SHA에서 정책과 게시 코드를 가져옵니다.
PR 코드, 설치 스크립트, AGENTS 지시는 실행하지 않습니다. PR 파일은 API diff와 base/head 전체 파일 데이터로만 읽습니다.
루트 및 변경 파일의 상위 디렉터리에서 Cargo.toml/package.json을 찾아 의존 문맥도 제공합니다.
추가 문맥 조회는 최대 40개이며, 변경 파일은 각 side당 한 번씩만 가져옵니다.
fork PR과 draft는 제외합니다. missing/binary/truncated patch, 100개 초과 파일,
300,000 bytes 초과 입력은 수동 검토가 필요하다는 실패로 처리합니다. 내용을 조용히 자르지 않습니다.
모델이 문맥 부족을 판단하면 `complete=false`를 반환해야 합니다. 이 초기 구현은 전체 저장소를
탐색하지 않으므로 큰 구조 변경은 실패하거나 사람 검토가 필요할 수 있습니다.

모델 실행 잡은 읽기 권한만, 게시 잡은 PR 쓰기 권한만 사용하며 OpenAI 키를 받지 않습니다.
출력은 실행별 artifact로 전달하고 크기, SHA, 스키마, 지적 파일/근거를 검증합니다.
전체 변경 파일의 reviewed_files 목록이 정확히 일치해야 하며, 지적 줄 번호는 실제 파일 범위 안이어야 합니다.
이는 검토 범위의 자기 보고를 검증하며 모델 판단의 정확성을 증명하지는 않습니다.
중복 JSON 키, 비표준 상수, 빈 근거와 잘린 base64 파일 문맥도 거부합니다.
게시 직전에 base/head SHA와 열린 PR 상태를 다시 확인합니다.
GitHub 리뷰 API에는 SHA compare-and-swap이 없으므로 검사와 POST 사이의 극히 짧은 경쟁은
없앨 수 없습니다. `commit_id`를 정확히 지정하므로 새 SHA에 대한 리뷰로 오인하지 않게 합니다.
향후 Approval 활성화에는 stale approval 무효화 및 필수 검사 정책을 함께 검토해야 합니다.

PR별 concurrency로 게시 경쟁을 직렬화하며 bot identity + base/head marker로 재실행을 중복 제거합니다.
POST 응답이 유실되면 자동 재전송하지 않습니다. 다음 실행이 기존 리뷰 목록을 확인합니다.
현재 ruleset에는 리뷰 요구 규칙이 없으므로 RC만으로 병합이 금지된다고 약속하지 않습니다.

## 활성화 전에 필요한 설정

- 사용자 승인된 OpenAI 인증 연결. secret 이름 `OPENAI_API_KEY`는 기존 존재 여부를 확인하지 않았습니다.
  이 구현은 API 키 인증만 포함하며 OIDC 계정/연결 생성은 하지 않습니다.
- repository variable `SOL_REVIEW_ENABLED`: 현재 미설정이면 실행하지 않습니다.
- `SOL_REVIEW_MODEL`: 기본 `gpt-6.1-sol`. 현재 허용 모델은 이것 하나입니다. 모델 교체 시 기능/가격 검토 후 allowlist도 갱신합니다.
- `SOL_REVIEW_EFFORT`: 기본 `high`, 허용 low/medium/high.
- `SOL_REVIEW_MAX_OUTPUT_TOKENS`: 기본 16,000, 허용 1,024–32,000. reasoning/output을 포함한 API 한도입니다.
- `SOL_REVIEW_TIMEOUT_SECONDS`: 기본 600초, 허용 30–900초. HTTP timeout 외 Actions 단계 16분, 리뷰 잡 25분 제한도 둡니다.
- 입력은 정책 포함 300,000 UTF-8 bytes 이하이며 잘라 보내지 않습니다. 토큰 수의 정확한 사전 계산이나 달러 상한은 아닙니다.
- 실행당 Responses API 호출은 **1회**, tools/agent loop/자동 재시도는 없습니다. 출력 한도 도달로 incomplete가 되면 실패합니다.
- 무료 크레딧 잔액을 안다고 가정하지 않습니다. 무료 소진 후 유료 잔액 사용을 막는 계정별 조건은 별도 확인 대상입니다.
  시간 제한·토큰 제한은 무제한 호출을 막지만 정확한 청구 금액 또는 무료-only를 보장하지 않습니다.
- API 사용 활성화, 게시 활성화, Approval 활성화와 branch rules 설정은 각각 검토 후 진행합니다.
- 최소 유료 통합 시험은 작은 같은 저장소 PR 하나, 게시 dry-run 유지, 1회 API 실행입니다.
  이 시험도 현재 실행하지 않았습니다. 먼저 인증 프로젝트 및 비용 한도를 확인해야 합니다.

## 오프라인 검증

```sh
python3 -m unittest discover -s .github/scripts -p 'test_*review*.py' -v
```

공통 테스트는 3분기, 오류/부분 리뷰/누락/타임아웃, SHA 변경, fork, 입력 한도,
잘린 diff, 근거/경로 검증, 중복 방지, 제출 전 경쟁, dry-run 및 Approval 차단을 검사합니다.
실제 모델 품질, 인증, API 과금, GitHub Actions sandbox, 실제 리뷰 제출은 검증하지 않았습니다.

공식 근거:
- https://developers.openai.com/api/reference/resources/responses/methods/create
- https://developers.openai.com/api/docs/models/gpt-6.1-sol
- https://developers.openai.com/api/docs/guides/chatgpt-api-credits
