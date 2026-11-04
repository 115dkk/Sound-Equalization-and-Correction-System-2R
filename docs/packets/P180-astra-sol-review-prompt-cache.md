# P180 ASTRA: Sol 리뷰 요청에 프롬프트 캐싱 적용

`.github/workflows/sol-review.yml`이 Responses API로 보내는 리뷰 요청을 gpt-6.1-sol의 명시적 프롬프트 캐싱에 맞게 고치고, 응답의 토큰 사용량을 기록해 리뷰 본문에 보이게 하십시오. 작업 트리는 `E:\wt\sol-cache`(브랜치 `claude/sol-cache`)이고 그 안에서만 일하십시오. 커밋하지 마십시오.

## 배경

유지보수자의 OpenAI 사용량이 11월에 11M 토큰, 54달러입니다. 리뷰 하나가 full 모드에서 36만~66만 토큰이고(PR #172 두 번 66만, #178 36만), 같은 PR이 지적을 고친 뒤 두세 번 다시 리뷰됩니다. 지금 요청은 `instructions`(정책·지침·규칙 약 38 KB)와 `input`(문자열 하나에 PR 전체)을 보내고 캐시 설정이 없습니다.

OpenAI 문서(https://developers.openai.com/api/docs/guides/prompt-caching 과 https://developers.openai.com/api/reference/resources/responses/methods/create)에서 확인한 사실입니다. 구현 전에 두 페이지를 WebFetch로 다시 읽고, 아래와 다른 점이 있으면 문서를 따르고 보고에 적으십시오.

- gpt-6.1-sol은 'GPT-5.6 이후' 캐싱 방식입니다. 기본(암시적 모드)은 마지막 메시지 끝에 캐시 지점을 두어 PR 본문까지 캐시에 쓰고, 캐시 쓰기는 입력 요금의 1.25배(ttl 30m) 또는 2.5배(12h)입니다. 캐시 읽기는 입력 요금의 0.05배입니다(272K 토큰 이하 $2.00/M 대 $0.10/M, 그 위 $4.00/M 대 $0.20/M).
- 명시적 모드는 `prompt_cache_options: {"mode": "explicit", "ttl": "30m"|"12h"}`를 요청에 두고, 재사용할 내용 블록(`input_text`)에 `prompt_cache_breakpoint: {"mode": "explicit"}`를 붙입니다. 캐시 지점 뒤의 내용은 캐시에 쓰지 않으므로 할증이 없습니다. 접두가 캐시 지점까지 완전히 같아야 읽기가 됩니다. 최상위 `instructions`에는 캐시 지점을 둘 수 없으므로 재사용할 지시는 `developer` 메시지의 `input_text` 블록으로 보냅니다.
- 응답의 `usage`에 `input_tokens`, `input_tokens_details.cached_tokens`, `input_tokens_details.cache_write_tokens`, `output_tokens`, `output_tokens_details.reasoning_tokens`가 있습니다.
- `prompt_cache_retention`은 이 모델에서 폐기되었고(`prompt_cache_options.ttl`을 씀), `prompt_cache_key`는 GPT-5.6 이후 라우팅에 필요 없습니다. 둘 다 쓰지 마십시오.

## 설계

요청의 `input`을 문자열 하나에서 메시지 배열로 바꾸고, 바뀌지 않는 것부터 자주 바뀌는 것 순서로 둡니다.

1. `developer` 메시지 하나: 지금 `instructions()`가 만드는 글 전체를 `input_text` 블록 하나에 두고 `prompt_cache_breakpoint`를 붙입니다. 최상위 `instructions`는 보내지 않습니다. 신뢰 경계는 그대로입니다. 지시는 main의 문서만으로 이루어지고 PR의 것은 여기에 들어가지 않습니다.
2. `user` 메시지 하나에 `input_text` 블록 셋을 차례로 둡니다. 각 블록은 자기만으로 뜻이 통하도록 머리글을 답니다.
   - 블록 1 `UNTRUSTED REVIEW DATA, PART 1 OF 3 (merge base contents): ` 뒤에 `context` 가운데 `side == "base"`인 항목만 담은 JSON. 같은 PR을 다시 리뷰할 때 merge base가 같으면 그대로이므로 `prompt_cache_breakpoint`를 붙입니다. diff 모드(merge base 내용이 없음)에서는 빈 목록을 보냅니다.
   - 블록 2 `PART 2 OF 3 (head contents): ` 뒤에 `side == "head"`인 항목만 담은 JSON. `prompt_cache_breakpoint`를 붙입니다. 푸시로 바뀐 파일이 있으면 이 블록은 캐시에 맞지 않지만, 블록 1은 맞습니다.
   - 블록 3 `PART 3 OF 3 (identity, pull request, files, manifests): ` 뒤에 지금 `payload_base`의 `identity`, `pull_request`, `files`, `unreadable_files`와 `context_mode`, 그리고 `context` 가운데 매니페스트 항목(`side`가 base도 head도 아닌 것)을 담은 JSON. 캐시 지점 없음. `head_sha`가 푸시마다 바뀌므로 맨 뒤여야 합니다. 그 뒤에 지금처럼 `MAINTAINER FALSE-POSITIVE VERDICTS` 글을 같은 블록에 이어 붙입니다.
   - 항목의 순서는 지금 `changed_context`가 만드는 순서(파일 순서, 같은 파일 안에서 head 다음 base)를 유지하되 side로만 나눕니다. 같은 입력이면 바이트까지 같은 JSON이 나오도록 `json.dumps(..., ensure_ascii=False)`의 키 순서를 고정하십시오.
3. 요청에 `prompt_cache_options: {"mode": "explicit", "ttl": <환경 변수 REVIEW_CACHE_TTL, 기본 "30m", "30m"과 "12h"만 허용>}`를 둡니다. `store: false`, `reasoning`, `max_output_tokens`, `text.format`은 그대로이고 도구와 `previous_response_id`는 여전히 없습니다.
4. 토큰 수 세기(`responses/input_tokens`)는 같은 `input` 배열을 보냅니다. 그 끝점이 `prompt_cache_options`나 `prompt_cache_breakpoint`를 거부하면 세기 요청에서만 빼고, 거부하는지 문서로 확인해 보고에 적으십시오. `MAX_INPUT_BYTES` 검사는 모든 블록의 글자 합으로 합니다.
5. 응답의 `usage`를 `metadata.json`에 `usage: {input_tokens, cached_tokens, cache_write_tokens, output_tokens, reasoning_tokens}`로 기록합니다(없는 필드는 0). `extract_result`는 그대로 결과만 돌려주고, `run`이 응답에서 usage를 꺼내 metadata에 저장합니다.
6. 게시 잡(`sol_review.py publish`)은 리뷰 본문 끝에 한 줄을 추가합니다. 예: `토큰: 입력 357,078(캐시 읽기 0, 캐시 쓰기 357,078) · 출력 12,345(추론 11,000)`. usage가 없으면 줄을 두지 않습니다. 본문 길이 한도 안에 들도록 `_bounded_body`가 자르는 순서를 지키십시오.
7. `.github/sol-review/policy.txt`의 신뢰 경계 문단 뒤에, 리뷰 데이터가 PART 1(merge base 내용), PART 2(head 내용), PART 3(identity·pull request·files·manifests)으로 나뉘어 오고 셋을 합친 것이 하나의 리뷰 데이터라는 한 문단을 영어로 추가하십시오. `context_mode`와 `identity`를 설명하는 문장은 그대로 둡니다.
8. `.github/workflows/sol-review.yml`의 리뷰 단계 env에 `REVIEW_CACHE_TTL: ${{ vars.SOL_REVIEW_CACHE_TTL || '30m' }}` 항목을 추가하고, `.github/sol-review/README.md`에 '프롬프트 캐싱' 절을 추가해 위 구조와 요금, `SOL_REVIEW_CACHE_TTL` 변수, 본문의 토큰 줄을 적으십시오(한국어 합니다체).

## 시험

- `test_sol_review_api.py`: `build_request`가 `instructions`를 보내지 않고 `input`이 developer 메시지 하나와 user 메시지 하나이며, 캐시 지점이 developer 블록과 PART 1·2에만 있고 PART 3에는 없음, `prompt_cache_options`의 기본과 허용 값, 잘못된 ttl 거부, `MAX_INPUT_BYTES`가 블록 합으로 계산됨, `run`이 usage를 metadata에 기록함(없는 필드 0), 기존 시험(재시도, 타임아웃, 토큰 수 폴백)이 새 모양으로 통과.
- `test_sol_review.py`: `prepare`가 세 블록으로 나눈 글을 만들고 PART 1이 base 항목만, PART 2가 head 항목만, PART 3이 나머지를 담으며 같은 입력이면 바이트까지 같음, diff 모드의 PART 1이 빈 목록, 게시 본문의 토큰 줄.
- 워크플로 계약 시험(`test_workflow_contract` 등)이 새 env를 확인하도록 고치십시오.

## 지킬 것

- 신뢰 경계를 바꾸지 마십시오. PR의 글은 user 메시지에만, main의 문서는 developer 메시지에만 들어갑니다.
- 요청 본문이나 응답 본문, 키를 로그에 남기지 마십시오(지금 규칙 그대로).
- `prompt-diff.txt`와 `prompt.txt`를 그대로 쓰되 내용은 블록 목록의 JSON(예: `{"developer": "...", "parts": ["...", "...", "..."]}`)으로 바꾸고, API 스크립트가 그것을 읽어 메시지를 만들게 하십시오. 파일 이름은 바꾸지 마십시오.
- 한국어 문서와 주석은 합니다체, em-dash와 이모지 금지. 영어 주석은 지금 파일의 어투를 따르십시오.

## 허용 파일

`.github/scripts/sol_review_api.py`, `.github/scripts/sol_review.py`, `.github/scripts/test_sol_review_api.py`, `.github/scripts/test_sol_review.py`, `.github/sol-review/policy.txt`, `.github/sol-review/README.md`, `.github/workflows/sol-review.yml`.

## 검증

포그라운드로 돌리고 끝날 때까지 턴을 끝내지 마십시오.

```sh
cd E:\wt\sol-cache
python -m unittest discover -s .github/scripts -p "test_*.py"
```

## 보고

바꾼 파일, 문서에서 다시 확인한 사실(위 배경과 다른 점 포함), 시험 결과의 마지막 줄, 남은 문제. 실제 API 호출은 하지 않습니다. 첫 실행은 머지 뒤 다음 PR의 리뷰에서 확인합니다.
