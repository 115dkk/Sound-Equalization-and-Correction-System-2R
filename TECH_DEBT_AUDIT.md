# 기술 부채 감사

| 항목 | 값 |
|---|---|
| 저장소 | `115dkk/Sound-Equalization-and-Correction-System-2R` |
| 날짜 | 2026-11-01 |
| 커밋 | main `d3aca082`(v0.45.1)에 PR #153(`c01d51e8`, 0.46.0 예정)을 합친 사본 |
| 방식 | 인터랙티브, 수동 실행. 정기 감사(`monthly-audit.yml`)를 끝까지 돌릴 API 크레딧이 없어 유지보수자 요청으로 Claude가 같은 스킬(`/115dkk-skills:tech-debt-audit`)을 따랐습니다. 모듈 다섯 묶음(G1 수치 코어, G2 서비스·IPC·API·CLI, G3 형식·작성기·적용·측정, G4 플랫폼·관리 서비스·CI·문서, G5 화면)을 읽기 전용 점검자 다섯이 병렬로 읽었고, Claude가 병합, 중복 제거, 번호, 순위를 정했습니다. |
| 범위 | 저장소 전체. 업스트림 파이썬 트리(ADR 0001), `oracle/goldens/`, 생성 바인딩과 기록 응답, 잠금 파일, `reference/art/`의 내용은 제외 |
| 기준선 | `TECH_DEBT_AUDIT.md`, 2026-10-24, `445c7c6`(v0.27.0, 이슈 #92). 그 뒤 커밋 453개 |
| 함께 본 것 | 같은 날의 아키텍처 감사(`ARCHITECTURE_AUDIT.md`, 후보 A1~A5). 그 Fine as is에 든 module은 나누거나 합치라고 권하지 않았습니다. |
| 도구 | `cargo deny` 0.20.2(advisories, bans, licenses, sources 통과), Dependabot 경고 조회, `npm audit`(0건), 쓰지 않는 크레이트 의존을 찾는 스크립트. `cargo audit`, `cargo machete`, `knip`, `madge`, clippy pedantic은 설치돼 있지 않아 돌리지 않았습니다. `cargo deny`가 `cargo audit`과 같은 RustSec 데이터를 봅니다. |
| 검증 | 발견은 정적 코드 대조입니다. TD-04, TD-105, TD-106, TD-111, TD-147은 Claude가 코드를 다시 읽어 확인했고, TD-147, TD-148은 재현 테스트를 쓰고 PR #153에서 고쳤습니다. TD-135는 재분석 지점을 재현하지 못해 추정입니다. 나머지는 점검자의 정적 확인을 근거로 하며, 고칠 때 먼저 재현합니다. |
| 다 읽지 못한 곳 | `secs-jos` writer, `secs-mdat` builder, `secs-dirac` 하위 reader, `secs-rew` HTTP 전송 전체, `secs-audio` 추정기와 장치, `secs-filters` 재생과 골든 전체, `wasapi.rs` 전체, `ci.yml`의 모든 잡, 화면의 모든 시각 컴포넌트와 스킨 CSS, 카탈로그 키 전수 대조, `secs-cuda-sys` 로더 ABI, `secs-dsp` 최적화기 전체 |

## 요약

- 발견 53건입니다. **Critical 0, High 15, Medium 32, Low 6**. 상태별로는 기준선에서 남은 `OPEN` 9건, 새로 찾은 `NEW` 42건, 이 감사 중에 PR #153에서 고친 `FIXED` 2건입니다. 기준선 104건 가운데 95건은 해결됐습니다.
- **설계를 CamillaDSP와 Equalizer APO에 적용하는 과정이 실패에 약합니다.** Equalizer APO 적용은 사용 중인 `SECS2R` 폴더에 파일을 하나씩 덮어써서, 중간에 실패하거나 APO가 도중에 다시 읽으면 두 설계가 섞입니다(TD-105). 그 폴더 안의 링크를 따라가 바깥 파일까지 지웁니다(TD-106). CamillaDSP 묶음도 같은 이유로 TD-22가 남았고, 같은 설계에서 장치 옵션만 바꿔 다시 적용했다가 거부되면 이전 설정을 되살릴 수 없습니다(TD-108). 아키텍처 감사 A1이 이 넷을 함께 다룹니다.
- **다른 이름으로 저장이 원본을 지웁니다.** 목적지의 측정 폴더가 원본 프로젝트를 품으면(`D:/case.irs/room.secs`를 `D:/case.secs`로 저장) 원본 폴더 전체를 옆으로 옮긴 뒤 지웁니다(TD-04가 반대 방향으로 남음).
- **관리 서비스가 다른 사용자의 실행 파일을 실행합니다.** 사용자 B가 EqAPO만 관리하게 바꾸면 소유자 SID만 B로 바뀌고 A가 등록한 CamillaDSP 실행 파일은 남아 B의 권한으로 실행됩니다(TD-111). EqAPO 설정 하나를 못 읽으면 서비스 전체가 끝나 CamillaDSP도 멈춥니다(TD-113).
- **신뢰할 수 없는 프로젝트 파일의 할당 상한이 비어 있습니다.** `.irs` 안의 `.src.json` 사이드카는 크기 제한 없이 읽고(TD-112), `.secs` JSON은 파일 상한 안에서도 넓은 배열로 큰 메모리를 잡습니다(TD-19가 남음).
- **화면이 프로젝트 교체와 저장 실패를 놓칩니다.** 설정 저장이 실패해도 이전 주소로 적용하고(TD-118), 새 프로젝트의 설정 조회가 실패하면 이전 프로젝트의 값을 보여 주고 저장까지 하며(TD-119), 저장 후 닫기가 큐에 남은 편집보다 먼저 실행됩니다(TD-120).
- **이름 없는 내보내기 폴더의 색인이 상위 폴더 전체를 훑습니다.** `E:/out`으로 내보내면 E 드라이브 전체를 링크까지 따라 읽고, 읽지 못하는 형제 폴더 하나로 이미 쓴 내보내기가 실패합니다(TD-110).
- **Unix에서 로컬 API 토큰을 다른 사용자도 읽을 수 있습니다**(TD-109). 엔진의 Schroeder 추정은 IR 길이만큼의 메모리를 예산 밖에서 잡습니다(TD-107).
- **0.46.0에 넣을 기능(극저역 보호 수동 설정)의 결함 둘은 감사 중에 고쳤습니다.** MIMO 성적표가 컷을 곱한 응답을 컷 없는 목표와 비교해 판정 수치가 달라졌고(TD-147), 설정 편집을 거치지 않는 계산 요청은 0 Hz 같은 범위 밖 값을 그대로 썼습니다(TD-148).
- Sol PR 리뷰는 바뀐 파일의 전체 내용이 300 KB를 넘는 PR을 검토하지 못합니다. 첫 실제 실행인 PR #153(파일 72개)에서 '검토 미완료'만 남겼습니다(TD-141).

## 구조 모델

SECS 2R은 업스트림 파이썬 SECS 2를 그대로 둔 채 그 옆에 세운 러스트 워크스페이스이고, 의존은 한 방향으로만 흐릅니다. 기준선 이후 큰 틀은 그대로이고, 새로 생긴 것은 다섯입니다.
- **`secs-graph`**: 내보내기 그래프(#92 C11). 신호 체인과 MIMO 경로를 한 번 검증하고 모든 작성기에 토폴로지와 파일 묶음 쓰기를 줍니다.
- **`secs-apply`**: 게시하지 않는 크레이트. CamillaDSP WebSocket 클라이언트와 Equalizer APO `config.txt` 편집 함수만 갖고, 순서와 파일 설치는 서비스의 `apply.rs`가 맡습니다(ADR 0018).
- **관리 서비스**: `apps/secs2r-host`(Windows 서비스, SYSTEM)와 `apps/secs2r-host-shell`(트레이). 사용자 세션의 토큰으로 CamillaDSP를 띄우고 EqAPO Include를 지킵니다. 순수 규칙은 `rules.rs`의 `Core`와 `ProcessRules`에 있고, 순서는 `main.rs`의 요청 처리와 세션 이벤트가 맞춥니다.
- **공개 API**: `secs-api`(게시 이름 `sound-equalization-and-correction-system-rust-api`), `secs-local-api`(토큰 인증 로컬 HTTP와 WebSocket), `bindings/python`(PyPI `secs2r`).
- **CI의 Sol PR 리뷰**: `pull_request_target`에서 base SHA의 스크립트만 돌려 OpenAI Responses API로 PR을 검토하고 봇이 리뷰를 게시합니다.

README와 ARCHITECTURE의 큰 그림은 실제와 맞습니다. 어긋나는 곳은 ARCHITECTURE의 의존 그림이 `secs-apply`, 관리 서비스 둘, 파이썬 바인딩을 빠뜨린 것입니다(TD-145).

변경과 결함은 서비스의 `apply.rs`, `export.rs`, `calibration.rs`, 관리 서비스 `main.rs`, 화면의 `state/session.ts`와 `ipc/mock.ts`에 몰려 있습니다. 기준선 때와 마찬가지로 결함 대부분은 한 함수 안이 아니라 함수 사이의 순서, 즉 '실패하면 무엇이 남는가'와 '그사이 프로젝트가 바뀌면 어떻게 되는가'에서 나옵니다.

## 발견

범주 번호는 스킬의 아홉 차원입니다(1 구조, 2 일관성, 3 계약, 4 테스트, 5 의존성, 6 성능, 7 오류, 8 보안, 9 문서). 줄 번호는 위 커밋 기준입니다.

| ID | Status | 범주 | File:Line | 심각도 | 노력 | 설명 | 권고 |
|---|---|---|---|---|---|---|---|
| TD-105 | NEW | 7 오류 | `crates/secs-service/src/apply.rs:482-501` | High | M | Equalizer APO 재적용이 사용 중인 `SECS2R` 폴더의 파일을 `read_dir` 순서로 하나씩 교체합니다. `SECS.txt`가 마지막이라는 보장이 없고, 중간에 실패하면 복원하지 않으며, APO의 폴더 감시가 설치 도중 다시 읽으면 이전 설계와 새 설계가 섞입니다. | 세대 폴더에 묶음을 완성한 뒤 고정 진입 파일의 Include만 바꾸고, 전환이 성공하기 전에는 사용 중인 세대를 지우지 않습니다(A1). 복사 실패를 주입하는 시험을 둡니다. |
| TD-106 | NEW | 8 보안 | `crates/secs-service/src/apply.rs:457-465,495-525` | High | M | `collect_files`와 `remove_empty_dirs`가 `is_dir()`로 링크를 따라갑니다. `SECS2R` 안에 바깥 폴더를 가리키는 링크나 junction이 있으면 남은 파일 정리가 그 너머의 파일을 지웁니다. | 링크와 junction은 따라가지 않고 링크 자체만 다루며, 정리는 서비스가 만든 세대 안으로 한정합니다. 바깥 감시 파일을 가리키는 링크로 시험합니다. |
| TD-04 | OPEN | 8 보안 | `crates/secs-service/src/session.rs:381-384,436-470` | High | S | 다른 이름으로 저장이 측정 폴더의 포함 관계를 한 방향으로만 검사합니다. 원본이 `D:/case.irs/room.secs`이고 목적지가 `D:/case.secs`이면 목적지 `.irs`가 원본 프로젝트를 품는데도 통과해, 그 폴더를 옆으로 옮겼다가 `finish`에서 지웁니다. 원본 `.secs`와 그 폴더의 다른 파일이 사라집니다. | 정규화한 두 측정 폴더의 포함 관계를 양쪽으로 검사하고, 목적지 교체가 원본 프로젝트 파일을 품는 경우도 거부합니다. 반대 방향 사례에서 원본이 남는 시험을 둡니다. |
| TD-111 | NEW | 8 보안 | `apps/secs2r-host/src/main.rs:619-645`, `apps/secs2r-host/src/rules.rs:476-487` | High | M | A가 CamillaDSP 실행 파일을 등록하고 로그아웃한 뒤 B가 EqAPO만 관리하게 바꾸면(화면의 정상 동작이 `camilla: None`을 보냄) 소유자 SID만 B로 바뀌고 A의 대상은 남아 B의 토큰으로 실행됩니다. A가 고칠 수 있고 B가 실행할 수 있는 파일이면 A가 B 권한으로 코드를 실행합니다. | 소유자 SID가 바뀌는 요청에서는 이전 사용자의 대상을 합치지 않고 지웁니다. A 등록 뒤 B의 EqAPO 전용 요청을 실제 처리 순서로 시험합니다(A3). |
| TD-22 | OPEN | 7 오류 | `crates/secs-graph/src/files.rs:133-147`, `crates/secs-camilla/src/lib.rs:1549-1567` | High | M | 같은 폴더에 다시 내보내면 계수 파일을 같은 이름으로 하나씩 교체하고 실패해도 복원하지 않습니다. 두 번째 교체가 잠금이나 권한으로 실패하면 옛 YAML에 새 FIR 일부가 섞입니다. 회귀 시험(`secs-camilla/tests/golden.rs:1189-1205`)은 staging 실패만 다룹니다. | 세대별로 계수를 완성한 뒤 진입 설정 하나로 전환합니다(A1). 실제 설치의 두 번째 교체를 실패시켜 이전 설정과 계수가 모두 남는지 시험합니다. |
| TD-108 | NEW | 7 오류 | `crates/secs-service/src/apply.rs:130-155`, `crates/secs-service/src/design_export.rs:193-213` | High | M | CamillaDSP 적용 폴더를 설계 ID만으로 정합니다. 같은 설계에서 장치 옵션만 바꿔 다시 적용하면 사용 중인 설정 파일을 먼저 교체하고 백업을 지운 뒤 검증과 Reload를 보냅니다. CamillaDSP가 거부하면 이전 설정은 이미 없고, 이전 경로로 되돌려도 새 파일을 다시 읽습니다. 시험의 가짜 adapter는 고정된 `old.yml`을 돌려줘 이 경우를 놓칩니다. | 적용 시도마다 새 세대를 두거나 실제 설정 전체를 식별자로 삼고, 외부 적용이 성공한 뒤에만 이전 세대를 지웁니다(A1). 장치 옵션만 바꾼 재적용과 거부 시 복구를 시험합니다. |
| TD-112 | NEW | 8 보안 | `crates/secs-project/src/record.rs:206-220,658-661,694-697` | High | M | 프로젝트를 열 때 `.irs` 안의 `.src.json` 사이드카를 크기 제한 없이 `read_to_string`으로 읽습니다. `.secs` 64 MiB와 WAV 256 MiB 상한이 적용되지 않고, 여러 기록이 같은 사이드카를 가리키면 기록마다 따로 보관합니다. | 사이드카를 상한+1바이트로 읽고 프로젝트 전체의 출처 정보에 누적 예산을 둡니다. 같은 사이드카의 반복 참조를 시험합니다. |
| TD-19 | OPEN | 8 보안 | `crates/secs-project/src/pyjson.rs:260-280`, `crates/secs-project/src/project.rs:119-123,338-344` | High | M | 파일 상한은 생겼지만 할당 예산이 없습니다. `snapshots`에 null 1,200만 개를 둔 약 60 MB JSON은 상한을 통과하고, 원소마다 훨씬 큰 값으로 바뀐 뒤 Project 변환에서 다시 복제됩니다. 깊이 제한은 얕고 넓은 입력을 막지 못합니다. | 파싱과 변환 중에 노드, 문자열, 벡터 할당을 예산에서 차감합니다. 크기 상한 이하의 큰 null 배열을 작은 시험 예산으로 거부하는 시험을 둡니다. |
| TD-110 | NEW | 6 성능 | `crates/secs-service/src/export.rs:452-459,653-654`, `crates/secs-service/src/design_export.rs:319-359` | High | M | 이름 없는 명시적 내보내기 폴더의 색인을 만들 때 상위 폴더 전체를 재귀로 읽습니다. `E:/out`이면 E 드라이브 전체를 링크까지 따라 읽고 깊이와 방문 상한이 없으며, 읽지 못하는 형제 폴더 하나 때문에 파일을 이미 저장한 잡이 실패합니다. | 색인 범위를 내보내기 기본 폴더나 고른 묶음으로 한정하고 링크는 건너뜁니다. 읽을 수 없는 형제 폴더가 정상 내보내기를 실패시키지 않는 시험을 둡니다. |
| TD-109 | NEW | 8 보안 | `crates/secs-local-api/src/token.rs:21-31`, `crates/secs-types/src/fs.rs:56-59` | High | S | 로컬 API 토큰을 기본 권한의 `create_new`로 저장합니다. umask 022인 Unix에서는 0644가 되어, 다른 로컬 사용자가 토큰을 읽고 앱 권한으로 프로젝트 열기, 파일 저장, 측정을 요청합니다. ADR 0016의 '같은 사용자만 읽음' 전제와 어긋납니다. | 토큰 파일을 처음부터 소유자 전용(0600)으로 만들고 기존 토큰의 권한도 확인합니다. Unix에서 권한 비트를 시험합니다. |
| TD-116 | NEW | 3 계약 | `crates/secs-service/src/export.rs:271-304,543-581` | High | M | 캐시 mdat 내보내기가 낡음, MIMO 판정, 원 측정, 설명은 요청 시점에 잡고, 다시 설계할 프로젝트, 설정, IR은 작업 스레드에서 다시 읽습니다. 요청 직후 같은 프로젝트의 타겟이나 측정을 바꾸면 티켓 검사를 통과해 새 보정에 옛 MIMO와 설명이 섞이고 `stale`은 false로 남습니다. | 요청 시점의 프로젝트, 설정, 측정 식별을 고정하고 그 입력으로만 재생하거나, 잡 시작 전에 바뀌었으면 `StaleInputs`로 멈춥니다. 같은 프로젝트의 편집과 재측정 사례를 시험합니다. |
| TD-107 | NEW | 6 성능 | `crates/secs-engine/src/levels.rs:113-133,173-185`, `crates/secs-engine/src/design.rs:247-285,539-544` | High | M | 방 부피가 있고 RC 상한이 자동이면 엄격, 완만 Schroeder 추정이 IR 길이만큼의 f64 배열 네 벌을 만들지만, 설계 예산은 위치와 역할 수만 셉니다. 1천만 샘플이면 이 배열만 약 320 MB이고 작은 메모리 한도에서도 거부되지 않습니다(계산값). | 실제 최대 IR 길이로 작업 메모리를 산정해 할당 전에 예약합니다(A2). 같은 구성에서 IR 길이만 늘린 입력이 작은 예산에서 할당 전에 거부되는 시험을 둡니다. |
| TD-118 | NEW | 7 오류 | `apps/secs2r/ui/src/screens/export/useExportScreen.ts:77-91,164-172` | High | S | CamillaDSP 주소를 A에서 B로 바꾼 저장이 실패해도 적용을 계속합니다. writer는 실패를 `false`로 돌려주는데 `saveAddress`와 `saveExe`가 결과를 버리고, 서비스는 저장된 설정을 읽으므로 화면에 B가 보여도 A에 적용합니다. | 필수 설정 저장의 성공 여부를 적용 동작까지 넘기고, 하나라도 실패하면 적용하지 않습니다. 주소, 실행 파일, 적용 수준의 저장 실패 시 적용 요청이 없는 시험을 둡니다. |
| TD-119 | NEW | 3 계약 | `apps/secs2r/ui/src/state/effects.ts:81-90`, `apps/secs2r/ui/src/state/store.ts:33-47` | High | M | A를 연 상태에서 B 열기는 성공했는데 B의 `dsp_get`이나 보정 설정 조회가 실패하면 A의 값이 resource에 남습니다. B의 프로젝트 정보와 A의 설정이 함께 보이고, DSP 필드 하나를 고치면 writer가 A의 나머지 값까지 B에 보냅니다. | 프로젝트가 바뀌면 프로젝트 소유 resource를 비우거나 새 프로젝트 식별이 확인된 값만 사용합니다. 같은 프로젝트 재조회의 실패 정책은 그대로 둡니다. B 열기 뒤 조회 실패를 주입해 시험합니다. |
| TD-120 | NEW | 3 계약 | `apps/secs2r/ui/src/screens/project/useProjectScreen.ts:36-41,61-65` | High | M | 순서 있는 프로젝트 writer에 편집 둘이 있고 첫 응답이 늦을 때 '저장 후 닫기'나 '저장 후 열기'를 고르면, 아직 보내지 않은 두 번째 편집보다 `project_save`가 먼저 실행된 뒤 닫히거나 교체됩니다. 사용자가 저장을 고른 변경을 잃습니다. | 저장 전에 그 프로젝트의 대기 편집과 실패를 확인한 뒤 닫기와 교체를 진행합니다. 전역 `settled()`가 아니라 프로젝트 편집 전용 대기를 둡니다. 첫 응답 지연과 두 번째 편집으로 시험합니다. |
| TD-113 | NEW | 7 오류 | `apps/secs2r-host/src/main.rs:499-502,1051-1084` | Medium | M | EqAPO 설정 변경 알림 뒤 `config.txt`가 잠시 없거나 잠겨 있거나 복구 도우미가 실패하면 `repair_eqapo(...)?`가 서비스 루프를 끝냅니다. kill-on-close 때문에 관련 없는 CamillaDSP도 멈추고, 서비스는 종료 코드 0을 보고합니다. | EqAPO 복구 실패는 그 대상의 상태와 로그로 처리하고 횟수를 제한해 다시 시도합니다. 실패 중에도 CamillaDSP가 계속 도는지 시험합니다(A3). |
| TD-114 | NEW | 7 오류 | `apps/secs2r-host/src/main.rs:477-495` | Medium | M | 관리 중인 콘솔 세션 A와 다른 세션 B에서 로그오프해도 이벤트의 세션 ID는 기록에만 남기고 CamillaDSP를 멈추며 재시작 예약도 지웁니다. 다중 사용자와 원격 데스크톱에서 A가 로그인해 있는데 관리가 멈춥니다. | 실행 중인 자식의 세션과 이벤트 세션을 비교하고, 영향을 받는 자식만 전환합니다. 다른 세션의 로그오프로 시험합니다(A3). |
| TD-115 | NEW | 3 계약 | `crates/secs-service/src/project_edit.rs:793-802`, `apps/secs2r/src/main.rs:78-95`, `apps/secs2r/src/local_api.rs:84-100` | Medium | M | 앱 설정 저장은 직렬화했지만 효과(업데이터, 로컬 API, 아이콘)의 적용 순서는 저장 순서와 어긋나기도 합니다. A가 auto를 저장하고 멈춘 사이 B가 off를 저장하고 적용한 뒤 A가 auto를 적용하면, 저장값은 off인데 업데이터는 자동 확인을 합니다. | 설정 판(revision)과 효과 적용을 함께 관리해 옛 효과를 버립니다(A4). 두 요청의 저장과 적용을 엇갈리게 시험합니다. |
| TD-68 | OPEN | 2 일관성 | `apps/secs2r/ui/src/state/session.ts:288-318` | Medium | M | app writer와 apply writer가 같은 `AppSettings`를 별도 큐로 저장합니다. 둘이 같은 옛 값을 읽은 뒤 apply가 주소를 저장하고 app이 테마 등 전체를 저장하면 새 주소가 되돌아갑니다. | app 전체 변경과 apply 하위 변경을 한 직렬화 단위로 묶습니다(#92 C16의 writer를 마무리). 두 writer의 조회와 저장을 엇갈리게 시험합니다. |
| TD-117 | NEW | 3 계약 | `crates/secs-service/src/apply.rs:474-494`, `crates/secs-service/src/design_export.rs:253-308` | Medium | S | Equalizer APO 적용이 `config.txt`를 `SECS.txt`로 바꾸면서 `secs-design.json`은 그대로 복사해, 적용된 묶음의 manifest가 없는 `config.txt`를 가리키고 `SECS.txt`는 빠집니다. 시험(`tests/apply.rs:902-904`)은 `config.txt`가 없다는 것만 봅니다. | 최종 설치 이름으로 manifest를 다시 만들고, 적용 폴더에서 manifest의 모든 파일의 존재, 크기, 해시를 시험합니다(A1). |
| TD-121 | NEW | 3 계약 | `apps/secs2r/ui/src/screens/calibrate/Settings.tsx:287,300-305,335`, `apps/secs2r/ui/src/screens/calibrate/useCalibrateScreen.ts:73-83` | Medium | M | 추가 보정 대역 `[2000,4000]`에서 하한을 2500으로 바꾸고 재조회 전에 상한을 4500으로 바꾸면 두 번째 요청이 옛 배열로 `[2000,4500]`을 보내 저장한 하한을 되돌립니다. 추가와 삭제도 옛 배열 전체를 보냅니다. | 필드 변경을 순서대로 최신 배열에 합치거나 앞선 변경이 확정될 때까지 다음 편집을 막습니다. 지연 응답 중의 서로 다른 끝값 편집과 추가, 삭제로 시험합니다. |
| TD-122 | NEW | 3 계약 | `apps/secs2r/ui/src/screens/measure/useMeasureScreen.ts:31-35`, `apps/secs2r/ui/src/state/session.ts:205` | Medium | M | A에서 mdat를 고르고 scan 응답 전에 다른 클라이언트가 B로 바꾸면 늦은 scan이 B의 `host.mdat`를 설정해, B에서 A의 파일로 가져오기 확인창이 뜹니다. 타겟 파일과 내보내기 폴더 대화상자 뒤의 후속 요청도 같습니다. | 대화상자부터 후속 읽기와 저장까지 한 프로젝트 epoch로 확인하고 바뀌면 멈춥니다. 마이크 파일 선택(`useConnectScreen.ts:109-111`)의 보호를 재사용합니다(A5). |
| TD-124 | NEW | 3 계약 | `apps/secs2r/ui/src/screens/calibrate/Settings.tsx:180-192`, `apps/secs2r/ui/src/state/session.ts:540-544` | Medium | S | 방 크기가 없는 A에서 너비만 입력한 부분 초안이 React state에 남습니다. 다른 클라이언트가 역시 방 크기가 없는 B로 바꾸면 초안이 그대로 있고, B에서 나머지 둘을 입력하면 A의 너비까지 B에 저장합니다. | 프로젝트 식별이 바뀌면 방 크기 부분 초안을 비웁니다. null에서 null로 바뀌는 교체를 시험합니다. |
| TD-128 | NEW | 3 계약 | `crates/secs-service/src/mic_calibration.rs:253-280` | Medium | S | 마이크 보정 파일을 고르면 파일을 읽는 동안 세션 잠금을 놓습니다. 그사이 B를 열면 마지막 `require_mut`가 B를 돌려줘 A에서 고른 파일을 B에 저장하고 B를 dirty로 만듭니다. 다른 긴 저장과 달리 `ProjectTicket` 확인이 없습니다. | 처음에 티켓을 잡고 읽기가 끝나면 `session::apply`로 같은 프로젝트인지 확인합니다. 느린 읽기 중 프로젝트 교체로 시험합니다. |
| TD-129 | NEW | 3 계약 | `crates/secs-service/src/calibration.rs:153-195` | Medium | S | XO 추천 도중 사용자가 XO를 직접 바꾸거나 추천 포함을 꺼도 완료 시 추천값을 현재 설정에 덮어씁니다. 보정 완료에는 설정 변경 검사가 있지만 XO 완료에는 없습니다. | 시작 때의 XO와 설정 판을 확인하고, 그 뒤 편집이 있으면 추천 결과만 보존하거나 `StaleInputs`로 적용을 거부합니다. |
| TD-130 | NEW | 6 성능 | `crates/secs-service/src/export.rs:205,281-285,543-553`, `crates/secs-service/src/session.rs:538-579` | Medium | M | 원 측정을 담는 mdat 내보내기가 잡을 등록하기 전에 세션 잠금 안에서 모든 WAV를 읽고 f64로 늘립니다. 캐시 결과 재생은 잠금을 잡은 채 새 기본 취소 토큰으로 읽습니다. 큰 프로젝트에서 다른 요청이 막히고 취소가 닿지 않습니다. | 잠금 안에서는 목록만 복사하고 잡 자리를 먼저 잡은 뒤, 읽기와 변환은 잠금 밖에서 잡의 취소 토큰과 메모리 예산으로 합니다. |
| TD-126 | NEW | 6 성능 | `crates/secs-local-api/src/lib.rs:140-147,224-228,288-324`, `apps/secs2r/src/local_api.rs:173-176` | Medium | M | 인증한 클라이언트가 선언한 본문을 끝내지 않으면 `read_body`가 시간 제한 없이 기다립니다. 그 상태에서 API 끄기, 포트 변경, 토큰 재발급을 하면 `thread.join`이 끝나지 않고 Manager 뮤텍스를 계속 잡습니다. | 요청 읽기 마감과 종료 취소를 연결하고 서버 중지에 유한한 대기를 둡니다. 본문 일부만 보낸 소켓을 열어 둔 채 중지가 끝나는지 시험합니다. |
| TD-127 | NEW | 6 성능 | `crates/secs-api/src/lib.rs:179-183,203-214,354-359` | Medium | M | 공개 `Engine`이 끝난 잡의 이벤트를 HashMap에서 지우지 않고, 구독은 상한 없는 채널입니다. 수신자를 쥐고 읽지 않는 파이썬 `events()`나 러스트 소비자가 있으면 진행과 레벨 이벤트가 끝없이 쌓입니다. | 끝난 이벤트의 보관 기간을 정하고, 구독 큐에 상한과 '누락' 알림을 둡니다. 보내는 쪽을 막지는 않습니다. |
| TD-131 | NEW | 3 계약 | `crates/secs-apply/src/eqapo.rs:40-42,53-70,105-125` | Medium | S | 관리 Include 뒤에 공백이 붙으면 손상으로 판정하고, 복구는 표식만 지운 채 옛 Include 줄을 남긴 뒤 새 표식과 Include를 넣어 같은 파일의 Include가 두 줄이 됩니다. APO가 공백을 무시하면 필터가 두 번 걸리고, 되돌리기도 첫 줄을 남깁니다. 시험(`:189`)은 표식 개수만 셉니다. | 관리 표식 바로 뒤의 Include는 공백을 정규화해 같은 대상으로 보고, 복구와 되돌리기 뒤의 실제 Include 개수를 시험합니다. |
| TD-132 | NEW | 6 성능 | `crates/secs-apply/src/camilla.rs:174-176,193-218` | Medium | S | WebSocket 요청에 전체 마감이 없어, 소켓 제한보다 짧은 간격으로 Ping만 보내는 서버를 만나면 요청 루프가 끝나지 않고 상태 조회나 적용 잡을 붙잡습니다. | 요청마다 마감을 정하고 프레임마다 남은 시간을 확인합니다. Ping만 보내는 서버로 시험합니다. |
| TD-133 | NEW | 6 성능 | `crates/secs-service/src/export.rs:516-530`, `crates/secs-filters/src/lib.rs:1537-1573,1731-1766` | Medium | M | 컨볼루션 내보내기가 렌더링과 DTFT 계산 전에 `begin_commit`을 불러 잡이 취소 불가가 됩니다. 모든 입출력 짝과 추가 레이트를 렌더링하고 주파수마다 전체 탭을 두 번 DTFT하는 동안 취소와 진행 보고가 없습니다. | 계산과 파일 설치를 나누고 계산 반복 중에 취소와 진행을 받으며, `begin_commit`은 설치 직전으로 옮깁니다. |
| TD-49 | OPEN | 6 성능 | `crates/secs-engine/src/levels.rs:156-201`, `crates/secs-engine/src/design.rs:515,539-544` | Medium | M | 새 완만 RC 상한 추정이 8개 대역 × 모든 위치 × 모든 메인의 전체 IR을 취소 확인 없이 필터링합니다. 엄격 추정도 같습니다. 취소 지연이 IR 길이와 위치 수에 비례해 늘어납니다. | 두 추정에 취소 확인을 넘기고 대역과 측정 사이, 긴 샘플 처리 중에 확인합니다(A2). 산술 순서는 그대로 둡니다. |
| TD-123 | NEW | 4 테스트 | `apps/secs2r/ui/src/ipc/mock.ts:108,587-588` | Medium | S | 모의 서비스의 잡 배타 규칙이 apply만 export로 묶고 나머지는 같은 종류끼리만 거부해, 실행 중인 calibrate와 recommend_xo를 함께 허용합니다. 서비스는 compute와 device 묶음으로 막습니다(`secs-service/src/jobs.rs:27-34`). | 실제 JobGroup 계약을 모의 서비스에 옮기고, 같은 묶음의 다른 종류 둘을 연달아 시작하는 거부 시험을 둡니다. |
| TD-125 | NEW | 4 테스트 | `apps/secs2r/ui/src/ipc/mock.ts:987-993`, `apps/secs2r/ui/src/ipc/mockWiring.test.ts:166-178` | Medium | S | 모의 서비스의 단독 `mimo`가 프로젝트 존재만 확인하고, 시험도 위치 하나인 프로젝트에서 성공을 기대합니다. 서비스는 MIMO 켜짐, 위치 수, live 보정, 최신 여부, 설정 일치를 검사합니다. | 단독 MIMO에도 실제 선행 조건을 적용하고, 성공 픽스처를 유효하게 바꾼 뒤 조건별 거부 시험을 둡니다. |
| TD-134 | NEW | 4 테스트 | `fuzz/fuzz_targets/mdat_load.rs:6-8`, `crates/secs-mdat/src/ir.rs:48-53,60-114` | Medium | S | MDAT fuzz 대상이 `secs_mdat::load`까지만 부릅니다. 앱이 사용하는 `load_mdat_project`의 IR 복제, 타이밍 문구 해석, 출처 정보 추출과 TD-18의 추출 예산은 fuzz하지 않습니다. | 기존 대상에서 예산을 둔 `load_mdat_project`도 부르고, 보정 배열을 공유하는 seed를 넣습니다. |
| TD-52 | OPEN | 4 테스트 | `crates/secs-engine/src/tests/hf_phase.rs:151-167,238-243` | Medium | S | 새 HF 위상 시험이 오차를 `fold(0.0, f64::max)`로 모아 NaN을 버립니다. 대역 일부가 NaN이어도 허용오차 단언이 통과합니다. 막는 도우미(`tests/mod.rs:32-65`)가 이미 있습니다. | 공용 비교 도우미를 사용하고 정상 오차 사이에 NaN 하나를 넣은 회귀 사례를 둡니다. |
| TD-64 | OPEN | 4 테스트 | `oracle/export_engine_xo.py:19-21,31-49,115-120` | Medium | S | XO 재개 캐시의 지문이 업스트림 커밋, exporter, NumPy, SciPy만 봅니다. 입력을 만드는 `engine_cases.py`나 `export_engine_e2e.py`를 바꿔도 지문이 같아 옛 입력의 추천이 새 골든에 들어갑니다. | 생성한 입력의 지문을 넣거나 입력 생성기와 e2e 도우미의 해시를 지문에 넣습니다. |
| TD-135 | NEW | 8 보안 | `apps/secs2r-host/src/rules.rs:169-200`, `apps/secs2r-host/src/main.rs:696-708` | Medium | M | 업데이트 원본 검사는 드라이브 문자로 시작하는지만 봅니다. 사용자가 `C:\...` 아래에 UNC를 가리키는 링크를 두면 SYSTEM의 `metadata`와 열기가 서명 검사 전에 그 링크를 따라가 UNC 거부의 목적이 깨집니다. 재현하지 않은 추정입니다. | 원본 접근을 호출 사용자 권한으로 하거나 재분석 지점과 실제 위치를 안전하게 확인합니다. 로컬 이름 아래 UNC 링크로 거부 시험을 둡니다. |
| TD-136 | NEW | 4 테스트 | `crates/secs-policy/tests/gates.rs:61-103,1093` | Medium | S | 정책 게이트가 first-party 크레이트를 `crates/`와 `apps/`에서만 찾아, 워크스페이스 멤버인 `bindings/python`이 unsafe, 운영체제 의존, 하위 프로세스 검사에서 빠집니다. ADR 0019는 이 크레이트에도 정책을 적용합니다. 지금 위반은 없습니다. | 워크스페이스 멤버 목록에서 대상을 구하거나 `bindings/python`을 명시하고, 멤버 누락 검사를 둡니다. |
| TD-137 | NEW | 3 계약 | `.github/workflows/release.yml:398-422,475-493` | Medium | M | `pypi_only` 재게시가 현재 main을 빌드하고 `api-v<버전>` 태그는 존재만 확인합니다. 게시 실패 뒤 같은 API 버전의 main에 코드가 들어가면 다른 코드의 wheel과 sdist를 같은 버전으로 게시하고, `skip-existing` 때문에 옛 파일과 섞이기도 합니다. | 재게시할 API 태그를 먼저 정하고 그 커밋에서 산출물을 만듭니다. |
| TD-138 | NEW | 6 성능 | `apps/secs2r-host/src/rules.rs:448,497-511,555-565` | Medium | S | 디스크 로그는 회전하지만 `Core.logs`는 실행 내내 push하는 Vec입니다. 시작 때의 1,000줄 제한이 이후에는 적용되지 않아 오래 돌면 메모리가 늘고, 큰 `Log` 응답이 파이프의 1 MiB 제한을 넘으면 로그 조회가 실패합니다. | 메모리 로그에 항목이나 바이트 상한을 두고, 요청 줄 수와 응답 크기를 제한합니다. |
| TD-139 | NEW | 4 테스트 | `.github/workflows/release.yml:267-270`, `.github/workflows/ci.yml:913-929` | Medium | M | ADR 0020은 게시 전에 직전 실제 릴리스에서 새 서명 설치본으로 갱신해 보라고 정했지만, 릴리스는 게이트와 빌드만 기다립니다. PR의 A/B 검사는 같은 최신 소스를 0.0.1로 다시 빌드하고 임시 키를 사용해, 구판 업데이터와의 호환과 실제 키의 일치를 확인하지 못합니다. | 직전 실제 릴리스에서의 갱신 검사를 게시 조건에 연결합니다. 지금의 A/B 검사는 그대로 둡니다. |
| TD-140 | NEW | 7 오류 | `.github/workflows/monthly-audit.yml:563-590,604-639,642-673` | Medium | S | 감사 이슈 생성이 실패해도 경고만 남기고 성공으로 끝난 뒤 감사 태그와 기준 문서를 갱신합니다. 공개 이슈가 없는 실행이 기준이 되고, 코드 변경이 없으면 다음 실행이 건너뛰어 이슈를 다시 시도하지 않습니다. `:607-608` 주석과 어긋납니다. | 이슈 게시 성공을 기준 갱신의 조건으로 둡니다. |
| TD-141 | NEW | 7 오류 | `.github/scripts/codex_review.py`(prepare 단계의 입력 한도) | Medium | M | Sol PR 리뷰가 바뀐 파일의 양쪽 전체 내용이 입력 한도(정책 포함 300 KB)를 넘으면 검토를 시작하지 않고 '검토 미완료'만 남깁니다. 첫 실제 실행인 PR #153(파일 72개)에서 그랬고, 이 저장소의 기능 PR 대부분이 이 크기라 봇이 승인을 내지 못합니다. | 전체 내용이 한도를 넘으면 diff와 주변 hunk만으로 검토하는 단계를 둡니다. 그때도 끝나지 않은 리뷰는 승인하지 않습니다. 한도를 올리는 방안은 비용과 함께 정합니다. |
| TD-74 | OPEN | 2 일관성 | `crates/secs-service/src/export.rs:757-758`, `apps/secs2r/src/update.rs:126-141` | Medium | S | 원자적 저장 도우미는 생겼지만 내보내기의 provenance는 여전히 직접 저장하고, 새 업데이터도 자기 원자적 저장을 따로 구현했습니다. | 두 곳 모두 공용 도우미(`secs_types::fs`)를 사용합니다. |
| TD-85 | OPEN | 9 문서 | `apps/secs2r/ui/src/screens/measure/types.ts:24-38`, `apps/secs2r/ui/src/screens/connect/types.ts:66,74,89` | Low | S | 측정 방식이 저장되지 않는다, 서비스에 none이 없다, 측정 설정 응답에 스윕 선택지와 샘플레이트가 없다는 주석이 남았습니다. hook은 이미 그 필드를 읽습니다. | 해소된 주석을 지우거나 지금의 응답 필드를 적습니다. 타입과 IPC 이름은 바꾸지 않습니다. |
| TD-142 | NEW | 9 문서 | `docs/ENGINE.md:45`, `crates/secs-engine/src/lib.rs:80-84` | Low | S | ENGINE.md는 내부 시험, 하류 크레이트, 성능 도구가 `test-support` 기능으로 `test_support` 로더를 공유한다고 하지만, 지금 `test_support`는 `cfg(test)` 전용이고 외부 로더는 `secs-engine-testkit`에 있습니다. | 내부 로더와 `secs-engine-testkit`의 쓰임을 나눠 적습니다. |
| TD-143 | NEW | 9 문서 | `crates/secs-types/src/ipc.rs:1226-1228` | Low | S | 공개 `ExportKind::Convolution` 문서가 IR이 48 kHz라고 적지만 실제 출력은 프로젝트의 출력 레이트와 추가 레이트를 따릅니다. | 설계 레이트와 출력 레이트를 나눠 고치고 TS 바인딩을 다시 생성합니다. |
| TD-144 | NEW | 9 문서 | `crates/secs-filters/src/convolution_reaper.lua:213`, `crates/secs-filters/src/lib.rs:1651-1656,1798-1799` | Low | S | 44.1, 96, 192 kHz로 내보낸 REAPER 스크립트도 성공 안내에서 프로젝트와 장치를 48000 Hz로 맞추라고 합니다. 스크립트에 routing의 sample_rate를 넘기지 않습니다. | 스크립트에 routing의 sample_rate를 넣고 안내 문구에도 그 값을 사용합니다. |
| TD-145 | NEW | 9 문서 | `docs/ARCHITECTURE.md:22-50,56-84` | Low | S | 트리와 의존 그림이 `secs-apply`, 관리 서비스 둘, 파이썬 바인딩을 빠뜨렸는데, `:56`은 그림이 Cargo.toml 의존과 같다고 적습니다. | 빠진 구성과 의존을 넣습니다. |
| TD-146 | NEW | 9 문서 | `apps/secs2r/ui/src/gallery/tour/scenes.ts:698-703` | Low | S | `export.mimo-blocked` 장면의 한국어, 영어 설명이 MIMO가 있으면 Equalizer APO가 꺼진다고 하지만 F12와 모의 서비스는 APO 내보내기를 허용합니다. | 장면 ID는 두고 두 언어의 설명을 실제 제한에 맞춥니다. |
| TD-147 | FIXED | 3 계약 | `crates/secs-engine/src/mimo/run.rs:511-542` | Medium | M | MIMO 성적표가 입력 컷을 곱한 응답을 컷 없는 목표와 비교해, 컷을 켜면 MIMO의 판정 수치가 달라졌습니다(ADR 0031은 판정 수치를 그대로 두라고 정함). | PR #153에서 고쳤습니다. 성적표는 응답을 컷으로 나눈 값으로 계산하고, 같은 서포트로 컷을 켠 경우와 끈 경우의 성적표가 같은지 시험합니다(`input_hp_leaves_the_mimo_scorecard_as_without_it`). |
| TD-148 | FIXED | 3 계약 | `crates/secs-engine/src/design.rs`(설계 입구) | Medium | S | 설정 편집을 거치지 않는 계산 요청이 `input_hp_hz`에 0이나 범위 밖 값을 넣어도 엔진이 그대로 사용해, 0 Hz면 모든 내보내기 응답이 유한하지 않게 나왔습니다. | PR #153에서 고쳤습니다. 설계 입구에서 유한한 10~50 Hz가 아니면 입력 오류로 거부합니다(`design_rejects_an_input_cut_outside_its_limits`). |

## 기준선 이후 해결된 것

기준선의 104건 가운데 아래 95건은 해결됐습니다. 근거는 그 발견을 인용한 커밋이고(PR #93, v0.27.1), 이번 점검자들이 묶음마다 지금 코드의 줄을 다시 확인했습니다. 남은 9건(TD-04, 19, 22, 49, 52, 64, 68, 74, 85)은 위 표에 `OPEN`으로 있습니다. TD-10은 새 캐시에 대해서는 해결됐고, 키가 없는 옛 캐시의 처리는 관리자에게 묻는 것에 두었습니다.

- TD-01: `d6de9b97` fix(audit): A-1 MIMO 낡음을 보정 실행 지문으로 판정합니다 (#92 TD-01)
- TD-02: `5b73f569` fix(audit): A-3 Dirac IR에 내용 지문을 줍니다 (#92 TD-02)
- TD-03: `5478db16` fix(audit): A-5 다른 이름으로 저장한 뒤에도 캐시 결과를 유지합니다 (#92 TD-03)
- TD-05: `ea02f8f2` fix(audit): A-7 REW 레벨 재생이 스스로 끝나게 합니다 (#92 TD-05)
- TD-06: `2e27c9cb` fix(audit): A-8 REW 모니터가 오류에도 신호 발생기를 끕니다 (#92 TD-06)
- TD-07: `bf661ec7` fix(audit): A-9 레벨 모니터가 시작한 프로젝트에만 씁니다 (#92 TD-07)
- TD-08: `eec6c52f` fix(audit): A-10 캐시 결과의 mdat 내보내기가 요청한 프로젝트로만 설계합니다 (#92 TD-08)
- TD-09: `2acdd0b3` fix(audit): A-11 낡은 캐시 결과의 mdat 막힘에 재보정 버튼을 보입니다 (#92 TD-09)
- TD-10: `ff8b6e61` fix(audit): A-2 낡음 판정에 스피커 구성을 넣습니다 (#92 TD-10)
- TD-11: `01dc6564` fix(audit): A-4 계산 중에 바꾼 보정 설정을 덮어쓰지 않습니다 (#92 TD-11)
- TD-12: `4416782c` fix(audit): A-12 CLI mimo가 새 프로세스에서도 동작합니다 (#92 TD-12)
- TD-13: `75272b2d` fix(audit): A-13 sample_load 위치 수에 상한을 둡니다 (#92 TD-13)
- TD-14: `16024426` fix(audit): A-14 서브만 계산한 뒤에도 BA 표를 보입니다 (#92 TD-14)
- TD-15: `ca332129` fix(audit): A-15 IR WAV를 원자적으로 씁니다 (#92 TD-15)
- TD-16: `1e2be181` fix(audit): A-16 IR 쓰기를 프로젝트의 .irs 안으로 제한합니다 (#92 TD-16)
- TD-17: `16c22e97` fix(audit): A-17 CamillaDSP 식별자가 내보내기 폴더를 벗어나지 못하게 합니다 (#92 TD-17)
- TD-18: `3eda7085` fix(audit): A-18 mdat 출처 정보 추출도 할당 예산을 씁니다 (#92 TD-18)
- TD-20: `ffdc7105` fix(audit): A-20 잘못된 spl_offset 행에서 패닉하지 않습니다 (#92 TD-20)
- TD-21: `b3280eba` fix(audit): A-21 REW 콜백 서버가 계속 보내는 연결에도 멈춥니다 (#92 TD-21)
- TD-23: `95837bc0` fix(audit): A-23 MIMO 입력 예산을 병렬 작업 안에서 기다리지 않습니다 (#92 TD-23)
- TD-24: `af318eee` fix(audit): A-24 설계와 MIMO 준비가 메모리를 먼저 예약하고 XO는 잡 예산을 나눠 씁니다 (#92 TD-24)
- TD-25: `3b348c3c` fix(audit): A-25 오라클 비교가 맞지 않는 무한대를 통과시키지 않습니다 (#92 TD-25)
- TD-26: `6461af32` fix(audit): A-26 릴리스 게이트가 push·fetch 실패를 건너뜀으로 바꾸지 않습니다 (#92 TD-26)
- TD-27: `975dee65` fix(audit): A-27 XO 추천이 끝나면 설정을 다시 읽고 보정을 다시 계산합니다 (#92 TD-27)
- TD-28: `a7af0cea` fix(audit): A-28 XO 탭의 그룹을 선택한 채널에 맞춥니다 (#92 TD-28)
- TD-29: `1cd6915d` fix(audit): A-29 앱 설정을 읽지 못하면 다시 시도 화면을 보입니다 (#92 TD-29)
- TD-30: `f1d22518` fix(audit): A-30 프로젝트를 바꾼 뒤 도착한 이전 응답을 버립니다 (#92 TD-30)
- TD-31: `1ece0c5a` fix(audit): A-31 출력 번호 경고를 역할·대역·출력 줄로 보이고 출력 배선으로 이동합니다 (#92 TD-31)
- TD-32: `6b761513` fix(audit): A-32 MIMO 상한 문구를 업스트림과 같게 씁니다 (#92 TD-32)
- TD-33: `4d4ce045` fix(audit): A-33 결과 그래프도 계산 환경이 바뀐 MIMO를 낡음으로 봅니다 (#92 TD-33)
- TD-34: `f637ccce` fix(audit): A-34 레벨 모니터 오류를 레벨 확인과 같게 분류합니다 (#92 TD-34)
- TD-35: `a987d1a5` fix(audit): A-35 REW 레벨의 피크 값을 그대로 전합니다 (#92 TD-35)
- TD-36: `9b34a9b2` fix(audit): A-36 CLI import-mdat의 --position이 적용됩니다 (#92 TD-36)
- TD-37: `990c73e5` fix(audit): A-37 무거운 파일 읽기를 세션 락 밖, 취소할 수 있는 잡 안으로 옮깁니다 (#92 TD-37)
- TD-38: `edad8214` fix(audit): A-38 캐시 저장 실패를 계산 실패가 아닌 경고로 알립니다 (#92 TD-38)
- TD-39: `d7a3e1f0` fix(audit): A-39 앱 설정 저장을 직렬화하고 크기를 제한합니다 (#92 TD-39)
- TD-40: `40b898bb` fix(audit): A-40 이름대로 확인하지 않던 테스트 셋을 고칩니다 (#92 TD-40)
- TD-41: `9231ad02` chore(audit): C-1 기능 등록부의 IPC 근거를 동작 테스트로 잇습니다 (#92 TD-41)
- TD-42: `955fedb1` refactor(audit): B-1 pyjson 객체 읽기를 키 색인으로 바꿉니다 (#92 TD-42)
- TD-43: `2b92e1e6` fix(audit): A-41 출처 사이드카 읽기도 .irs 안으로 제한합니다 (#92 TD-43)
- TD-44: `de05f610` fix(audit): A-42 IR 캐시가 상한을 지키고 최근 사용 순서로 비웁니다 (#92 TD-44)
- TD-45: `f8ebfb46` fix(audit): A-43 f32로 넘치는 FIR 계수를 쓰기 전에 거부합니다 (#92 TD-45)
- TD-46: `bb4de2ee` fix(audit): A-44 CamillaDSP 작성기도 중복 역할을 거부합니다 (#92 TD-46)
- TD-47: `4a73fd18` fix(audit): A-45 WAV 읽기를 작성기와 무관한 형식별 픽스처로 검사합니다 (#92 TD-47)
- TD-48: `1eaf0703` fix(audit): A-46 XO 추천이 빠진 측정을 입력 오류로 알립니다 (#92 TD-48)
- TD-50: `8d6822f0` fix(audit): A-48 설계가 지원 범위 밖의 탭 수를 입력 오류로 거부합니다 (#92 TD-50)
- TD-51: `2a9ce1eb` fix(audit): A-49 넓은 대역의 홀수 차수 butter 대역통과가 패닉하지 않습니다 (#92 TD-51)
- TD-53: `ace6c8e2` refactor(audit): A-71 오라클 재생 adapter와 케이스 로더를 test_support 하나로 모읍니다 (#92 C9, TD-53)
- TD-54: `3f004935` fix(audit): A-51 GPU 실패의 상세 사유를 계산 보고에 남깁니다 (#92 TD-54)
- TD-55: `030a25c8` fix(audit): A-52 골든 로더가 인라인 꼬리 조각을 읽습니다 (#92 TD-55)
- TD-56: `7c3d5680` fix(audit): A-53 WASAPI 녹음과 재생이 멈춘 장치에서 무한히 기다리지 않습니다 (#92 TD-56)
- TD-57: `2c7e2ac8` fix(audit): A-54 WASAPI 녹음 큐를 미리 예약해 실시간 스레드에서 늘리지 않습니다 (#92 TD-57)
- TD-58: `0175faee` fix(audit): A-55 WASAPI 공유 모드 탐색이 장치 오류를 형식 거부로 바꾸지 않습니다 (#92 TD-58)
- TD-59: `af9178c2` fix(audit): A-56 기능 테스트 게이트가 ignored 테스트를 구별합니다 (#92 TD-59)
- TD-60: `7d60598f` ci(audit): C-2 Miri 검사 이름은 필수 검사 규칙이 아는 이름 그대로 둡니다 (#92 TD-60)
- TD-61: `cc96d246` chore(audit): C-3 외부 Actions를 커밋 SHA로, cargo-deny를 판으로 고정합니다 (#92 TD-61)
- TD-62: `109f918a` fix(audit): A-57 업스트림 동일성 검사가 git 실패를 일치로 보지 않습니다 (#92 TD-62)
- TD-63: `b237c730` fix(audit): A-58 오라클 런처로 engine_mimo_iir를 돌릴 수 있게 합니다 (#92 TD-63)
- TD-65: `c70e332d` fix(audit): A-60 프로젝트 이주 경고를 화면에 보입니다 (#92 TD-65)
- TD-66: `1a39e727` fix(audit): A-61 직접 측정한 출처를 가져온 파일로 보이지 않습니다 (#92 TD-66)
- TD-67: `c08517e6` fix(audit): A-62 타겟과 XO 저장이 실패하면 그래프를 되돌리고 다시 저장할 수 있게 합니다 (#92 TD-67)
- TD-69: `61baf06d` fix(audit): A-63 다른 이름으로 저장한 뒤 결과와 내보내기 폴더를 다시 읽습니다 (#92 TD-69)
- TD-70: `dce31044` fix(audit): A-64 축 범위 입력이 틀리면 적용하지 않고 그 칸으로 갑니다 (#92 TD-70)
- TD-71: `7d8944a7` fix(audit): A-65 모의 서비스의 어긋남 셋을 서비스 동작에 맞춥니다 (#92 TD-71)
- TD-72: `cc65bf59` fix(audit): A-77 작업 트레이와 화면이 같은 잡 장부를 봅니다 (#92 C12, TD-72)
- TD-73: `7d50e519` chore(audit): C-4 권고가 있는 화면 개발 의존성을 고친 판으로 올립니다 (#92 TD-73)
- TD-75: `d922c55a` refactor(audit): B-3 MIMO의 onset과 대역 경계 도우미를 하나씩으로 모읍니다 (#92 TD-75)
- TD-76: `0b2d3d1f` chore(audit): C-5 crate 문서가 없는 seam과 캐시, 구현자를 적지 않게 고칩니다 (#92 TD-76)
- TD-77: `a3be94a8` fix(audit): C-6 IPC 문서를 wire에 맞추고 시작 지연 선택지를 규칙 상수로 둡니다 (#92 TD-77)
- TD-78: `eaccd472` refactor(audit): B-4 파이썬 참거짓 판정을 한 곳으로 모읍니다 (#92 TD-78)
- TD-79: `c37c0900` chore(audit): C-7 secs-rew PARITY의 R1 설명과 Java 출력 경로를 구현에 맞춥니다 (#92 TD-79)
- TD-80: `d860cf4b` chore(audit): C-8 WASAPI 진단 변수를 문서에 적고 trace 오류를 알립니다 (#92 TD-80)
- TD-81: `795b56d9` refactor(audit): B-5 쓰이지 않는 오라클 Recorder를 지웁니다 (#92 TD-81)
- TD-82: `80c97716` chore(audit): C-9 현행 문서의 unsafe 예산, 캐시 판, 신호 사슬 설명, README를 고칩니다 (#92 TD-82)
- TD-83: `b29ac0ca` fix(audit): A-66 레벨 재생 요청이 실패하면 기다림을 바로 정리합니다 (#92 TD-83)
- TD-84: `15149d7a` refactor(audit): B-6 쓰이지 않는 화면 코드와 내보내기를 지웁니다 (#92 TD-84)
- TD-86: `a9436a25` chore(audit): C-11 루프백 안내와 실행 옵션의 하이픈을 고칩니다 (#92 TD-86)
- TD-87: `346f38a2` chore(audit): C-12 글꼴 생성 도구 판과 원본 해시를 적습니다 (#92 TD-87)
- TD-88: `a14f807e` chore(audit): C-13 의도한 secs-dsp 의존성을 machete 무시 목록에 적습니다 (#92 TD-88)
- TD-89: `f3e39aab` refactor(audit): B-7 쓰이지 않는 옵션을 지우고 예산 계산을 한 곳에서 합니다 (#92 TD-89)
- TD-90: `cb89583c` fix(audit): A-81 튜토리얼이 작업 폴더가 아니라 임시 폴더에 씁니다 (#92 TD-90)
- TD-91: `21c7ee58` fix(audit): A-82 측정 중에는 REW 드라이버와 장치를 바꾸지 않습니다 (#92 TD-91)
- TD-92: `2a5f30a9` fix(audit): A-83 중첩 객체의 모르는 키를 저장할 때 보존합니다 (#92 TD-92)
- TD-93: `fcc9405a` fix(audit): A-84 data 청크가 잘린 IR WAV를 거부합니다 (#92 TD-93)
- TD-94: `840989ac` fix(audit): A-85 음수 지연을 두 작성기가 똑같이 거부합니다 (#92 TD-94)
- TD-95: `b2638a6d` chore(audit): C-14 의존 방향 그림을 Cargo.toml과 맞춥니다 (#92 TD-95)
- TD-96: `96ab93a3` docs(audit): A-86 Householder 분해 한 번이 취소 지연의 상한임을 적습니다 (#92 TD-96)
- TD-97: `cf33eeaa` fix(audit): A-87 FFT 계획 캐시에 상한을 둡니다 (#92 TD-97)
- TD-98: `9db97286` chore(audit): C-15 버퍼와 Session의 Drop 실패 정책을 나눠 적습니다 (#92 TD-98)
- TD-99: `09b3746e` fix(audit): A-88 작업 표시줄 아이콘 AND mask 크기를 규칙대로 셉니다 (#92 TD-99)
- TD-100: `04da4949` fix(audit): A-89 배치도 스피커 누름 영역을 40px로 넓힙니다 (#92 TD-100)
- TD-101: `608bc010` fix(audit): A-90 편집 응답 전에 열거나 닫아도 저장 안 한 변경을 묻습니다 (#92 TD-101)
- TD-102: `e6d5d8df` fix(audit): A-91 모니터 종료 기다림에 시간 제한을 둡니다 (#92 TD-102)
- TD-103: `930f2295` chore(audit): C-16 DESIGN 1절에 go 단추의 예외와 조건을 적습니다 (#92 TD-103)
- TD-104: `9a620bf7` fix(audit): A-92 MIMO IIR 조율 값의 범위를 입구에서 검사합니다 (#92 TD-104)

## Top 5: 다른 것은 못 고쳐도 이것은 고칩니다

1. **배포 묶음을 세대로 설치합니다(TD-105, TD-106, TD-22, TD-108, TD-117, 아키텍처 A1).**
   - Equalizer APO: 새 묶음을 `SECS2R/<세대>/`에 완성하고, 고정 진입 파일 `SECS2R/SECS.txt`를 원자적으로 바꿔 `Include: <세대>/config.txt` 한 줄만 가리키게 합니다. 전환이 끝나면 다른 세대를 지웁니다. 폴더를 훑을 때는 `symlink_metadata`로 링크를 걸러 따라가지 않습니다.
   - CamillaDSP: 적용 시도마다 새 세대 폴더를 만들고, `ValidateConfig`와 `Reload`가 성공한 뒤에만 이전 세대를 지웁니다. 실패하면 이전 경로와 그 파일이 그대로 남습니다.
   - 묶음 쓰기(`secs-graph::files`): 같은 폴더의 재내보내기도 세대 폴더와 진입 파일 교체로 바꿉니다. manifest는 최종 설치 이름으로 만듭니다.
   - 시험: 두 번째 파일 설치에 실패를 주입해 사용 중인 묶음이 그대로인지, 바깥을 가리키는 링크가 있어도 바깥 파일이 남는지 봅니다.
2. **다른 이름으로 저장의 포함 검사를 양쪽으로 합니다(TD-04).**
   ```rust
   // session.rs reject_nested_irs: 지금은 destination이 source 안인 경우만 거부
   if destination == source || destination.starts_with(&source) || source.starts_with(&destination) {
       return Err(ServiceError::InvalidRequest("cannot save a project into a folder that holds it".into()));
   }
   // 그리고 열린 프로젝트 파일 자체가 destination_irs 아래인지도 같은 방식으로 거부
   ```
3. **관리 서비스가 소유자가 바뀔 때 이전 대상을 버립니다(TD-111).**
   ```rust
   // rules.rs Core::handle(SetManaged)
   if update.owner_user_sid.is_some() && update.owner_user_sid != self.managed.owner_user_sid {
       self.managed.camilla = None; // 이전 사용자가 등록한 실행 파일과 설정은 새 사용자에게 넘기지 않음
       self.managed.eqapo = None;
   }
   ```
   main.rs의 요청 처리는 이 결과로 실행 중인 자식을 멈춥니다. A 등록 뒤 B의 EqAPO 전용 요청 순서로 시험합니다.
4. **신뢰할 수 없는 프로젝트 파일에 할당 예산을 둡니다(TD-112, TD-19).** 사이드카는 `read_capped(path, SIDECAR_MAX + 1)`로 읽고, `pyjson` 파서에 노드마다 `size_of::<PyJson>()`와 문자열, 벡터 길이를 차감하는 예산을 넘깁니다. 예산은 `.secs` 상한의 몇 배처럼 파일 크기에 비례하게 정하고, 크기 상한 이하의 넓은 null 배열과 반복 참조 사이드카를 거부하는 시험을 둡니다.
5. **화면의 저장과 프로젝트 교체 순서를 바로잡습니다(TD-118, TD-119, TD-120).** 적용 동작은 `await saveAddress()`와 `await saveExe()`가 둘 다 true일 때만 `apply_camilla`를 부릅니다. 프로젝트 식별이 바뀌면 프로젝트 소유 resource(`dsp`, `calibrationSettings` 등)를 비웁니다. '저장 후 닫기'는 그 프로젝트 writer의 대기 편집이 끝나기를 기다린 뒤 `project_save`를 보냅니다.

## 빠른 수정 (노력 S, 심각도 Medium 이상)

- [ ] TD-04 다른 이름으로 저장의 양방향 포함 검사
- [ ] TD-109 로컬 API 토큰을 소유자 전용 권한으로
- [ ] TD-118 설정 저장이 실패하면 적용하지 않기
- [ ] TD-117 적용한 묶음의 manifest를 최종 이름으로
- [ ] TD-124 프로젝트가 바뀌면 방 크기 부분 초안 비우기
- [ ] TD-128 마이크 보정 파일 선택에 ProjectTicket 확인
- [ ] TD-129 XO 추천 완료 시 설정 판 확인
- [ ] TD-131 관리 Include 뒤의 공백 정규화
- [ ] TD-132 CamillaDSP WebSocket 요청의 전체 마감
- [ ] TD-123 모의 서비스의 잡 묶음을 서비스와 같게
- [ ] TD-125 모의 서비스의 단독 MIMO 선행 조건
- [ ] TD-134 MDAT fuzz 대상에 `load_mdat_project`
- [ ] TD-52 HF 위상 시험의 NaN 비교
- [ ] TD-64 XO 재개 캐시 지문에 입력 생성기
- [ ] TD-136 정책 게이트에 `bindings/python`
- [ ] TD-138 관리 서비스 메모리 로그의 상한
- [ ] TD-140 감사 이슈가 실패하면 기준을 갱신하지 않기
- [ ] TD-74 provenance와 업데이터의 원자적 저장을 공용 도우미로

## 이상해 보이지만 맞는 것

기준선의 38건은 그 아래 코드가 바뀌지 않아 그대로 유효하므로 다시 적지 않았습니다. 아래는 기준선 이후 새로 생긴 코드에서 검토하고 결함에서 뺀 것입니다.

- `crates/secs-audio/src/calibration.rs:384-420`: 저장된 IR은 두고 계산용 사본에서만 마이크 곡선을 뺍니다. 근거는 ADR 0015(13~16행)의 원본 보존 결정입니다.
- `crates/secs-graph/src/design.rs:36-43`: 설계 ID에 출력 샘플레이트를 넣지 않습니다. ADR 0017(15~17행, 27행)이 출력 레이트를 설계와 나누고 manifest에 적게 정했습니다. TD-108의 세대 폴더도 설계 ID의 뜻을 바꾸지 않고 따로 정합니다.
- `crates/secs-apply/src/eqapo.rs:104-132`, `camilla.rs:131-144`: 게시하지 않는 크레이트에 순수 편집 함수와 동기 클라이언트만 둡니다. ADR 0018(64~69행)의 결정입니다.
- `crates/secs-service/src/calibration.rs:125-147`: 한 잡에서 보정 결과를 먼저 커밋하고 MIMO를 돌립니다. `session.rs:265-280`의 계약이 뒤 단계의 취소가 끝난 단계를 되돌리지 않는다고 정합니다.
- `crates/secs-service/src/calibration.rs:469-493`: CPU 하나에서 자동 갱신은 토큰을 확인할 때 쉬는 방식으로 늦춥니다. ADR 0029(24~27행)입니다.
- `apps/secs2r/src/update.rs:21-22,363`: 업데이트 설치본을 메모리에 둡니다. ADR 0020(31행)의 선택입니다.
- `apps/secs2r/src/applog.rs:111-137`: 앱 로그 파일의 오류로 사용자 작업을 실패시키지 않습니다. ADR 0027(13~14행)입니다.
- `crates/secs-local-api/src/lib.rs:247-285`: Origin 헤더가 있는 요청을 거부합니다. ADR 0016(26행)이 브라우저 접근을 막기로 정했습니다.
- `crates/secs-engine/src/export_view.rs:57-80`: 입력 컷을 RC 설계에는 넣지 않고 내보내기용 응답에만 곱합니다. ADR 0031(35~39행)입니다. TD-147은 이 구분이 아니라 성적표의 비교 기준 문제였습니다.
- `crates/secs-dsp/src/rate.rs:1-25`: 출력 레이트를 바꿔도 설계와 성적표는 48 kHz입니다. ADR 0017(17~27행)입니다.
- `.github/workflows/sol-review.yml:4-6,17-31,40-52,64-89`: `pull_request_target`이지만 같은 저장소의 PR만 대상으로 하고 base SHA의 코드만 checkout하며 PR 코드를 실행하지 않습니다. 모델 잡과 게시 잡의 권한을 나눈 근거는 `.github/codex/README.md`의 신뢰 경계 절입니다.
- `.github/codex/policy.txt:9-16`: 병합을 막는 지적을 보안, 데이터 손실, 빌드, 불변식으로 한정합니다. 유지보수자가 2026-11-01에 정한 정책입니다.
- `.github/workflows/ci.yml:844-848,913-929`: PR의 업데이트 검사는 임시 키와 로컬 HTTP를 사용합니다. 주석에 시험 전용이라는 이유가 있습니다. TD-139는 이 검사가 아니라 직전 실제 릴리스 검사가 없다는 지적입니다.
- `crates/secs-sys/src/wasapi.rs:301-309,410-424`: 녹음 큐가 넘치면 패킷 전체를 버리고 끊김을 기록합니다. 실시간 처리 중에 재할당을 막는 이유가 주석에 있습니다.
- `apps/secs2r/ui/src/ipc/fixtures.ts:411-423`: 모의 서비스의 울트라 모드 표본은 새 프로젝트 예산에서 기록한 하나를 다른 예산에도 사용합니다. 주석과 기록기(`secs-service/tests/recorded.rs:145-168`)가 그 한계를 밝힙니다.
- `apps/secs2r/ui/src/screens/calibrate/Settings.tsx:542-568`: 극저역 보호의 자동 설정과 수동 설정은 함께 켤 수 있습니다. ADR 0031(57~63행)입니다.

## 관리자에게 묻는 것

1. **키가 없는 옛 캐시를 설정 면에서 최신으로 보는 정책을 유지하시겠습니까?** `crates/secs-service/src/results_tests.rs:458-462`가 그렇게 고정합니다. 그런 캐시를 연 뒤 스피커를 지워도 측정, 설정, 타겟 키가 같으면 TD-10의 문제가 남습니다.
2. **다시 연 MIMO를 재보정 뒤 숨기는 것은 의도입니까?** `results_tests.rs:378-397`은 캐시 MIMO가 있을 때 live 보정을 넣으면 MIMO를 없앱니다. `docs/ARCHITECTURE.md:145`는 MIMO가 남고 '다시 계산 필요'로 바뀐다고 적습니다. live 결과에만 해당하는 약속이면 문서에 밝힙니다.
3. **XO 추천 포함(`xo_rec_include`)은 화면 동작에만 적용합니까?** 서비스의 calibrate와 CLI(`apps/secs2r-cli/src/main.rs:774-777`)는 추천을 돌리지 않습니다. 공개 API에도 같은 정책이 필요한지는 제품 결정입니다.
4. **`MimoParams.settings`는 저장하지 않는 일회성 덮어쓰기입니까?** `crates/secs-service/src/calibration.rs:211-245,325-334`는 그 값으로 계산만 하고 저장하지 않아, 프로젝트와 다른 MIMO 옵션이면 결과가 곧바로 낡음이나 꺼짐으로 판정됩니다.
5. **내보내기 화면의 mdat 재보정이 XO 추천과 MIMO를 건너뛰는 것은 의도입니까?** `apps/secs2r/ui/src/screens/export/useExportScreen.ts:157`은 `settings: null, subs_only: false`만 보냅니다. 캐시 복구 전용이면 그 예외를 ADR 0028에 적고, 아니면 공용 전체 보정 도우미를 사용합니다.
6. **Equalizer APO가 내용만 바뀐 FIR WAV를 다시 읽습니까?** CamillaDSP는 읽지 않아 0.45.1에서 고쳤습니다. APO는 코드만으로 판정하지 못했습니다. TD-105의 세대 폴더로 바꾸면 파일 이름이 매번 달라지므로 이 질문과 상관없이 새 FIR을 읽습니다.
7. **MIMO IIR 최종 가드가 서포트를 뺀 이유도 메모로 알려야 합니까?** `crates/secs-engine/src/mimo/iir.rs:1226-1240`이 제거 목록을 만들지만 `run.rs:348-359`는 서포트만 가져옵니다. `features.toml:736-738`은 원인 메모의 범위를 이상 FIR로 정했습니다.
8. **극저역 보호의 DC 보정에 양의 DC 합이 보장됩니까?** `crates/secs-engine/src/rc.rs:457-463`은 부호 있는 `sum(h) - 1`만 봅니다(ADR 0022 71행의 식과 같음). 보장된다면 문서와 시험으로 남기고, 아니면 ADR의 식부터 다시 봅니다.
9. **관리 서비스의 파이프 응답 시한과 빠른 사용자 전환.** `crates/secs-sys/src/host.rs:157-167`은 처리 전에 만든 5초 `PipeWait`를 응답에도 사용하는데, 처리가 그보다 오래 걸리기도 합니다. `:1080-1088`은 connect와 disconnect 이벤트를 버립니다. 둘 다 Windows 실측이 필요합니다.
10. **CI의 paths-filter 권한과 설치 폴더 DACL.** `ui-gallery.yml`과 `ci.yml:190-209`의 paths-filter 잡에 `pull-requests: read`가 명시되지 않았고, `host.rs:570-654`의 설치 폴더 검사가 사용자 쓰기 권한이 있는 Administrators 소유 폴더까지 막는지는 확인하지 못했습니다.
