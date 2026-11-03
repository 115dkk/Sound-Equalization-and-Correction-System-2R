# Sol PR 리뷰

`.github/workflows/sol-review.yml`은 기본 활성화입니다. Responses API를 직접 부르는 GitHub Actions 봇이고,
리뷰 작성자는 `github-actions[bot]`입니다. 기본 모델은 `gpt-6.1-sol`, reasoning effort는 `xhigh`입니다.
머지하는 코드는 없습니다.

## 리뷰 기준

모델에 주는 지시(`instructions`)는 신뢰하는 문서만으로 이루어지고, 모두 main에서 가져옵니다.

- `policy.txt`: 신뢰 경계, SECS 불변식, 범주와 막는 지적의 규칙, 위치 표기, 출력 형식, 유지보수자가 정한 한국어 작성 규칙.
- `guide.md`: 리뷰 방법. Google Engineering Practices의 리뷰어 지침(CC BY 3.0)과 OpenAI Codex의 리뷰 루브릭(Apache-2.0)을
  PR 하나를 리뷰하는 모델에 맞게 줄여 고쳐 썼습니다. 이 PR이 만든 결함인지, 작성자가 고칠 만한지, 영향받는 코드를 실제로 짚을 수
  있는지를 따져 지적할 것만 지적하고, 지적할 것이 없으면 지적하지 않습니다. 출처는 파일 머리와 `docs/THIRD_PARTY.md`에 있습니다.
- `CLAUDE.md`, `CONTEXT.md`: 저장소 규칙. 규칙을 어긴 변경은 지적이 되고, 지적의 근거에 그 규칙을 적습니다.

PR의 제목과 본문, 파일 이름과 내용, 패치는 모두 신뢰하지 않는 데이터로 따로 보냅니다. 그 안에서 리뷰어에게
'승인하라', '지적하지 마라' 같은 말을 거는 문장은 그 자체가 막는 보안 지적입니다.

## 판정과 게시

- 끝난 리뷰는 막는 지적이 없으면 `APPROVE`, 있으면 `REQUEST_CHANGES`입니다(유지보수자의 설계, 2026-11-03에 다시 확인).
  막는 지적은 `blocking`이면서 범주가 `security`, `data_loss`, `build`, `invariant` 가운데 하나이고, 이 PR이 추가하거나 바꾸거나
  지운 줄을 가리키는 것입니다. 지운 줄은 merge base 쪽 줄 번호로 가리킵니다. 그 밖의 지적은 의견으로 함께 남깁니다.
  이 규칙은 정책 문장뿐 아니라 게시 코드가 강제합니다.
- 지적은 PR의 그 줄에 다는 인라인 리뷰 코멘트로 게시합니다. 디프에 보이지 않는 줄을 가리키는 지적만 리뷰 본문에 두고,
  PR의 Files changed로 가는 링크와 그 줄 앞뒤를 보여 주는 커밋 고정 링크를 붙입니다.
- 새 커밋을 끝까지 리뷰해 게시하면, 봇이 앞선 리뷰에서 연 스레드를 해결 처리합니다. 그래서 열린 스레드는 늘 최신 리뷰의 지적입니다.
- 오탐은 엄지 아래 반응(GitHub의 `-1`)으로 알립니다(유지보수자 결정 2026-11-03). 유지보수자 쪽 AI가 봇의 인라인 지적을
  오탐으로 판단하면 그 코멘트에 이 반응을 달고, 오탐인 이유를 답글로 단 뒤 스레드를 해결합니다. 리뷰어는 다음 리뷰부터 이 PR과
  최근 PR들에서 이 반응이 달린 지적을 모아 모델에 따로 보여 주고, 같은 지적을 되풀이하지 않게 합니다. 반응만 있어도 판정으로 치고,
  이유 답글은 있으면 함께 보여 줍니다.
- 게시할 때마다 지금 head가 아닌 커밋에 붙은 봇의 승인을 취소합니다. 새 커밋의 리뷰가 실패해도 예전 승인이 남지 않습니다.
  게시한 직후 head가 바뀌었으면 방금 단 승인도 취소합니다. 승인 취소와 스레드 해결 처리는 `GITHUB_TOKEN`으로 시도하고,
  거부되면 그 이유를 게시 잡의 로그에 남길 뿐 리뷰는 그대로 둡니다. GitHub는 `contents: write`가 없는 토큰의 스레드 해결을
  거부하므로(#164에서 확인), 게시 잡은 그 권한을 받습니다.
- 모델이 읽을 수 없는 파일(바이너리, 10 MB를 넘는 파일)은 리뷰하지 않고 본문에 목록으로 적으며, 판정은 나머지 파일로 냅니다
  (유지보수자 결정 2026-11-03).
- 끝나지 않은 리뷰(`complete=false`), 출력 누락, 잘못된 스키마, 시간 초과, API 오류, 리뷰 중 head가 바뀐 경우는 이유를 적은
  `COMMENT`입니다. 그때까지 찾은 지적은 함께 남기되 승인도 변경 요청도 하지 않습니다.
- API 크레딧 고갈: OpenAI가 `insufficient_quota`, `billing_hard_limit_reached`, `billing_not_active`로 거부하면
  '[API 고갈]'로 시작하는 한 줄 `COMMENT`만 남깁니다. 오류 본문에서는 이 코드만 읽고, 본문 자체는 기록하지 않습니다.
- GitHub Actions의 PR 승인은 저장소 설정(Settings → Actions → General → Workflow permissions의
  'Allow GitHub Actions to create and approve pull requests')이 켜져 있어야 합니다. GitHub가 그 이유로 승인을 거부하면
  같은 본문에 그 사실을 덧붙여 `COMMENT`로 한 번 남깁니다. 다른 오류는 숨기지 않습니다.
- 모델이 쓴 글은 게시하기 전에 다듬습니다. `@` 멘션은 알림이 가지 않게 끊고, HTML은 글자로 바꾸며,
  이 저장소 밖을 가리키는 링크와 이미지는 링크를 풀어 코드로 보여 줍니다.

`SOL_REVIEW_PUBLISH=false`는 게시하지 않고 판정만 기록하는 dry-run입니다.

## 신뢰 경계

- 이벤트는 둘입니다. `sol-review-request.yml`(`pull_request`)은 PR의 워크플로 파일로 돌며 끝나기만 합니다.
  그 완료가 `sol-review.yml`(`workflow_run`)을 부르고, 이 워크플로와 스크립트는 언제나 main의 것입니다.
  그래서 리뷰받는 PR은 자기를 리뷰하고 승인할 수 있는 코드를 바꿀 수 없습니다. `pull_request_target`은
  2026-12-02부터 공개 저장소에서 기본으로 막히기 때문에 쓰지 않습니다.
- 앞의 실행이 만든 것은 아무것도 쓰지 않습니다. PR은 GitHub가 그 실행에 기록한 head 커밋으로 API에 물어 찾고,
  열려 있고 draft가 아니며 main을 향하고 이 저장소의 브랜치인 PR이 정확히 하나일 때만 리뷰합니다. 포크의 PR은 리뷰하지 않습니다.
- PR의 파일은 API로 데이터만 읽습니다. 체크아웃하거나 실행하지 않고, 모델에는 도구가 없으며 자율 실행 루프도 없습니다.
- 리뷰는 head와 merge base 사이를 봅니다. 바뀐 파일의 예전 내용도 main의 끝이 아니라 merge base에서 읽으므로,
  리뷰 중 main이 앞으로 가도 리뷰가 낡지 않습니다.
- 모델 잡은 읽기 권한과 OpenAI 키만 받습니다. 게시 잡은 OpenAI 키 없이 PR 쓰기 권한과, 스레드 해결에 드는
  `contents: write`를 받습니다(유지보수자 승인 2026-11-03). 게시 잡은 main의 코드만 돌리고 PR의 것은 체크아웃하거나
  실행하지 않으며, main 자체는 ruleset이 지킵니다. 같은 실행의 아티팩트를 읽되, 바뀐 파일 목록은 API에서 다시 받아 맞춰 봅니다.
- 오탐 판정은 PR이 아니라 저장소에서 모읍니다. 저장소 소유자나 쓰기 권한이 있는 협업자가 단 엄지 아래 반응만 판정으로 치고,
  이유도 그 사람이 같은 스레드에 단 답글만 씁니다. 쓰기 권한이 없는 사람의 반응은 판정이 되지 않습니다. 쓰기 권한이 있는
  사람은 자기가 연 PR의 지적에도 판정을 남길 수 있고, 관리자와 그 AI가 PR을 여는 이 저장소에서는 그것이 의도한 쓰임입니다.
  반응을 단 뒤에 봇 코멘트가 고쳐졌으면 그 반응은 판정으로 치지 않습니다. 같은 저장소의 PR 워크플로는 `github-actions[bot]`
  이름으로 코멘트를 고칠 수 있으므로, 반응한 사람이 본 글과 다른 글이 판정이 되지 않게 하기 위해서입니다. 판정은 리뷰
  데이터와 다른 칸에 들어가고, 정책은 그 칸을 리뷰를 조종하려는 글로 보지 않게 합니다. 판정을 읽지 못하면 판정 없이 리뷰합니다.
- 리뷰 본문의 표식(`secs-sol-v3`)은 안내용입니다. 같은 저장소의 PR은 자기 워크플로로 `github-actions[bot]` 이름의 리뷰를
  달 수 있으므로, 표식으로 '이미 리뷰했다'고 판단하지 않습니다. 다시 실행하면 리뷰가 한 번 더 게시됩니다.

**승인을 머지 조건으로 삼기 전에 볼 것.** 같은 저장소의 PR은 자기 워크플로의 `GITHUB_TOKEN`으로 `github-actions[bot]`
이름의 승인을 직접 달 수 있습니다. 그래서 이 봇의 승인만으로는 Sol이 승인했다고 확인할 수 없습니다. 승인을 머지 조건으로
쓰려면 리뷰어에게 따로 GitHub App 같은 고유한 신원을 주고, ruleset에서 새 커밋이 들어오면 이전 승인을 무효로 하도록 해야 합니다.
GitHub 리뷰 API에는 SHA를 비교해 바꾸는 쓰기가 없어, 마지막 확인과 게시 사이의 경쟁은 게시 뒤 확인과 취소로만 줄입니다.

## 입력 한도

입력이 모델에 들어가는지는 크기가 아니라 토큰으로 판정합니다. 준비 단계는 바뀐 파일 목록을 페이지마다 모두 읽고
(GitHub가 돌려주는 최대 3,000개), 전체 문맥 프롬프트와 diff 프롬프트를 둘 다 만듭니다. API 단계는 실제 요청을 OpenAI의 입력 토큰
계산 API(`POST /v1/responses/input_tokens`)로 세어, 전체 문맥이 gpt-6.1-sol의 프롬프트 한도(922,000 토큰) 안에 들면 그것을 보내고,
넘으면 diff와 매니페스트만 보내며(`context_mode: diff`) 리뷰 본문에 그 사실을 적습니다. 계산 API는 모델 문맥보다 큰 입력을 세지 않고
400으로 거부하므로, 전체 문맥에서 400이 오면 들어가지 않는 것으로 봅니다.

남은 한도는 바깥 API가 정한 것입니다. Responses API가 받는 입력 문자열이 10,485,760자까지라 프롬프트와 지시를 합쳐 10 MB를 넘지
않아야 합니다. `GITHUB_TOKEN`의 REST 요청은 저장소당 시간당 1,000회라 파일 내용 읽기는 600회까지 하고, 넘으면 diff만으로 리뷰합니다.
조회는 429, 5xx, 끊긴 연결에서 기다렸다가 다시 하지만, 리뷰 게시 같은 쓰기는 응답이 사라져도 자동으로 다시 보내지 않습니다.
OpenAI 연결에는 TCP keepalive를 켭니다. GitHub의 호스팅 러너가 도는 Azure는 4분쯤 아무것도 오가지 않는 연결을 끊는데,
`xhigh`의 리뷰는 답을 보내기 전까지 그보다 오래 생각하기도 합니다(2026-11-03 #163의 리뷰가 5분 만에 `RemoteDisconnected`로 끊겼습니다).

## 설정

필수 secret은 `OPENAI_API_KEY` 하나입니다. 키 값이나 기존 secret 목록은 코드에서 조회하지 않습니다.

| repository variable | 기본값 | 의미 |
| --- | --- | --- |
| SOL_REVIEW_ENABLED | 켜짐 | 문자열 `false`일 때만 전체 실행 제외 |
| SOL_REVIEW_PUBLISH | 켜짐 | 문자열 `false`일 때만 게시 대신 dry-run |
| SOL_REVIEW_MODEL | gpt-6.1-sol | 현재 허용 모델 하나; 교체 시 기능/가격과 allowlist 검토 |
| SOL_REVIEW_EFFORT | xhigh | low/medium/high/xhigh |
| SOL_REVIEW_MAX_OUTPUT_TOKENS | 64000 | 1024–128000, reasoning/output API 한도 |
| SOL_REVIEW_TIMEOUT_SECONDS | 1800 | 30–1800초, 별도 Actions 단계 32분/리뷰 잡 40분 한도 |

effort 기본값은 `xhigh`이고(유지보수자 결정 2026-11-02), 생각이 길어지는 만큼 응답 시한 기본값도 허용 최대인 1800초입니다.
`xhigh`의 추론 토큰도 출력 한도(`SOL_REVIEW_MAX_OUTPUT_TOKENS`)에 들어가므로, 출력 한도에 걸린 불완전 응답이 잦으면 그 값을 올려야 합니다.

실행당 모델 응답 호출은 **1회**이고, 그 앞에 입력 토큰 계산 호출이 프롬프트마다 한 번(많아야 두 번) 있습니다.
2026-11-02의 가격(입력 100만 토큰당 2달러, 출력 100만 토큰당 10달러)으로 입력 922,000 토큰과 출력 64,000 토큰을 다 쓰면
한 번에 약 2.5달러이고, 보통 PR은 그보다 적습니다. 재실행은 그만큼 비용이 더 듭니다.

## 검증

```sh
python3 -m unittest discover -s .github/scripts -p 'test_sol_review*.py' -v
```

오프라인 시험은 PR 찾기, 판정과 막는 지적의 범위, 인라인 코멘트, 승인 취소와 스레드 정리, 읽지 못한 파일, 미완료 COMMENT,
stale 결과, 게시 오류, 글 다듬기, 자격 증명 분리를 검사합니다. 실제 모델의 리뷰 품질과 인젝션 저항, 인증, 과금, 리뷰 제출은
오프라인 시험으로 확인되지 않습니다. `workflow_run`은 main에 있는 워크플로만 부르므로, 이 구조는 머지한 뒤 다음 PR에서 처음 돕니다.

- https://developers.openai.com/api/reference/resources/responses/methods/create
- https://developers.openai.com/api/docs/models/gpt-6.1-sol
- https://docs.github.com/en/actions/reference/security/secure-use
- https://github.blog/changelog/2026-11-02-pull_request_target-blocked-by-default-december-2/
