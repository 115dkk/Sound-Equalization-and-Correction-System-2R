# 기술 부채 감사

| 항목 | 값 |
|---|---|
| 저장소 | `115dkk/Sound-Equalization-and-Correction-System-2R` |
| 날짜 | 2026-10-24 |
| 커밋 | `445c7c6` (v0.27.0) |
| 방식 | 인터랙티브. 모듈 다섯 묶음(G1 수치 코어, G2 서비스·IPC·셸·CLI, G3 형식·작성기·측정 연동, G4 플랫폼·게이트·CI·문서, G5 화면)을 읽기 전용 감사자 다섯이 병렬로 보고, Claude가 병합·중복 제거·번호·순위를 정했습니다. |
| 범위 | 저장소 전체. 업스트림 파이썬 트리(ADR 0001), `oracle/goldens/`, 생성 바인딩, 잠금 파일은 제외 |
| 기준선 | 없음 (첫 전체 감사. 이전 감사는 M5 P41, 성능 P13, 스킨 P65 같은 부분 감사뿐) |
| 함께 본 것 | 같은 날의 아키텍처 리뷰(deepening 후보 16개, 감사 이슈의 Architecture Analysis). 그 리뷰의 Fine as is에 든 module은 나누거나 합치라고 권하지 않았습니다. |
| 도구 | `cargo audit` 0.22.2, `cargo deny` 0.20.2, `cargo machete`, `cargo clippy -W clippy::pedantic`(경고 2,620건, 대부분 수치 코드의 형 변환), `npm audit`, `knip` 5, `madge` 8(순환 없음). `cargo udeps`는 돌리지 않았습니다. |
| 검증 | 발견은 정적 코드 대조입니다. TD-01, 02, 03, 05, 07, 08, 17, 25, 27, 32, 33은 Claude가 코드를 직접 다시 읽어 확인했습니다. TD-23은 구조만 확인했고 재현하지 않았습니다(PLAUSIBLE). 나머지는 감사자 보고를 근거로 하며, 고칠 때 먼저 재현합니다. |
| 다 읽지 못한 곳 | 스킨 CSS(`rack.css`, `matrix.css`, `ui.css`) 전체, 영어 카탈로그 대부분, 일부 통합 테스트·지원 해석기(`secs-camilla/tests/support`, `secs-filters/tests/support`, `eqapo_playback.rs`), `docs/perf/`, `docs/skins/`, `docs/DESIGN.md` 본문 전체, `CHANGELOG.md` 220행 이후, 엔진 테스트 일부 구간 |

## 요약

- 발견 104건: **Critical 2, High 30, Medium 50, Low 22**. 기준선이 없으므로 모두 `NEW`입니다. TD-90–TD-104는 처음 판에서 '관리자에게 묻는 것'에 두었던 항목을 코드, 업스트림, ADR로 판정하면서 결함으로 확인해 더한 것입니다.
- **잘못된 보정이 출하될 수 있는 경로가 둘입니다.** MIMO의 낡음 판정은 보정 설정 키만 봅니다. 그래서 측정이나 타겟을 바꾸고 다시 보정하면, 이전 측정으로 만든 MIMO가 최신으로 판정돼 새 보정과 함께 내보내집니다(TD-01). Dirac 가져오기는 UUID 없이 저장하므로 같은 자리에 다른 측정을 다시 가져와도 결과가 낡음으로 바뀌지 않습니다(TD-02). 낡음 판정에는 스피커 구성도 빠져 있습니다(TD-10).
- **다른 이름으로 저장이 약합니다.** 저장하면 캐시 결과가 사라지고(TD-03), 쓰기가 실패해도 세션은 새 경로로 바뀌며, `.irs`를 자기 하위로 저장하면 끝없이 복사하고, 기존 목적지의 IR을 원자적이지 않게 덮어씁니다(TD-04).
- **REW 레벨 재생이 멈추지 않습니다.** 100ms마다 신호를 다시 시작해 자동 종료에 닿지 못합니다(TD-05). 오류가 나면 신호 발생기를 끄지 않습니다(TD-06).
- **프로젝트를 바꾼 뒤 늦은 결과가 새 프로젝트에 들어갑니다.** 레벨 모니터(TD-07), cached mdat 내보내기(TD-08), 화면의 일반 응답(TD-30)이 그렇습니다. ADR 0009가 막으려던 경로입니다.
- **신뢰할 수 없는 파일의 경계가 쓰기 쪽과 할당 쪽에서 비어 있습니다.**
  - 쓰기: 조작된 캐시의 역할 이름으로 내보내기 폴더 밖에 쓸 수 있고(TD-17), IR 저장은 심볼릭 링크를 따라갑니다(TD-16).
  - 할당: `.secs`·WAV 읽기(TD-19)와 mdat 보정 배열 복제(TD-18)에 상한이 없고, 화면은 프로젝트의 출력 번호만큼 DOM을 만듭니다(TD-31).
- **파일 쓰기의 원자성이 고르지 않습니다.**
  - IR WAV를 덮어쓸 때 원자적이지 않습니다(TD-15).
  - 재내보내기 묶음 전체가 원자적이지 않습니다(TD-22).
  - 원자적 쓰기 구현 자체가 여섯 벌입니다(TD-74).
- **메모리 예산과 취소가 엔진 일부에 닿지 않습니다.**
  - 일반 설계와 MIMO 준비는 예산 밖에서 할당합니다(TD-24).
  - MIMO 입력별 예산은 중첩 병렬과 맞물려 교착할 수 있습니다(TD-23).
  - 준비, BA 버스 지연, RC 단계는 취소를 확인하지 않습니다(TD-49).
- **화면이 서비스 계약과 끊긴 곳이 있습니다.**
  - XO 추천값이 실제 앱에 나오지 않습니다(TD-27).
  - 설정 조회가 한 번 실패하면 빈 화면이 됩니다(TD-29).
  - 이주 경고가 표시되지 않습니다(TD-65).
  - 모의 서비스가 서비스와 어긋나 있고, 테스트가 그 어긋난 값을 고정해 테스트 결과를 믿을 수 없게 합니다(TD-71).
- **검증 장치가 몇 군데서 실패를 놓칩니다.**
  - 오라클 비교가 무한대를 통과시킵니다(TD-25).
  - 테스트의 오차 계산이 NaN을 삼킵니다(TD-52).
  - 하류 재생 adapter에 RC 중복 제거가 없습니다(TD-53).
  - 릴리스 게이트가 push 실패를 조용한 건너뜀으로 바꿉니다(TD-26).
  - 업스트림 동일성 검사가 diff 실패를 일치로 바꿉니다(TD-62).
- **의존성**: 출하 의존성에는 알려진 취약점이 없습니다. 개발 의존성 vite 8.2.1, happy-dom 20.14.5, vitest 4.1.11에는 권고가 있습니다(TD-73). `cargo audit` 경고 7건은 Tauri·GTK를 거쳐 들어온 전이 의존성이고, `deny.toml`이 그렇게 판단하도록 정해 두었습니다.

## 구조 모델

SECS 2R은 업스트림 파이썬 SECS 2를 그대로 둔 채 그 옆에 세운 러스트 워크스페이스입니다. 의존은 한 방향으로만 흐릅니다.
- **`secs-types`**: IPC와 도메인 어휘를 정의하고, ts-rs로 화면의 바인딩을 생성합니다.
- **수치 코어**: 계산은 `secs-dsp`의 f64 프리미티브와 `secs-engine`의 설계(레벨·딜레이, BA, XO, RC, MIMO, 성적표)가 합니다. `secs-compute`의 `Context`가 스레드, 예산, 취소, 진행을 들고 다닙니다.
- **GPU**: `secs-gpu`의 `RidgeBackend` adapter가 이상 FIR 정칙화만 GPU로 풉니다. 네이티브 호출은 `secs-cuda-sys`에 격리되어 있습니다.
- **형식**: 파일 형식은 `secs-project`(.secs, .irs), `secs-jos`/`secs-mdat`(REW mdat), `secs-dirac`(.liveproject)가 맡습니다.
- **작성기**: 출력은 `secs-camilla`(CamillaDSP), `secs-filters`(필터 텍스트·Equalizer APO·MIMO FIR 재생), `secs-export`(REW mdat)가 씁니다.
- **측정 연동**: `secs-rew`(REW HTTP)와 `secs-audio`(자체 측정)가 맡고, 운영체제 오디오는 `secs-sys`(WASAPI, cpal)가 담당합니다.
- **서비스**: `secs-service`가 `call(Request) -> Envelope`과 잡 이벤트로 이 모두를 묶습니다. Tauri 셸과 CLI는 이 서비스만 부릅니다.
- **화면**: React 화면은 세션 store와 훅이 IPC를 부르고, 모의 서비스와 견본 갤러리가 같은 seam의 두 번째 adapter입니다.

README와 ARCHITECTURE의 큰 그림은 실제와 맞습니다. 어긋나는 곳은 세 군데입니다(TD-82).
- 엔진의 interface가 "호출 하나"라는 설명은 실제로는 네 단계 규약입니다(아키텍처 리뷰 C7).
- "신호 사슬은 MIMO 경로까지 담은 값 하나"라는 CONTEXT의 설명과 달리, 코드에서는 MIMO 경로가 `SignalChain` 옆에 따로 다닙니다.
- `secs-sys`의 unsafe 예산은 문서에 0으로 적혀 있지만 실제로는 3입니다(ADR 0011).

변경이 몰리는 곳은 다음과 같고, 결함도 대부분 여기에 있습니다.
- 서비스의 `export.rs`, `measurement.rs`, `calibration.rs`
- `secs-types/src/ipc.rs`
- 화면의 `ipc/fixtures.ts`·`mock.ts`, 시스템·연결·보정 화면, 카탈로그

## 발견

| ID | Status | 범주 | File:Line | 심각도 | 노력 | 설명 | 권고 |
|---|---|---|---|---|---|---|---|
| TD-01 | NEW | 3 계약 | `crates/secs-service/src/calibration.rs:196` | Critical | M | MIMO가 의존한 보정의 식별자로 `settings.engine_key()`만 저장합니다. 측정이나 타겟을 바꾸고 같은 설정으로 다시 보정하면 새 보정은 최신이고 키도 같아서, 이전 측정으로 만든 MIMO가 다시 최신으로 판정됩니다. 조회(`calibration.rs:697-719`)와 내보내기(`export.rs:72`)가 이 판정을 써서 새 보정과 옛 MIMO 필터가 함께 출하됩니다. 다시 보정해도 `state.mimo`는 지우지 않습니다(`calibration.rs:122-128`). | MIMO에 보정 실행의 입력 지문(측정 서명, 타겟 키, 설정 키)을 저장하고 live와 cached 판정, 내보내기, 곡선에서 비교합니다. 캐시에도 같은 필드를 둡니다. 측정 교체 후 재보정하면 MIMO가 낡음이 되는 회귀 테스트를 둡니다. |
| TD-02 | NEW | 3 계약 | `crates/secs-service/src/dirac_import.rs:280` | Critical | S | Dirac 측정을 `uuid: None`으로 저장합니다. 측정 서명이 UUID를 더하는 이유가 같은 자리의 파형 교체를 잡기 위해서인데(`calibration.rs:837-839`), Dirac 재가져오기는 이 장치를 피합니다. 다른 측정을 같은 위치·역할로 다시 가져와도 보정과 MIMO가 최신으로 남습니다. | 가져온 IR마다 내용 지문(이미 쓰는 `sha2`) 또는 새 UUID를 부여합니다. 서로 다른 파형을 다시 가져오면 결과가 낡음이 되는 테스트를 둡니다. |
| TD-03 | NEW | 7 오류 처리 | `crates/secs-service/src/session.rs:143` | High | S | 다른 이름으로 저장은 캐시 파일을 포함한 `.irs`를 복사하면서(`session.rs:139`) 메모리의 `open.cached`를 버립니다. 다시 연 프로젝트(캐시 결과만 있음)를 다른 이름으로 저장하면 결과가 사라지고, 내보내기는 "calibration must run before export"로 실패합니다(`export.rs:35-40`). 캐시 읽기에는 경로 검사가 없어(`cache.rs:76`) 다시 열면 결과가 돌아오므로 의도된 무효화도 아닙니다. | Save As 뒤 `open.cached`를 유지하거나 새 경로에서 `cache::load`로 다시 읽습니다. 캐시 결과만 있는 프로젝트의 Save As → 결과·내보내기 테스트를 둡니다. |
| TD-04 | NEW | 7 오류 처리 | `crates/secs-service/src/session.rs:140` | High | M | 다른 이름으로 저장의 절차가 세 가지로 깨집니다. ① `.secs` 쓰기가 성공하기 전에 세션의 경로와 store를 새 목적지로 바꿉니다. 그래서 쓰기가 실패해도 세션은 실패한 경로를 가리킵니다. ② 목적지 `.irs`가 원본 `.irs` 안에 있으면 `copy_tree`가 방금 만든 목적지를 다시 복사하며 끝나지 않고, 그동안 세션 락을 쥡니다(`session.rs:228`). ③ 기존 프로젝트 위에 저장하다 중간에 실패하면 목적지 IR 일부만 바뀝니다(`session.rs:235`). | 새 store를 임시 값으로 준비하고 IR 트리를 임시 폴더에 완성한 뒤, 저장이 성공하면 교체합니다. 정규화한 원본과 목적지의 조상 관계를 거부합니다. 세 실패 경로를 테스트합니다. |
| TD-05 | NEW | 6 자원 | `crates/secs-service/src/monitor.rs:340` | High | S | REW 레벨 재생 요청을 처리한 뒤에도 `handled_generation`이 바뀌지 않아, 100ms 반복마다 `play_pink`를 다시 부르고 시작 시각을 초기화합니다(`monitor.rs:341-349`). 그래서 2.5초 자동 종료(`:361-364`)에 닿지 못하고 사용자가 멈출 때까지 핑크 노이즈가 나옵니다. | 이미 활성인 generation은 다시 시작하지 않습니다. 자동 종료까지 Play 한 번, Stop 한 번, `done` 이벤트를 확인하는 REW 테스트를 둡니다. |
| TD-06 | NEW | 6 자원 | `crates/secs-service/src/monitor.rs:345` | High | S | REW 모니터 본문의 `?` 오류 경로가 끝의 `stop_generator`와 `stop`(`monitor.rs:376-380`)을 건너뜁니다. `RewClient`는 Drop에서 Stop을 보내지 않으므로, 재생 중 오류가 나면 REW 신호 발생기가 켜진 채 남습니다. | 결과와 무관하게 정리하는 가드를 둡니다. Play 뒤 실패를 주입해 Stop과 구독 해제가 시도되는지 테스트합니다. |
| TD-07 | NEW | 3 계약 | `crates/secs-service/src/monitor.rs:384` | High | S | `store_level`이 시작한 프로젝트를 확인하지 않고 지금 열린 프로젝트에 레벨과 SPL을 쓰고 dirty로 만듭니다. A에서 재생하는 중 B를 열면 A의 장치로 잰 레벨이 B에 들어갑니다. ADR 0009의 '늦은 결과가 새 프로젝트를 덮지 않음'을 어깁니다. | 모니터 시작 때 세션 identity를 잡고 저장 직전에 비교합니다(아키텍처 리뷰 C4). 프로젝트 교체 테스트를 둡니다. |
| TD-08 | NEW | 3 계약 | `crates/secs-service/src/export.rs:273` | High | M | cached 결과로 mdat를 내보낼 때, 작업 스레드가 지금 세션을 다시 잠그고 현재 프로젝트로 설계합니다. 반면 SPL 오프셋, source 정보, 폴더, 라벨은 요청할 때 잡은 A의 값입니다. 중간에 B를 열면 B의 측정으로 설계하고 A의 메타데이터를 붙여 내보냅니다. | 요청할 때 프로젝트 스냅숏을 고정하거나 재설계 직전에 identity를 확인하고 `StaleInputs`로 멈춥니다. |
| TD-09 | NEW | 3 계약 | `crates/secs-service/src/export.rs:57` | High | M | 다시 연 프로젝트의 mdat 내보내기는 cached MIMO를 `!mdat` 조건으로 뺍니다. 그 뒤 보정만 다시 설계하고 MIMO 응답은 복구하지 않습니다. 그래서 다시 열기 전에는 들어가던 MIMO가 staged/before-after mdat에서 조용히 빠집니다. CamillaDSP에는 cached MIMO가 들어가므로 두 내보내기가 서로 다른 보정을 나타냅니다. | 재설계한 보정에 cached MIMO 경로를 적용해 mdat 입력을 복원합니다. 불가능하면 명시적으로 알립니다. |
| TD-10 | NEW | 3 계약 | `crates/secs-service/src/calibration.rs:821` | High | M | 낡음 판정이 측정 서명, 보정 설정, 타겟만 봅니다. 보정 후 `config_remove_speaker`로 채널을 지워도 결과가 최신으로 남고, 지운 채널이 든 사슬이 내보내집니다. | 엔진 입력에 영향을 주는 구성(역할, 그룹, 출력)을 지문에 넣습니다. 채널 삭제와 역할 변경 뒤 낡음 표시를 테스트합니다. |
| TD-11 | NEW | 3 계약 | `crates/secs-service/src/calibration.rs:129` | High | M | 계산을 시작할 때 복사한 설정을 완료할 때 프로젝트에 무조건 다시 씁니다(`store_calibration`). 계산 중에 `calibration_settings_edit`로 바꾼 설정이 옛 값으로 덮입니다. 설정 편집은 세션 identity를 바꾸지 않습니다. | 시작할 때 설정 키를 잡고, 그 사이 편집이 있으면 결과만 보존하고 낡음으로 표시합니다. 실행 설정을 현재 편집값에 되쓰지 않습니다. |
| TD-12 | NEW | 3 계약 | `apps/secs2r-cli/src/main.rs:446` | High | S | CLI의 독립 `mimo <project>` 명령은 프로젝트를 열자마자 MIMO를 요청합니다. 그런데 프로젝트를 열면 live 결과는 비고 cache만 돌아오고, `start_mimo`는 live 보정만 받습니다. 그래서 앞선 프로세스에서 `calibrate`를 했어도 `mimo`는 항상 "calibration must run before MIMO"로 실패합니다. | 서비스 경로로 보정 설계를 복원하거나 먼저 보정하도록 잇습니다. `calibrate` 다음 별도 프로세스에서 `mimo`가 성공하는 CLI 테스트를 둡니다. |
| TD-13 | NEW | 8 보안 | `crates/secs-service/src/measurement.rs:526` | High | S | `sample_load.positions`에는 하한만 있고 상한이 없습니다. 큰 u32를 주면 잡을 등록하기 전에 거대한 벡터를 할당하고, 취소를 막은 채 위치마다 IR을 만들어 파일로 씁니다. | 할당 전에 명시적 상한(배치 최대 17)을 검사하고 거부합니다. |
| TD-14 | NEW | 3 계약 | `crates/secs-service/src/curves.rs:371` | High | S | 전체 보정 없이 '서브만 계산'을 먼저 하면 `state.subs`는 있지만, `result_lines`가 `state.calibration`이 없다는 이유로 빈 결과를 냅니다. BA 곡선은 보이는데 BA 수치, 표, 경고는 사라집니다. 테스트는 항상 전체 보정을 먼저 합니다. | BA 요약을 `subs.or(calibration)`에서 만듭니다. 서브만 계산한 상태의 표와 경고를 테스트합니다. |
| TD-15 | NEW | 7 오류 처리 | `crates/secs-project/src/irs.rs:261` | High | S | 같은 위치와 역할을 다시 측정하거나 가져올 때 기존 WAV를 `fs::write`로 바로 덮어씁니다. 쓰기 도중 실패하면 기존 IR이 손상되고, 원자적으로 저장된 `.secs`가 그 손상된 파일을 가리킵니다. | 형제 임시 파일에 쓰고 동기화한 뒤 교체합니다(TD-74의 공용 도구). 기존 WAV가 있을 때 실패를 주입해 이전 바이트가 남는지 테스트합니다. |
| TD-16 | NEW | 8 보안 | `crates/secs-project/src/irs.rs:257` | High | M | 읽기에는 F5 경로 제한이 있지만 쓰기에는 없습니다. 받은 프로젝트의 `.irs/L_P1.wav`나 `.irs` 자체가 바깥을 가리키는 심볼릭 링크면, 재측정이나 가져오기가 링크 대상에 씁니다. | 쓰기 경로에도 F5를 적용하고 링크인 저장소와 중간 경로를 거부합니다. 원자적 교체로 파일 링크를 따라가지 않습니다. 파일 링크와 디렉터리 링크를 각각 테스트합니다. |
| TD-17 | NEW | 8 보안 | `crates/secs-camilla/src/lib.rs:415` | High | M | Camilla 식별자 검사가 ASCII, 줄바꿈, 콜론만 막고 `/`, `\`, `..`는 허용합니다. 역할 이름은 FIR 파일 이름에 그대로 들어갑니다(`lib.rs:823`, 쓰기 `:1515-1517`). 그래서 조작된 프로젝트 캐시의 사슬로 내보내면(`secs-service/src/export.rs:101`) 내보내기 폴더 밖의 `*_fir.wav`에 씁니다. | 역할과 `fir_prefix`를 안전 식별자 하나로 제한하고, 모든 계수 경로가 계수 폴더 안인지 쓰기 전에 검사합니다. 캐시 → 내보내기 탈출 테스트를 둡니다. |
| TD-18 | NEW | 8 보안 | `crates/secs-mdat/src/source_info.rs:125` | High | M | `load_mdat_project`의 후속 추출은 JOS·MDAT 할당 예산을 이어받지 않습니다. 여러 측정이 같은 큰 `meterCal` 배열을 참조하면 측정마다 배열을 복제합니다. 예를 들어 8 MiB 배열을 200번 참조하면 작은 파일 하나로 1.6 GiB를 넘게 잡습니다. | `extract_ir`와 `source_info::extract`가 예산을 받아 복제 전에 차감합니다. 공유 배열을 반복 참조하는 작은 합성 그래프로 테스트합니다. |
| TD-19 | NEW | 8 보안 | `crates/secs-project/src/record.rs:91` | High | M | 프로젝트 열기가 크기 제한 없이 `read_to_string`을 하고, JSON 파서에도 노드와 할당 예산이 없습니다. IR 읽기(`record.rs:500`)도 파일 전체와 data 청크 사본, 변환 배열을 제한 없이 만들고, 256 MiB 캐시 한도는 변환한 뒤에야 적용됩니다. 큰 `.secs`나 WAV를 열면 메모리가 고갈됩니다. | 읽기를 상한+1바이트로 제한하고 WAV 표본 수 상한을 읽기 전에 검사합니다. 캐시 한도와 입력 해석 한도를 나눕니다. 정상 상한과 초과 오류를 테스트합니다. |
| TD-20 | NEW | 7 오류 처리 | `crates/secs-project/src/record.rs:973` | High | S | 기존 측정 행의 `"spl_offset"`이 `null`이거나 객체가 아니면, SPL 오프셋을 가진 측정을 `record`하는 순간 `setdefault_nested`의 `expect`에서 패닉합니다. 그 전에 WAV와 사이드카는 이미 바뀌어 있습니다. `repair_entry`는 이 키를 복구하지 않습니다. | 파일을 쓰기 전에 형식을 검증하거나 다른 맵과 함께 복구합니다. 잘못된 기존 행으로 테스트합니다. |
| TD-21 | NEW | 6 자원 | `crates/secs-rew/src/callback.rs:247` | High | M | 큰 콜백 본문을 버리는 `io::copy`와 본문 읽기(`:258`)가 취소를 확인하지 않습니다. 로컬 클라이언트가 1초 소켓 제한보다 짧은 간격으로 계속 보내면 `stop()`의 worker join이 끝나지 않습니다. 경로 토큰도 본문을 다 읽은 뒤에 확인합니다. | 작은 청크로 읽으며 취소와 요청 마감 시각을 확인합니다. 토큰은 헤더 단계에서 확인합니다. |
| TD-22 | NEW | 7 오류 처리 | `crates/secs-camilla/src/lib.rs:1513` | High | M | 같은 폴더에 다시 내보낼 때 FIR을 하나씩 원자적으로 바꾼 뒤 YAML을 바꿉니다. 중간에 실패하면 이전 YAML이 새 FIR과 이전 FIR을 섞어 참조합니다. Equalizer APO(`secs-filters/src/lib.rs:1724`, `config.txt`를 마지막에 씀)와 필터 텍스트(`:1686`)도 같습니다. | 이번 실행의 계수를 세대 이름이나 디렉터리에 먼저 완성하고, 진입 설정 교체 하나로 새 묶음을 활성화합니다. 중간 실패 시 이전 묶음이 보존되는지 테스트합니다. |
| TD-23 | NEW | 6 자원 | `crates/secs-engine/src/mimo/design.rs:1214` | High | M | (PLAUSIBLE) 메인 입력마다 메모리 허가를 잡은 채 같은 Rayon 풀에 중첩 map을 보냅니다(`design.rs:349-354`). Rayon은 자식 작업을 기다리는 동안 다른 입력 작업을 같은 스택 위에서 실행할 수 있습니다. 그 작업이 Condvar에서 허가를 기다리면, 아래쪽 입력은 허가를 반환하지 못해 사용자가 취소할 때까지 멈춥니다. 허가가 입력 하나만 들어갈 만큼일 때(메모리가 적은 기기의 큰 MIMO) 닿습니다. | 허가를 쥔 입력 안에서는 Gram 블록을 순차로 만들거나 병렬을 한 계층으로 제한합니다. 입력 하나만 들어가는 예산, 여러 입력, 스레드 2개 이상으로 완료하는 회귀 테스트를 둡니다. |
| TD-24 | NEW | 6 자원 | `crates/secs-engine/src/mimo/prepared.rs:18` | High | M | MIMO 준비는 기존 `Design`을 쥔 채 500 ms 스펙트럼을 다시 만들고, 이어서 2 s 스펙트럼을 또 만듭니다. 이 할당은 입력별 허가 밖입니다(27역할×17위치면 스펙트럼만 약 1.4 GiB). 일반 설계(`design.rs:419`)도 예산을 쓰지 않습니다. XO 추천은 후보마다 전체 크기의 새 예산을 만듭니다(`xo.rs:266-271`). 그래서 메모리가 적은 기기에서 동시성을 줄이거나 미리 거절하지 못합니다. | 공유 준비 메모리와 채널별 작업 메모리를 할당 전에 예약합니다. XO 후보는 부모 예산을 공유합니다(`ctx.child_serial()`, 아키텍처 리뷰 C10). 정렬 시각은 스펙트럼 없이 구합니다(C8). |
| TD-25 | NEW | 4 테스트 | `crates/secs-oracle/src/lib.rs:406` | High | S | `assert_close`의 `near`가 기대값이 무한대일 때 성립합니다. 기대 +∞에 실제 유한수이고 `rtol > 0`이면 양변이 모두 ∞입니다. 반대 부호 무한대도 통과합니다. `assert_same`의 foreign-libm 경로도 이 함수를 써서 수치 회귀가 골든에서 숨습니다. | 비유한 값은 일치만 보고, 둘 다 유한할 때만 허용오차를 씁니다. 유한 대 무한, 반대 무한, NaN 조합의 거절 테스트를 둡니다. |
| TD-26 | NEW | 7 오류 처리 | `.github/scripts/release_gate.py:257` | High | S | bump push가 실패하면 원인을 가리지 않고 모두 "main이 움직임"으로 보고 `None`을 돌려줍니다(`:302-304`에서 `should_release=false`로 성공 종료). 배포 키 만료, 권한, 네트워크 실패에도 워크플로는 녹색인데 설치본은 나오지 않습니다. 태그와 main fetch의 반환 코드도 버리고 남은 `FETCH_HEAD`로 판정합니다(`:271`). | push 실패 뒤 원격 main을 다시 조회해 실제로 전진했을 때만 건너뜁니다. 나머지는 stderr를 남기고 실패시킵니다. fetch 성공을 요구하고 실패 허용을 고정한 `test_release_gate.py:145-146`을 바꿉니다. |
| TD-27 | NEW | 3 계약 | `apps/secs2r/ui/src/screens/calibrate/Settings.tsx:287` | High | M | 실제 앱에서 XO 추천을 마쳐도 추천값이 보이지 않습니다. 서비스 `start_xo`는 추천을 돌려주기만 하고 `xo_rec`에 저장하지 않습니다(`secs-service/src/calibration.rs:140-164`). 화면은 `settings.xo_rec`만 읽고, 완료 값은 `host.xoRecommendation`에 둡니다(`session.ts:305`). 모의 서비스는 설정까지 바꿔서 이 끊김을 가립니다(`mock.ts:523`). | 업스트림 동작을 확인해 서비스가 `xo_rec`를 저장하거나 화면이 완료 응답을 읽게 하고, 모의 서비스의 자동 변경을 뺀 상태에서 추천 → 표시 → 적용을 테스트합니다. |
| TD-28 | NEW | 3 계약 | `apps/secs2r/ui/src/screens/calibrate/ResultGraph.tsx:294` | High | M | SPL 탭에서 센터를 고르고 XO 탭으로 처음 가면, `host.xoGroup`이 `null`이라 화면은 첫 그룹(프론트)을 고릅니다. 그런데 서비스는 선택 채널의 그룹(센터)으로 곡선을 만듭니다(`curves.rs:283-290`). 그래서 센터 곡선을 보면서 선을 끌면 프론트 XO가 바뀝니다(`ResultGraph.tsx:367`). | 탭 진입과 채널 변경 때 그룹을 함께 정규화해 곡선 요청, 표시, 저장이 같은 그룹을 씁니다. SPL에서 C 선택 → XO → 끌기 경로를 테스트합니다. |
| TD-29 | NEW | 7 오류 처리 | `apps/secs2r/ui/src/App.tsx:210` | High | S | 시작할 때 `app_settings_get`이 한 번 실패하면 앱 전체가 빈 화면입니다. 오류를 그릴 `Shell`이 `if (!settings) return null` 아래에 있고 다시 시도할 길도 없습니다. | 설정 로딩·실패 화면을 `Shell` 밖에 두고 다시 시도를 제공합니다. 첫 조회 실패 뒤 복구하는 테스트를 둡니다. |
| TD-30 | NEW | 3 계약 | `apps/secs2r/ui/src/state/session.ts:109` | High | M | 잡 시작 응답에만 프로젝트 세대를 확인하고 일반 응답에는 하지 않습니다. 이전 프로젝트의 `measure_configure`나 `measure_config_get` 응답이 늦게 오면 새 프로젝트의 `measurementSettings`와 `host.path`를 덮고, 이후 `configure()`가 그 값을 새 프로젝트에 보냅니다. | 프로젝트에 묶인 일반 호출도 시작할 때 세대를 기록하고 교체 후 응답을 버립니다. 지연 응답 테스트를 둡니다. |
| TD-31 | NEW | 8 보안 | `apps/secs2r/ui/src/screens/system/SystemScreen.tsx:344` | High | S | 외부 `.secs`의 출력 번호가 DOM 할당량이 됩니다. 장치 채널 수를 모를 때 `max_channel`까지 `Array.from`으로 선택지를 만들고 스피커마다 그립니다. 파서는 `out`을 정수로만 읽습니다(`secs-project/src/config.rs:216-219`). 작은 파일 하나로 화면이 멈춥니다. 기준 출력 선택(`:677`)도 같습니다. | 선택지 수에 장치 정책 상한을 두고 범위 밖 저장값은 선택지 하나로 붙입니다. 큰 출력 번호 프로젝트로 테스트합니다. 서비스 쪽 검증도 함께 검토합니다. |
| TD-32 | NEW | 3 계약 | `crates/secs-service/src/export.rs:138` | Medium | S | 서비스가 MIMO 상한 문구를 `format!("max {}")`로 만듭니다. 업스트림(`src/secs/calibration.py:1000-1002`)과 엔진의 오라클 검증 함수(`secs-engine/src/mimo/run.rs:169-175`)는 `" · 입력당 서포트 ≤ N"`입니다. 그래서 mdat 노트가 "(Fsiso 150 Hzmax 4)"로 나갑니다. 설정 원값을 쓰고 엔진이 고정한 `run.key.max_supports`(0–12)를 쓰지 않습니다. 골든은 이 문자열을 오라클에서 직접 넣어 결함을 놓칩니다(`secs-export/tests/golden.rs:175`). | `secs_engine::mimo::cap_text(run.key.max_supports)`를 씁니다(아키텍처 리뷰 C2). `max_supports != 0`인 mdat 내보내기 테스트를 둡니다. |
| TD-33 | NEW | 2 일관성 | `crates/secs-service/src/curves.rs:75` | Medium | S | 그래프와 채널별 경로 수의 `fresh_mimo`는 계산 환경 키를 보지 않습니다. 결과 요약(`calibration.rs:705`)과 내보내기는 봅니다. Auto에서 CPU로 바꾸면 요약은 MIMO를 낡음으로 표시하는데, 그래프와 `result_lines`는 이전 MIMO를 최신처럼 그립니다. | 조회, 곡선, 내보내기가 같은 판정을 씁니다(아키텍처 리뷰 C1). 정책 변경 뒤 곡선 테스트를 둡니다. |
| TD-34 | NEW | 7 오류 처리 | `crates/secs-service/src/monitor.rs:49` | Medium | S | 레벨 확인은 `measure_error`로 오류를 구분하는데, 모니터는 설정 오류, 잘못된 역할, 공유 모드 거부, 녹음 오류를 모두 `DeviceUnavailable`로 바꿉니다. 같은 실패가 경로에 따라 다른 코드와 복구 안내로 나갑니다. | 모니터도 `measure_error`를 쓰고 실제 share 설정을 넘깁니다. |
| TD-35 | NEW | 3 계약 | `crates/secs-service/src/measure/rew.rs:103` | Medium | S | REW 레벨 결과의 `peak_dbfs`에 `level.input_dbfs`(RMS)를 넣습니다. 피크를 RMS로 보고하고, `level_apply`도 이 값으로 클리핑을 복원합니다. | `level.peak_dbfs`를 넘깁니다. RMS는 낮고 피크가 0 dBFS인 입력으로 테스트합니다. |
| TD-36 | NEW | 3 계약 | `apps/secs2r-cli/src/main.rs:427` | Medium | S | `import-mdat --position N`이 scan의 제안 위치에 가려집니다. scan이 모든 행에 `position: Some(..)`을 넣고 서비스는 이를 `params.position`보다 우선합니다. 위치 태그 없는 파일을 `--position 3`으로 가져와도 위치 1에 기록됩니다. | 위치 태그가 없는 행은 CLI가 지정한 위치를 쓰게 합니다. 테스트를 둡니다. |
| TD-37 | NEW | 6 자원 | `crates/secs-service/src/calibration.rs:279` | Medium | M | 보정과 XO 요청은 잡을 등록하기 전에 세션 락 안에서 모든 WAV를 읽습니다. 잡 ID가 없어 취소할 수 없고, 다른 요청이 막히며, 이미 계산 중이어도 준비를 다 마친 뒤에야 `JobBusy`가 나옵니다. mdat·Dirac 가져오기도 scan에서 읽은 파일을 락 안에서 다시 파싱하고, 그 전에 `begin_commit`으로 취소를 막습니다(`measurement.rs:1118`). | 잡 슬롯을 먼저 잡고 파일 I/O를 취소 가능한 본문으로 옮깁니다. 락은 스냅숏을 잡는 동안만 쥡니다. 실제 기록 구간에서만 commit으로 바꿉니다. |
| TD-38 | NEW | 7 오류 처리 | `crates/secs-service/src/calibration.rs:135` | Medium | S | 결과와 설정을 메모리에 반영한 다음 캐시 저장 실패를 잡 실패로 돌려줍니다. 사용자는 실패 알림을 받지만 결과는 이미 바뀌어 있습니다. MIMO도 같습니다. | 캐시 저장 실패를 성공한 계산의 경고로 분리합니다. 계약을 테스트합니다. |
| TD-39 | NEW | 3 계약 | `crates/secs-service/src/project_edit.rs:799` | Medium | S | 앱 설정의 파일 저장과 메모리 갱신이 한 동기화 구간이 아닙니다. Tauri가 요청을 병렬로 돌리므로, 교차하면 지금 조회되는 값과 재시작 후 복원되는 값이 달라질 수 있습니다. 읽기는 1 MiB로 제한하는데 저장에는 상한이 없어서(`:831`), 크게 저장된 설정은 재시작 때 버려집니다. | 설정 쓰기를 직렬화하고, 직렬화한 크기를 읽기 상한으로 검사합니다. 왕복과 교차 쓰기를 테스트합니다. |
| TD-40 | NEW | 4 테스트 | `crates/secs-service/tests/measure_ipc.rs:1170` | Medium | S | 이름이 약속한 것을 확인하지 않는 테스트가 셋입니다. ① `session_free`가 락이 아니라 HTTP 요청 도착만 봅니다. ② 프로젝트 없는 측정 핸들러 테스트가 성공 응답이면 `continue`합니다(`tests/service.rs:1322`). ③ F1 테스트가 다른 대상 `blocker/project.secs`의 저장을 실패시킵니다(`secs-project/tests/record.rs:446`). | 각 테스트가 이름대로 단언하게 바꿉니다. 전송을 멈춘 동안 `ProjectInfo` 완료를 확인하고, 요청별 기대 코드를 쓰고, 같은 대상에 실패를 주입합니다. |
| TD-41 | NEW | 4 테스트 | `features.toml:1030` | Medium | S | `ipc.project_new`, `project_open`, `project_info`, `calibrate`, `recommend_xo`, `mimo`, `results`, `job_cancel`, `jobs`, `rew_launch`가 성공 동작을 확인하지 않는 공통 테스트 둘을 가리킵니다. 둘은 메서드 이름 왕복(`secs-types/src/ipc.rs:2182`)과 잘못된 요청 거절(`tests/service.rs:359`)입니다. 핸들러를 망가뜨려도 등록부의 근거는 통과합니다. | 각 항목을 실제 동작을 확인하는 기존 통합 테스트에 연결하고, 없는 성공 경로만 작게 더합니다. |
| TD-42 | NEW | 6 자원 | `crates/secs-project/src/pyjson.rs:212` | Medium | S | JSON 객체의 키마다 앞선 키를 선형 탐색합니다. 키가 많은 `.secs`를 읽으면 O(n²)입니다. | 삽입 순서 벡터는 두고 키 → 인덱스 표로 조회합니다. 중복 키 의미(마지막 값, 첫 위치)를 테스트합니다. |
| TD-43 | NEW | 8 보안 | `crates/secs-project/src/record.rs:408` | Medium | S | `load_project_sources`가 `.secs`의 IR 경로를 직접 조합해 `.src.json`을 읽고 `confined_ir_path`(F5)를 거치지 않습니다. 저장과 mdat 내보내기가 부릅니다(`secs-service/src/export.rs:117`, `session.rs:249`). | IR 경로와 사이드카 경로 모두에 F5를 적용합니다(아키텍처 리뷰 C6). 테스트를 둡니다. |
| TD-44 | NEW | 6 자원 | `crates/secs-project/src/record.rs:542` | Medium | S | IR 캐시는 항목이 하나 남으면 축출을 멈춰서, 상한보다 큰 단일 IR을 계속 보관합니다(ADR 0007의 바이트 상한과 다름). 적중해도 `order`를 갱신하지 않아 LRU가 아니라 삽입 순서로 축출합니다(`:497`). | 상한보다 큰 항목은 반환만 하고 넣지 않습니다. 적중한 키를 최근 위치로 옮깁니다. 두 경우를 테스트합니다. |
| TD-45 | NEW | 3 계약 | `crates/secs-camilla/src/lib.rs:572` | Medium | S | FIR은 f64가 유한한지만 검사하는데, WAV는 f32로 줄입니다(`:344`). 그래서 `1e100` 같은 계수가 검사를 통과하고 WAV에서는 무한대가 됩니다. APO도 같은 `wav_bytes`를 씁니다. | f32로 바꾼 값까지 미리 검사하고, 저율 FIR은 펼친 결과도 검사합니다. |
| TD-46 | NEW | 3 계약 | `crates/secs-camilla/src/lib.rs:552` | Medium | S | Camilla 사슬 검증은 중복 역할을 거부하지 않아 YAML 중복 키나 계수 덮어쓰기가 생깁니다. `secs-filters`는 같은 입력을 거부합니다(`lib.rs:1867`). | Camilla도 역할 유일성과 예약 이름을 검사합니다. 장기적으로는 아키텍처 리뷰 C11의 검증 한 벌로 모읍니다. |
| TD-47 | NEW | 4 테스트 | `crates/secs-project/tests/golden.rs:373` | Medium | S | `format.irs_store`의 근거 테스트는 자체 float32 작성기로 만든 WAV만 읽습니다. 지원한다고 한 PCM16/24/32, float64, extensible GUID, valid-bits 분기는 검사하지 않습니다. | 작성기와 독립적인 최소 WAV 바이트 픽스처로 형식별 표본과 레이트, 거부 사례를 테스트합니다. |
| TD-48 | NEW | 7 오류 처리 | `crates/secs-engine/src/xo.rs:160` | Medium | S | 기준 위치에는 메인과 서브가 다 있고 다른 위치에는 메인만 있으면 XO 점수 계산이 `stage_responses(..).expect`에서 패닉합니다(`chain.rs:139`가 `None`을 돌려줌). 잡 레지스트리가 패닉을 잡아 잡 실패로 바꾸지만, 입력 오류로 알려 주지 않습니다. `xo.rs:73`의 설명과도 다릅니다. | 추천 시작 전에 위치별 완전성을 검사하고 위치·역할이 있는 오류를 돌려줍니다. 부분 측정으로 테스트합니다. |
| TD-49 | NEW | 6 자원 | `crates/secs-engine/src/measurement.rs:199` | Medium | M | 취소가 닿지 않는 구간이 셋입니다. ① 측정 준비(검증, onset, FFT) ② BA 버스 지연 탐색(`ba.rs:1414`, 토큰 자체를 받지 않음) ③ RC의 위상 준비, FIR 사다리, 넓은 다듬기, `fit_gd_cuts`(`rc.rs:655`, `fit.rs:276`). M2의 '취소가 1초 안에 멈춤' 조건을 큰 프로젝트에서 보장하지 못합니다. | 내부 경로에 취소 확인을 넘겨 위치, 후보, 반복 경계에서 검사합니다. 목적 함수와 순서는 바꾸지 않습니다. |
| TD-50 | NEW | 3 계약 | `crates/secs-engine/src/rc.rs:213` | Medium | S | 공개 `design`에 `EngineConfig.taps > N_FFT`를 주면 입력 오류 대신 슬라이스 패닉이 납니다. 탭 상한 계약이나 검사가 없습니다. | 진입점에서 탭 범위를 검사해 `EngineError`로 돌려주고 경계값을 테스트합니다. |
| TD-51 | NEW | 3 계약 | `crates/secs-dsp/src/iir.rs:144` | Medium | M | `butter(1, &[1.0, 100.0], Bandpass, 600.0)`처럼 넓은 대역의 홀수 차수 필터에서 실수 극점 둘이 `p.im > 0.0` 필터에 모두 걸러지고, `min_by(..).unwrap()`이 빈 목록에서 패닉합니다. | 대역통과 SOS 짝짓기에 실수 극점 쌍을 처리합니다. SciPy 계수로 대조합니다. |
| TD-52 | NEW | 4 테스트 | `crates/secs-engine/src/tests/mimo_run.rs:138` | Medium | S | `(a-b).abs()`를 `fold(0.0, f64::max)`로 모으면 NaN을 무시해서, 전부 NaN이어도 오차가 0입니다. `tests/mimo_iir.rs:91`, `tests/mimo.rs:110`도 같습니다. | NaN을 무한 오차로 다루는 공통 비교 함수를 씁니다. NaN 배열이 실패하는 테스트를 둡니다. |
| TD-53 | NEW | 4 테스트 | `crates/secs-engine/src/test_support.rs:309` | Medium | S | 하류 crate용 `replay_case`가 ADR 0012의 RC 준비 공유를 반영하지 않습니다. 엔진 자체 재생기는 중복 준비 fit을 제거하고 해시와 호출 횟수를 확인합니다(`tests/design.rs:181-252`). 이 재생기는 mixed 모드에서만 맞고, linear·ultra 케이스에서는 잘못 재생하며 허용오차 무한대로 조용히 주입합니다(`:319-323`). | 엔진 테스트와 같은 정규화와 검사를 적용합니다. `test_support`를 재생 adapter의 유일한 자리로 만듭니다(아키텍처 리뷰 C9). |
| TD-54 | NEW | 7 관측성 | `crates/secs-engine/src/mimo/acceleration.rs:297` | Medium | S | CUDA가 실패하면 오류의 `detail`(호출 이름, 상태 코드)을 버리고 넓은 `ComputeReason`만 남깁니다. GPU를 골랐는데 계속 CPU로 돌아온다는 신고가 와도 원인을 구분할 수 없습니다. | fallback 동작은 그대로 두고 detail을 제한된 진단 필드나 로그에 남깁니다. |
| TD-55 | NEW | 3 계약 | `oracle/common.py:156` | Medium | S | 긴 배열을 나눌 때 마지막 조각이 64개 이하면 그 조각만 인라인으로 남습니다. Rust `Fixture::f64s`는 첫 항목이 descriptor면 나머지도 descriptor라고 봅니다(`secs-oracle/src/lib.rs:147-151`). 그래서 길이 24,001 같은 배열로 새 골든을 만들면 읽을 때 패닉합니다. | 조각을 모두 descriptor로 만들거나 로더가 혼합을 읽게 하나로 맞춥니다. 경계 길이를 테스트합니다. |
| TD-56 | NEW | 6 자원 | `crates/secs-sys/src/wasapi.rs:1855` | Medium | M | WASAPI 녹음은 패킷이 오지 않아도 조회가 성공하는 동안 무기한 기다립니다. 10ms timeout을 성공으로 바꾸고(`:759-763`) 누적 무진행 시간은 보지 않습니다. 재생 대기(`:360-392`)도 같습니다. cpal 쪽에는 2초 stall 검사가 있습니다. | 무진행 누적 제한을 두어 stall을 오류로 돌려줍니다. 가짜 패킷으로 정상 지연과 영구 무진행을 구별하는 테스트를 둡니다. |
| TD-57 | NEW | 6 자원 | `crates/secs-sys/src/wasapi.rs:1511` | Medium | S | 녹음 큐는 endpoint 버퍼 하나만큼 예약하는데, 읽기 루프는 둘까지 쌓도록 허용합니다(`:1868-1884`). 그래서 MMCSS 스레드에서 `VecDeque::extend`가 재할당할 수 있습니다. | 허용 backlog 전체를 세션을 열 때 예약하고 push 전에 용량을 검사합니다. |
| TD-58 | NEW | 7 오류 처리 | `crates/secs-sys/src/wasapi.rs:1361` | Medium | S | shared-mode probe는 시험 초기화가 실패하면 원인과 관계없이 `supported=false`로 돌려줍니다. 장치가 분리되거나 서비스가 멈춰도 "형식 거절"로 보입니다. exclusive probe와 open은 형식 거절만 `UnsupportedFormat`으로 분류합니다(`:1021-1029`, `:1198-1207`). | 같은 오류 분류를 씁니다. 장치 무효화는 probe 오류로 돌려줍니다. |
| TD-59 | NEW | 4 테스트 | `.github/scripts/check_feature_tests.py:53` | Medium | S | 게이트는 "Cargo가 실행하는 테스트"를 검사한다면서 `--list` 결과만 봅니다. 그래서 `#[ignore]` 테스트를 가리켜도 통과합니다. | ignored 목록을 따로 구해 구별하고, 별도 CI 잡에서 도는 테스트는 허용 목록과 잡을 연결합니다. |
| TD-60 | NEW | 4 테스트 | `.github/workflows/ci.yml:497` | Medium | M | 신뢰할 수 없는 `.secs`와 WAV를 읽는 `secs-project`가 Miri 대상에 없고, WAV reader fuzz 대상도 없습니다(`fuzz/Cargo.toml:43-55`). 모든 fuzz 대상에 mdat와 liveproject seed만 들어가서, `project_load_text`는 유효한 `.secs` seed가 없습니다(`ci.yml:533`). CLAUDE.md의 신뢰할 수 없는 파일 규칙이 `secs-project`를 앞으로의 일로 미뤄 두었습니다. | `secs-project`의 순수 파서 테스트를 Miri에 넣고 WAV fuzz 대상을 더합니다. 대상별 seed 목록을 둡니다. |
| TD-61 | NEW | 8 보안 | `.github/workflows/release.yml:66` | Medium | S | 배포 키와 contents 쓰기 권한을 쥔 잡을 포함해 외부 Actions가 모두 움직이는 major 태그입니다(`:171-186`도 같음). CI의 cargo-deny도 판을 고정하지 않습니다(`ci.yml:474`). | 외부 Actions를 검토한 커밋 SHA로 고정하고 주석에 판을 적습니다. release와 publish 잡부터 고정합니다. cargo-deny 판을 고정합니다. |
| TD-62 | NEW | 7 오류 처리 | `.github/scripts/upstream.py:181` | Medium | S | 업스트림 동일성 검사가 `git diff` 실패 코드를 무시하고 stdout이 비었는지만 봅니다. diff 명령이 실패해도 "upstream tree matches"로 통과합니다. | 명령이 실패하면 게이트를 실패시키고 stderr를 출력합니다. 일치, 차이, 실패를 구별하는 테스트를 둡니다. |
| TD-63 | NEW | 3 계약 | `oracle/export_engine_mimo_iir.py:310` | Medium | S | 공통 런처는 exporter에 인자를 넘기지 않는데(`oracle/run.py:58`), 이 exporter는 바로 `sys.argv[1]`을 읽습니다. 문서대로 `oracle/run.py engine_mimo_iir`를 돌리면 `IndexError`로 끝납니다. | 기본 scratch 디렉터리를 쓰거나 런처가 옵션을 넘기게 합니다. 인자 없는 진입을 테스트합니다. |
| TD-64 | NEW | 4 테스트 | `oracle/export_engine_xo.py:92` | Medium | S | 선택적 XO 재개 캐시는 `label.json`이 있는지만 보고 재사용합니다. 업스트림 커밋, exporter, 입력, NumPy·SciPy 판은 확인하지 않습니다. 업스트림을 갱신한 뒤 같은 캐시를 쓰면 옛 추천이 새 골든으로 저장됩니다. | 캐시에 지문을 기록하고 일치할 때만 재사용합니다. |
| TD-65 | NEW | 7 오류 처리 | `apps/secs2r/ui/src/App.tsx:182` | Medium | S | 프로젝트 이주 경고(`migration_warnings`)를 표시하는 화면 소비자가 없습니다. 외부 IR 경로가 F5로 거부된 프로젝트를 열면 어떤 측정이 왜 빠졌는지 알 수 없습니다. F5는 경고 표시를 약속합니다. | 프로젝트 열기 결과의 경고를 지속 영역에 표시하고 코드, 대상, 복구 행동을 번역합니다(키 추가, ko·en 함께). |
| TD-66 | NEW | 3 계약 | `apps/secs2r/ui/src/screens/measure/format.ts:63` | Medium | S | 화면은 빈 문자열만 직접 측정으로 보는데, 서비스는 `"measured"`를 보냅니다(`secs-service/src/measurement.rs:417-424`). 그래서 실제 측정 뒤 출처에 '가져온 파일: measured'가 나옵니다. 모의 서비스는 출처 키를 지워 이를 가립니다. | `"measured"`를 직접 측정으로 정규화하고 모의 서비스와 fixtures도 맞춥니다. 섞인 출처 표시를 테스트합니다. |
| TD-67 | NEW | 7 오류 처리 | `apps/secs2r/ui/src/screens/calibrate/ResultGraph.tsx:307` | Medium | M | 타겟 저장이 실패해도 그래프에는 저장되지 않은 `draft`가 남습니다. `setTargetPoints`가 성공 여부를 돌려주지 않기 때문입니다. 그래서 보이는 타겟과 서비스가 쓰는 타겟이 달라집니다. XO `dragHz`도 같습니다. | 편집 동작이 결과를 돌려주게 하고, 실패하면 정본으로 돌아가거나 미저장 상태와 다시 시도를 보입니다. |
| TD-68 | NEW | 2 일관성 | `apps/secs2r/ui/src/App.tsx:217` | Medium | M | 앱 설정이 세 곳에서 낡은 전체 객체로 써집니다(`App.tsx:212-218`, `useCalibrateScreen.ts:31-38`, `useProjectScreen.ts:26-30`). 겹치면 늦게 도착한 쓰기가 앞선 변경을 되돌립니다(GPU 선택 중 테마 변경). 표시 설정은 저장 전에 화면에 반영하고 실패해도 되돌리지 않습니다(`app/prefs.tsx:53`). 계산 장치 설정은 응답을 정본으로 삼아, 같은 설정 저장에 오류 정책이 둘입니다. | 공통 설정 작성기가 순서를 지키고 최신 정본에 필드 변경만 합칩니다. 실패 정책은 하나로 정합니다(아키텍처 리뷰 C16). 겹친 변경으로 회귀 테스트를 둡니다. |
| TD-69 | NEW | 2 일관성 | `apps/secs2r/ui/src/state/triggers.ts:27` | Medium | S | `project_save`(다른 이름으로 저장)가 결과와 내보내기 폴더의 재조회 트리거에 없습니다(`:27`, `:32`). 저장한 뒤 화면의 기본 내보내기 폴더는 이전 경로를 보이는데 실제로는 새 경로에 씁니다. 결과도 서비스에서 바뀌는데(TD-03) 다시 읽지 않습니다. | `project_save` 뒤 결과와 폴더를 다시 읽습니다. 테스트합니다. |
| TD-70 | NEW | 3 계약 | `apps/secs2r/ui/src/ui/plot/AxisRange.tsx:65` | Medium | S | 축 범위에 `abc`나 범위 밖 숫자를 넣고 Enter나 적용을 누르면 오류 입력을 무시하고 팝오버를 닫습니다. `apply()`가 마지막 유효값만 검사합니다. | 폼이 각 칸의 초안 유효성을 알게 하고, 잘못된 칸이 있으면 제출을 막고 초점을 옮깁니다. |
| TD-71 | NEW | 4 테스트 | `apps/secs2r/ui/src/ipc/mock.ts:550` | Medium | S | 모의 서비스가 서비스와 어긋나 있고 테스트가 그 어긋난 값을 고정합니다. ① MIMO가 든 Equalizer APO 내보내기를 거부하는데 F12는 허용하고, `mock.test.ts:45-56`이 이 거부를 고정합니다. ② '서브만 계산'이 전체 결과를 바꿉니다(`mock.ts:514`, 서비스는 `state.subs`만 바꿈). ③ XO fixture 키가 `front`/`center`인데 계약은 `프론트`/`센터`입니다(`fixtures.ts:309`, `mock.ts:340`, `:374`). 그래서 센터 100 Hz가 화면에서 80 Hz로 보입니다. | 세 곳을 서비스에 맞추고 테스트 기대값을 F12와 서비스 동작으로 바꿉니다. 근본 대책은 아키텍처 리뷰 C13입니다. |
| TD-72 | NEW | 2 일관성 | `apps/secs2r/ui/src/screens/measure/useMeasureScreen.ts:20` | Medium | S | 측정 화면의 취소는 `session.call(job_cancel)`로 나가고 잡 추적기가 그 응답을 처리하지 않습니다. 그래서 앱에서는 '취소 중'(`MeasureScreen.tsx:59`, `:266`)이 뜰 수 없고, 트레이의 취소(`useJobs.ts:59-68`)만 `cancel_requested`를 표시합니다. 서비스가 실제로 멈추려면 스윕 하나가 끝나야 할 수 있습니다. | 취소를 한 경로로 모아 `cancel_requested`를 모든 보기에 반영합니다(아키텍처 리뷰 C12). 측정 화면 취소 테스트를 둡니다. |
| TD-73 | NEW | 5 의존성 | `apps/secs2r/ui/package.json:33` | Medium | S | 개발 의존성에 권고가 있습니다. vite 8.2.1은 GHSA-9jrq-w75r-8gcw, GHSA-rq7h-c2jc-7f22 등 6건(high, 개발 서버)입니다. happy-dom 20.14.5는 GHSA-834v-8h66-3h79(high, 테스트 환경)입니다. vitest 4.1.11과 @vitest/mocker는 GHSA-qxg6-89mg-rqv8 등(moderate)입니다. 출하 의존성(`--omit=dev`)에는 없습니다. | 고친 판으로 정확히 고정해 올립니다(vite 8.2.3 이상의 고친 판, happy-dom 21.0.4, vitest 4.1.12 이상). 테스트와 갤러리 빌드를 돌려 확인합니다. |
| TD-74 | NEW | 2 일관성 | `crates/secs-service/src/project_edit.rs:380` | Medium | M | 임시 파일 후 이름 바꾸기 원자적 쓰기가 여섯 벌입니다. `project_edit.rs:380`, `session.rs:305-342`, `cache.rs:167-231`, `secs-project/src/record.rs:1007`, `secs-camilla/src/lib.rs:1443`(pub), `secs-export/src/lib.rs:792-831`입니다. 재시도, 이름 규칙, 정리 방식이 조금씩 다르고, 정작 IR 저장(TD-15)과 계산 출처 파일(`secs-service/src/export.rs:480`)은 쓰지 않습니다. | 의존 방향에 맞는 낮은 crate 하나에 공용 도구를 두고(경계 결정은 ARCHITECTURE 3절 갱신과 함께) 여섯 곳을 옮깁니다. 크기 제한이 있는 cache 판은 도구에 쓰기 함수를 넘겨 유지합니다. |
| TD-75 | NEW | 1 아키텍처 | `crates/secs-engine/src/mimo/iir.rs:171` | Low | S | MIMO의 같은 도우미가 세 벌씩입니다. 저율 onset(최대치 10%를 처음 넘는 인덱스)은 `iir.rs:171`, `mimo/design.rs:213`, `decay.rs:51`에 있고, 3분의 1 옥타브 경계 생성은 `mimo/design.rs:372`, `:435`, `iir.rs:689`에 있습니다. 하나만 바뀌면 목적 함수와 가드의 창이 갈라집니다. | 각각 내부 도우미 하나로 합치고 비트 동등 테스트를 유지합니다. 규칙이 다른 `measurement::onset_index`는 합치지 않습니다. |
| TD-76 | NEW | 9 문서 | `crates/secs-engine/src/lib.rs:26` | Low | S | crate 문서가 없는 것을 설명합니다. 엔진은 없어진 `SupportFitBackend` seam을 적었고(ADR 0008은 `RidgeBackend`), `secs-compute`는 구현에 없는 지문 LRU 캐시를 적었습니다(`secs-compute/src/lib.rs:10`, `Cargo.toml:3`). `secs-audio`는 이 저장소에 없는 `impulcifer-*` crate를 구현자로 적었고(`audio.rs:4`), `cpal.rs`는 Windows 비교 사용을 적었지만 Windows에서는 빠져 있습니다(`cpal.rs:4`). | 각 문서를 현재 책임과 구현자로 고칩니다. 출처 고지는 유지합니다. |
| TD-77 | NEW | 9 문서 | `crates/secs-types/src/ipc.rs:1705` | Low | S | IPC 타입 문서가 wire와 다릅니다. `LayoutResult.start_delay`는 `off`, `5`…로 적혔지만 서비스는 `끄기`, `5 초`…만 받습니다. `EqualizerApo`는 범위를 RC·BA로 적었지만 F12는 MIMO까지 넣습니다(`:1020`). | 문서를 고치고 바인딩을 다시 생성합니다. |
| TD-78 | NEW | 1 아키텍처 | `crates/secs-project/src/record.rs:981` | Low | S | 같은 파이썬 truthiness 판정이 `project.rs:88`, `plan.rs:366`, `calibration.rs:322`, `record.rs:981` 네 곳에 있습니다. | `PyJson`의 내부 함수 하나로 옮깁니다. |
| TD-79 | NEW | 9 문서 | `crates/secs-rew/PARITY.md:165` | Low | S | R1이 `/measure/command`에만 적용된다고 적혀 있지만 실제로는 모든 경로의 401-403에 적용됩니다(`client.rs:161`). Java 출력 경로도 `outputs`/`output`으로 적혀 있지만 코드는 `output-channels`/`output-channel`입니다(`:32`, `client.rs:333-336`). | 문서와 86행의 테스트 이름을 현재 구현과 편차 대장에 맞춥니다. |
| TD-80 | NEW | 9 문서 | `crates/secs-sys/src/wasapi.rs:736` | Low | S | 진단 환경 변수 `SECS_PA05_SHARED_PERIODS`(2·3·4, 기본 4)와 `SECS_PA05_TRACE_PREFIX`(절대 경로)가 코드 밖 어디에도 없습니다. trace 파일 생성과 쓰기 실패도 버립니다(`:719`). | `docs/HARDWARE.md`에 용도, 허용값, 기본값, 해제 방법을 적고, trace 오류는 stderr에 남깁니다. |
| TD-81 | NEW | 1 아키텍처 | `oracle/common.py:201` | Low | S | `Recorder`, 전역 `REC`, `interpose`는 정의만 있고 쓰이지 않습니다. 그런데 `docs/ENGINE.md:35`는 이것을 공통 관찰 경로로 설명합니다. | 지우고 문서가 실제 관찰 방식을 가리키게 합니다. |
| TD-82 | NEW | 9 문서 | `docs/ARCHITECTURE.md:19` | Low | S | 현행 문서에 낡은 값이 남아 있습니다. ① `secs-sys` unsafe 예산이 0으로 적혀 있지만 실제는 3이고 ADR 0011이 승인했습니다(`ARCHITECTURE.md:19`, `CLAUDE.md:9`, `CONTEXT.md:49`). ② 캐시를 v4로 설명하지만 실제는 v5입니다(`DEVIATIONS.md:97`의 S1, `features.toml:862`, `cache.rs:10`). ③ 신호 사슬이 MIMO 경로까지 담는다고 설명하지만 코드의 `SignalChain`에는 없습니다(`CONTEXT.md:33`, `ARCHITECTURE.md:107`, `secs-types/src/chain.rs:121-137`, `:218`). ④ README 한국어 '지금 상태'에 Dirac 가져오기가 빠졌습니다(`README.md:9`, 영어 절에는 있음). | 현행 값으로 고치고 ADR 0011과 0012를 연결합니다. ADR 본문은 역사로 둡니다. |
| TD-83 | NEW | 6 자원 | `apps/secs2r/ui/src/screens/connect/useConnectScreen.ts:61` | Low | S | `level_play` 요청 전에 만든 종료 이벤트 구독과 8초 타이머가, 요청이 실패해도 바로 해제되지 않습니다. | 해제 함수를 지역 변수로 두고 `finally`에서 부릅니다. |
| TD-84 | NEW | 1 아키텍처 | `apps/secs2r/ui/src/screens/calibrate/settingsView.ts:42` | Low | S | 쓰이지 않는 화면 코드가 있습니다. `SUB_FILTER_DEFAULT_HZ`, `DEFAULT_XO_HZ`, `FILTER_KEYS`, `bundleMembers`, `MeasureScreen.tsx:563` `projectFileName`, `HOST_TRIGGERS.runs`·`.selected`(`state/triggers.ts:40-41`), `session.waitForJob`(`session.ts:177-192`, 테스트만 씀), `useProjectScreen.restartTutorial`(`:89-92`), `jobStore.isActive`(별칭)입니다. knip은 테스트를 진입점으로 넣어도 쓰이지 않는 내보내기 38개와 내보낸 타입 25개를 보고합니다(`ipc/fixtures.ts` `staleResults`, `exportResult`, `screens/calibrate/fixtures.ts` 12개 등). | 쓰이지 않는 것을 지우고, 같은 파일 안에서만 쓰는 것은 export를 뗍니다. 대체 추상은 만들지 않습니다. |
| TD-85 | NEW | 9 문서 | `apps/secs2r/ui/src/screens/measure/types.ts:31` | Low | S | 이미 연결된 IPC를 "service gap"이라고 설명하는 주석이 남아 있습니다. `measure/types.ts:31`, `calibrate/types.ts:81-88`, `export/types.ts:35-39`입니다. | 해소된 주석을 지우고 현재 응답 필드를 적습니다. `PositionRecord`는 생성 바인딩을 다시 내보냅니다. |
| TD-86 | NEW | 9 문서 | `apps/secs2r/ui/src/i18n/ko.json:1175` | Low | S | 카탈로그 두 곳이 틀렸습니다. ① 루프백 문제 안내가 연결·점검에서 루프백 입력 채널을 고르라고 하지만 그 조작이 없습니다. ② 수동 REW 실행 안내의 `‑api`가 U+2011입니다(`ko.json:691`). 둘 다 en도 같습니다. | 두 카탈로그를 함께 고칩니다. 실행 옵션의 유사 하이픈을 검사에 더합니다. |
| TD-87 | NEW | 5 의존성 | `apps/secs2r/ui/scripts/build-fonts.py:15` | Low | S | 출하 글꼴을 다시 만드는 도구(`fonttools[woff]`, Brotli) 판과 Orbit 원본 리비전·해시가 기록되어 있지 않습니다. | 도구 판과 원본 해시를 기록합니다. 글꼴 바이트는 바꾸지 않습니다. |
| TD-88 | NEW | 5 의존성 | `crates/secs-dsp/Cargo.toml:18` | Low | S | `cargo machete`가 `pulp`, `gemm`, `faer-traits`를 쓰지 않는 의존성으로 보고합니다. 이유는 워크스페이스 `Cargo.toml:56`에 있지만 이 manifest에는 없어서 다음 감사도 같은 경고를 받습니다. | `[package.metadata.cargo-machete] ignored`에 셋을 넣고 워크스페이스 주석을 가리키는 한 줄을 적습니다. |
| TD-89 | NEW | 1 아키텍처 | `crates/secs-camilla/src/lib.rs:63` | Low | S | 쓰이지 않는 옵션이 둘입니다. `Options.fsiso`는 검증(`:522`)만 하고 쓰지 않습니다. `MemoryBudget::from_probe`는 쓰이지 않고, 서비스가 같은 값을 따로 계산합니다(`secs-service/src/calibration.rs:304-312`). | 필드를 지우거나 쓰는 쪽으로 정리하고, 서비스가 `from_probe`를 쓰게 합니다. |
| TD-90 | NEW | 7 오류 처리 | `crates/secs-service/src/session.rs:96` | High | M | 튜토리얼은 `path: ""`로 프로젝트를 만듭니다(`apps/secs2r/ui/src/App.tsx:196`, `useProjectScreen.ts:46`). `.secs`는 저장하지 않지만, 측정 IR과 결과 캐시는 그 빈 경로를 기준으로 씁니다. 그래서 `secs_run_cache.json`과 IR WAV가 **앱의 현재 작업 폴더**에 생깁니다(`cache.rs:258-260`, `secs-project/src/irs.rs:8-11`). 작업 폴더에 쓸 수 없으면 튜토리얼의 보정이 캐시 저장 실패로 끝납니다. `ProjectNewParams.tutorial`의 설명 "non-writable"(`secs-types/src/ipc.rs:622`)도 실제 계약(저장·내보내기만 막음, P36 14번)과 다릅니다. | 튜토리얼 세션마다 비공개 임시 폴더(`temp_dir()/secs2r-tutorial-<pid>-<n>/tutorial.secs`)를 쓰고, 프로젝트를 닫거나 바꿀 때 지웁니다. 타입 문서를 실제 계약으로 고칩니다. 작업 폴더를 임시 폴더로 바꾼 테스트에서 튜토리얼 측정과 보정 뒤 작업 폴더가 비어 있는지 확인합니다. |
| TD-91 | NEW | 3 계약 | `crates/secs-service/src/measurement.rs:380` | Medium | S | `rew_driver_set`과 장치를 바꾸는 `measure_configure`(`:908`)는 측정 잡이 REW로 스윕하는 중에도 REW의 드라이버와 장치를 바꿉니다. 화면이 막는 것에만 기대고 있습니다. | 측정 무리의 잡(측정, 샘플 불러오기, 가져오기)이 도는 동안에는 `JobBusy`로 거부합니다. 레벨 모니터는 화면이 다시 시작하므로 허용합니다. IPC 테스트로 확인합니다. |
| TD-92 | NEW | 3 계약 | `crates/secs-project/src/calibration.rs:84` | Medium | M | ADR 0004의 파일 호환 조항은 모르는 키를 순서째 보존하라고 정합니다. 그런데 `SpeakerConfig`, `DspProfile`, `TargetCurve`의 변환은 모르는 중첩 키를 버리고, 설정을 저장할 때는 `calibration` 객체를 통째로 바꿉니다. P02(108-110행)는 최상위와 전달용 객체만 적었지만, 상위 결정은 ADR입니다. 그래서 더 새로운 SECS 2가 쓴 중첩 키를 SECS 2R이 저장하면 잃습니다. | 각 구조체에 순서를 지키는 모르는 키 보관 칸을 두고 되씁니다. 모르는 중첩 키가 든 `.secs`가 바이트 순서째 왕복하는 테스트를 둡니다. 모르는 키가 없는 파일의 골든 바이트는 그대로입니다. |
| TD-93 | NEW | 7 오류 처리 | `crates/secs-project/src/irs.rs:132` | Medium | S | WAV 읽기는 선언된 data 청크 끝을 실제 파일 끝으로 줄입니다. 그래서 남은 길이가 표본 크기의 배수이면, 잘린 IR도 짧은 IR로 조용히 받아들입니다(업스트림 `session.py:84`도 같음). 저장 중에 끊긴 파일이 오류 없이 다른 측정이 됩니다. | 선언 길이가 실제보다 길면, 파일과 두 길이를 담은 오류로 거부합니다. 편차 번호를 붙여 `docs/DEVIATIONS.md`에 적고 테스트합니다. |
| TD-94 | NEW | 3 계약 | `crates/secs-filters/src/lib.rs:941` | Medium | S | 음수 지연을 두 작성기가 다르게 처리합니다. CamillaDSP는 음수 `Delay`로 쓰고, Equalizer APO는 조용히 뺍니다. 캐시에서 되살린 사슬에는 엔진 같은 검증이 없습니다. `plan_responses`(`:589`)는 받은 `f`가 아니라 그 길이로 만든 균등 격자에서 FIR 응답을 계산하는데, 이 전제가 interface에 없습니다. | 두 작성기 모두 유한하지 않거나 음수인 지연을 첫 쓰기 전에 거부합니다. `plan_responses`는 격자 전제를 문서와 검사로 드러냅니다. |
| TD-95 | NEW | 9 문서 | `docs/ARCHITECTURE.md:58` | Low | S | 의존 방향 그림에 실제 의존 둘이 빠졌습니다. `secs-project → secs-mdat`(source info, `crates/secs-project/Cargo.toml:15`)와 `secs-export → secs-camilla`(숫자 포매터, `crates/secs-export/Cargo.toml:16`)입니다. | 그림에 두 선과 이유를 더합니다. |
| TD-96 | NEW | 6 자원 | `crates/secs-engine/src/mimo/acceleration.rs:84` | Medium | S | `RidgeSweep::new`는 분해 전후에만 취소를 확인하고, Householder 분해(`mimo/solve.rs:36`) 안에는 확인 지점이 없습니다. 공개 요청은 720탭까지 받으므로, 큰 입력에서 M2의 '취소가 1초 안에 멈춤'을 보장하지 못합니다. | 분해의 열 반복마다 취소를 확인합니다(산술 순서는 그대로). 취소하면 `Cancelled`가 돌아오는지 테스트합니다. |
| TD-97 | NEW | 6 자원 | `crates/secs-dsp/src/fft.rs:18` | Low | S | 스레드 로컬 FFT planner(realfft의 무제한 `HashMap`)가 길이마다 계획을 계속 보존합니다. 리샘플링처럼 원본 길이로 FFT를 하는 경로가 있어서, 오래 켜 둔 앱에서는 길이 종류만큼 스레드마다 쌓입니다. | 스레드마다 최근에 쓴 길이 몇 개만 남기는 상한을 둡니다(결과는 그대로). 길이를 바꿔 가며 부를 때 보관 수가 상한을 넘지 않는지 테스트합니다. |
| TD-98 | NEW | 9 문서 | `docs/UNSAFE_BOUNDARIES.md:52` | Low | S | 문서는 장치 선택이나 동기화가 실패하면 Drop이 그냥 돌아간다고 적었습니다. 그런데 `Session::drop`(`crates/secs-cuda-sys/src/native.rs:761`)은 stream 동기화 오류를 무시하고 핸들을 파괴합니다. 이것은 CUDA의 파괴 API 계약(작업이 남은 stream의 파괴는 완료 뒤에 자원을 놓음)에 맞지만, 문서는 버퍼만 설명합니다. | 버퍼와 Session을 나눠 각각의 정책과 CUDA 문서 근거를 적습니다. |
| TD-99 | NEW | 8 보안 | `crates/secs-sys/src/taskbar.rs:26` | Medium | S | `CreateIcon`의 AND mask는 행마다 2바이트 단위로 맞춘 단색 비트맵이라 `ceil(w/16)·2·h`바이트가 필요합니다. 코드는 `w·h`바이트만 할당하고, 공개 API(`secs-sys/src/lib.rs:56`)는 너비 1을 허용합니다. 너비 1이면 Windows가 버퍼 밖을 읽습니다(앱은 16px 이상만 넘김). | mask 길이를 규칙대로 계산하고, 그 계산을 단위 테스트합니다. unsafe 개수는 그대로입니다. |
| TD-100 | NEW | 2 일관성 | `apps/secs2r/ui/src/ui/maps/RoomMap.tsx:99` | Low | M | 배치도의 스피커 누름 영역이 지름 36px(`r=18`)로, 화면 규칙의 40px보다 작습니다. 다른 보이지 않는 누름 영역도 '별도 PR'로 미뤄져 있습니다(`docs/skins/README.md:51`). | 겉모습을 바꾸지 않는 보이지 않는 누름 영역부터 40px로 넓힙니다. 세 스킨에서 확인합니다. |
| TD-101 | NEW | 3 계약 | `apps/secs2r/ui/src/screens/project/useProjectScreen.ts:20` | Medium | S | 저장 안 한 변경을 확인하는 guard가 `project.dirty`를 읽습니다. 그런데 이 값은 편집 요청의 응답과 재조회가 끝난 뒤에야 바뀝니다. 그래서 편집 직후 바로 프로젝트를 열거나 닫으면 확인 대화상자 없이 변경을 잃을 수 있습니다. | 진행 중인 프로젝트 편집 요청이 끝나기를 기다린 뒤 판정합니다. 지연 응답 테스트로 확인합니다. |
| TD-102 | NEW | 6 자원 | `apps/secs2r/ui/src/state/session.ts:233` | Low | S | 모니터 종료를 기다리는 구독(`:233-242`)에 시간 제한이 없습니다. 완료 이벤트를 잃으면 뒤따르는 모니터 큐와 프로젝트 작업이 계속 기다립니다. | 시간 제한을 두고, 넘으면 `jobs`를 다시 읽어 상태를 되맞춥니다. 가짜 타이머로 테스트합니다. |
| TD-103 | NEW | 9 문서 | `docs/DESIGN.md:10` | Low | S | DESIGN 1절은 화면마다 채운 기본 동작 하나를 정합니다. 보정 결과의 '내보내기'는 `go` 변형으로 둘째 채운 단추이고(`screens/calibrate/Results.tsx:96`), 그 이유는 `ui/Button.tsx:7`에만 있습니다. | DESIGN 1절에 `go`의 예외와 조건(결과 바로 옆에서 다음 단계로 넘어감, 색으로 주 동작과 구별)을 적습니다. |
| TD-104 | NEW | 3 계약 | `crates/secs-engine/src/mimo/iir.rs:953` | Low | S | `Tuning`의 `mimo_k_bands`·`mimo_a_bands` 상한을 검사하지 않습니다. 검사를 통과한 아주 좁은 gain/Q 범위는 `:275`의 양쪽 `1e-6` 여유를 뒤집을 수 있습니다. 앱은 기본값만 쓰지만 공개 API입니다. | 엔진 입구에서 `Tuning`의 범위를 검사하고 `EngineError::Input`으로 돌려줍니다. 경계값을 테스트합니다. |

## Top 5 — 다른 것은 못 고쳐도 이것은 고칩니다

1. **TD-01 + TD-10: MIMO와 보정의 낡음을 실행 지문으로 판정합니다.** 잘못된 보정이 출하되는 경로입니다.
   ```rust
   // secs-service/src/calibration.rs
   pub(crate) struct RunFingerprint { signature: String, target_key: String, engine_key: String, config_key: String }
   // CalibrationRun에 fingerprint를 두고, MimoResult.calibration_key: String 대신
   // MimoResult.calibration: RunFingerprint를 저장합니다.
   // live_mimo.stale = calibration.stale || run.fingerprint != result.calibration || ...
   // cache.rs v5에 mimo_calibration_fingerprint를 더하고(없으면 낡음), config_key는 project_groups 직렬화.
   ```
   측정 교체 → 재보정 → MIMO 낡음, 채널 삭제 → 보정 낡음 테스트 두 개를 먼저 씁니다. 길게 보면 아키텍처 리뷰 C1의 결과 장부가 이 판정의 자리입니다.
2. **TD-02: Dirac IR에 내용 지문을 줍니다.** `dirac_import.rs:280`의 `uuid: None`을 `Some(format!("dirac-{}", hex(sha256(f32 bytes ‖ rate ‖ t0))))`로 바꿉니다. 같은 파형을 다시 가져오면 같은 값이 나오므로 쓸데없이 낡음이 되지 않습니다.
3. **TD-03 + TD-04: 다른 이름으로 저장을 '준비 → 교체'로 바꿉니다.**
   ```rust
   // session.rs save()
   reject_nested(&canonical(old_irs), &canonical(new_irs))?;
   let staged = stage_irs(&old_irs, &new_irs)?;        // 임시 폴더에 완성
   let mut next = MeasurementStore::new(path.clone(), rewritten_project);
   next.save()?;                                        // 실패하면 open은 그대로
   staged.commit()?;                                    // rename
   open.path = path; open.store = next;
   open.cached = crate::cache::load(&open.path);        // 버리지 않음
   ```
4. **TD-05 + TD-06: REW 레벨 재생의 생애를 고칩니다.** `requested.filter(|p| p.generation != handled_generation && active.as_ref().is_none_or(|(a, _, _)| a.generation != p.generation))`로 다시 시작을 막습니다. 본문을 `let result = (|| { ... })();`로 감싸 결과와 무관하게 `stop_generator`/`stop`을 부릅니다. 자동 종료 테스트에 Play 횟수 단언을 더합니다.
5. **TD-17 + TD-16: 쓰기 경로를 폴더 안에 가둡니다.** Camilla 식별자는 `[A-Za-z0-9_-]+`만 허용하고, 모든 계수 경로가 `coefficient_dir.join(name)`이며 `name`에 구분자가 없음을 쓰기 전에 단언합니다. `save_ir`는 `confined_ir_path`를 통과한 경로에 `symlink_metadata`로 링크가 아닌지 확인하고 TD-74의 원자적 쓰기를 씁니다.

## 빠른 수정 (노력 S, 심각도 Medium 이상)

- [ ] TD-02 Dirac IR 내용 지문
- [ ] TD-03 Save As 뒤 cached 유지
- [ ] TD-05 REW 레벨 재생 다시 시작 막기
- [ ] TD-06 REW 모니터 정리 가드
- [ ] TD-07 레벨 모니터 identity 확인
- [ ] TD-12 CLI 독립 mimo
- [ ] TD-13 `sample_load.positions` 상한
- [ ] TD-14 서브만 계산한 BA 표
- [ ] TD-15 IR WAV 원자적 쓰기
- [ ] TD-20 `spl_offset` 형식 검증
- [ ] TD-25 `assert_close` 무한대
- [ ] TD-26 릴리스 게이트 push·fetch 실패
- [ ] TD-29 설정 조회 실패 화면
- [ ] TD-31 출력 번호 상한
- [ ] TD-32 MIMO 상한 문구
- [ ] TD-33 곡선의 계산 환경 키
- [ ] TD-34 모니터 오류 분류
- [ ] TD-35 REW 피크 값
- [ ] TD-36 CLI `--position`
- [ ] TD-38 캐시 저장 실패 순서
- [ ] TD-39 앱 설정 저장 직렬화와 크기
- [ ] TD-40 이름대로 확인하지 않는 테스트 셋
- [ ] TD-41 `features.toml` IPC 근거
- [ ] TD-42 pyjson 키 표
- [ ] TD-43 사이드카 F5
- [ ] TD-44 IR 캐시 정책
- [ ] TD-45 FIR f32 범위
- [ ] TD-46 Camilla 중복 역할
- [ ] TD-47 WAV 형식 픽스처
- [ ] TD-48 XO 부분 측정 오류
- [ ] TD-50 탭 상한
- [ ] TD-52 NaN 오차 비교
- [ ] TD-53 하류 재생 adapter
- [ ] TD-54 CUDA detail
- [ ] TD-55 골든 조각 계약
- [ ] TD-57 WASAPI 큐 예약
- [ ] TD-58 WASAPI probe 분류
- [ ] TD-59 ignored 테스트 게이트
- [ ] TD-61 Actions SHA 고정
- [ ] TD-62 upstream diff 실패
- [ ] TD-63 오라클 런처 인자
- [ ] TD-64 XO 재개 캐시 지문
- [ ] TD-65 이주 경고 표시
- [ ] TD-66 `"measured"` 출처
- [ ] TD-69 `project_save` 트리거
- [ ] TD-70 축 범위 제출
- [ ] TD-71 모의 서비스 어긋남 셋
- [ ] TD-72 측정 화면 취소 표시
- [ ] TD-73 개발 의존성 권고
- [ ] TD-91 측정 중 REW 장치·드라이버 변경 거부
- [ ] TD-93 잘린 WAV data 청크 거부
- [ ] TD-94 음수 지연 거부와 `plan_responses` 격자
- [ ] TD-96 Householder 분해 취소
- [ ] TD-99 작업 표시줄 아이콘 AND mask 크기
- [ ] TD-101 저장 안 한 변경 확인 경쟁

## 이상해 보이지만 맞는 것

기준선이 없으므로 모두 새 항목입니다. 각 항목은 그 자리의 근거를 함께 적습니다.

- `crates/secs-engine/examples/engine_perf.rs:5` — 엔진 소스 약 20개를 `#[path]`로 다시 컴파일합니다. 근거: 5행 주석("benchmark APIs를 라이브러리에 넣지 않고 같은 비공개 단계를 컴파일"), ADR 0007의 커널별 벤치. 래퍼를 줄이는 일은 아키텍처 리뷰 C10입니다.
- clippy pedantic의 수치 경고(형 변환 약 900건, `f64::midpoint` 권고 37건 등) — 업스트림 NumPy와 같은 부동소수점 연산 순서를 지키려는 것이고, `midpoint`는 반올림이 달라질 수 있습니다. 근거: ADR 0004, ARCHITECTURE 6절.
- `crates/secs-dsp/Cargo.toml:18`의 직접 쓰지 않는 `pulp`·`gemm`·`faer-traits` — faer의 `std` 없이 런타임 SIMD 감지를 유지합니다. 근거: 워크스페이스 `Cargo.toml:56` 주석, ADR 0007. 도구 설정은 TD-88입니다.
- `cargo audit` 경고 7건(`glib`, `tokio` unsound, `zbus_names`·`zvariant` 등 unmaintained) — 모두 Tauri의 Linux 스택을 거친 전이 의존성입니다. 근거: `deny.toml`의 `unmaintained = "workspace"`와 그 주석.
- `crates/secs-sys/src/wasapi.rs:168`과 `:160` — 첫 패킷 discontinuity와 위치 점프를 녹음 손실로 보지 않습니다. 근거: `first_packet_discontinuity_is_start_transition_not_gap`(2223), `position_jump_is_traced_but_not_a_gap`(2281), `CHANGELOG.md:210`.
- `crates/secs-sys/src/wasapi.rs:425` — shared 버퍼 네 period. 근거: 같은 자리의 두·세 period 반복 실측 underrun 기록.
- `crates/secs-sys/src/taskbar.rs:30` — OS 격리 crate의 unsafe 세 블록. 근거: `unsafe-budget.toml:10`(3), ADR 0011, `docs/UNSAFE_BOUNDARIES.md`. 문서의 0은 TD-82입니다.
- `crates/secs-sys/src/cpal.rs:554` — overflow 때 프레임 전체를 버리고 discontinuity를 올립니다. 근거: `:746` 테스트.
- `.github/workflows/release.yml:20` — bump 커밋을 배포 키로 main에 넣고 `[skip ci]`를 붙입니다. 근거: ADR 0006, PR #91.
- `.github/scripts/eqapo_engine.py:75` — CI가 외부 실행 파일을 받습니다. 근거: 압축 해제 전 SHA-256 확인, `.github/eqapo-engines.toml`의 판·해시 고정, 경로 거절 테스트.
- `crates/secs-filters/tests/eqapo_playback.rs:745` — 실제 엔진 재생 테스트가 `#[ignore]`입니다. 근거: 이유 주석과 `ci.yml:262`의 `--ignored` 잡.
- `oracle/export_engine_fit.py:38`(pickle), `oracle/export_service_settings_edit.py:31`(AST 실행) — 부모 exporter가 만든 작업 목록과 고정 업스트림 파일만 다룹니다. 근거: 각 파일 머리의 목적·출처, `:304-305`.
- `crates/secs-service/src/jobs.rs:27` — 보정, XO, MIMO가 계산 무리 하나를 공유합니다. 근거: `docs/M4.md:86` 이후의 무리 계약.
- `crates/secs-service/src/export.rs:41` — 낡은 결과의 내보내기를 막지 않고 `stale`로 알립니다. 근거: P36 69행, `tests/service.rs:900`.
- `crates/secs-service/src/measurement.rs:645` — REW는 채널마다 저장하고 취소 뒤에도 완료 채널을 남깁니다. 근거: F2, `tests/measure_ipc.rs:943`. 이 결정을 서비스가 method 문자열로 내리는 것은 아키텍처 리뷰 C5입니다.
- `crates/secs-service/src/measure/settings.rs:262`, `measure/rew.rs:187` — U15, D38. 근거: `tests/measure.rs:392`, `:234`.
- `crates/secs-service/src/host.rs:53` — `open_folder`가 없는 폴더를 만듭니다. 근거: P36 13번, `tests/host.rs:135`.
- `apps/secs2r/tauri.conf.json:24`, `capabilities/default.json:6` — CSP와 권한이 좁습니다(script self, connect IPC, 파일·셸 권한 없음).
- `apps/secs2r-cli/src/main.rs:477` — `check`가 `secs-compute`와 rayon을 직접 부릅니다. 근거: P25 명령 표 74행.
- `crates/secs-jos/src/parser.rs:44`, `:148`, `writer.rs:13` — 작업 스택, 서러게이트 원본 보존, infallible 편의 API. 근거: F3·F7·F8, `tests/untrusted.rs`, `try_write`.
- `crates/secs-dirac/src/lib.rs:153`, `:369`, `tests.rs:539` — 암호화된 뒤쪽 상태를 검증하지 않음, 누적 표본 예산, 실제 표본 테스트 ignore. 근거: ADR 0013, `docs/DIRAC_LIVEPROJECT.md:81`, `:88`.
- `crates/secs-rew/src/client.rs:161`, `:709` — 경로와 무관한 401-403 분류(R1), abort 계열 best-effort. 근거: `DEVIATIONS.md:65`, `PARITY.md:15-19`. 문서 표류는 TD-79입니다.
- `crates/secs-audio/src/calibration.rs:7`, `level.rs:214` — 마이크 보정을 IR에 합치지 않고, 스트리밍 레벨은 미완료로 공개합니다. 근거: 파일 머리의 REW 규격 링크, `features.toml:894-899`.
- `crates/secs-camilla/src/lib.rs:1055` — MIMO 지연을 정수 샘플로 반올림한 뒤 공통분을 나눕니다. 근거: ADR 0012, F13.
- `crates/secs-project/src/record.rs:252` — SNR을 메모리에만 두고 `MeasurementResult.source`를 쓰지 않습니다. 근거: `tests/record.rs:79`, `:151`, `mdat_import.rs:443-445`.
- `crates/secs-project/src/project.rs:337` — 알려진 최상위 키를 SECS 2의 저장 순서로 정규화합니다. 근거: `features.toml:421`, P02 101행.
- `crates/secs-engine/src/ba.rs:567`, `:1429`, `secs-dsp/src/lowrate.rs:13`, `secs-engine/src/rc.rs:232` — 쓰이지 않는 올패스 차원, 코히런트 합 벌점, 보간 투영, DC 항. 근거: ADR 0012 30행, D11, D1, D3, D4.
- `crates/secs-engine/src/mimo/iir.rs:369`, `mimo/run.rs:281`, `mimo/acceleration.rs:287` — 중앙차분, IIR의 CPU 고정, GPU 실패 시 탐색 전체 재실행. 근거: ADR 0012 34행, ADR 0008, `docs/perf/gpu.md:52`, `acceleration.rs:417-437`.
- `crates/secs-cuda-sys/src/native.rs:603`, `:863`, `:1020` — 공개 factor 경계 검사, 동기화 실패 버퍼를 해제하지 않음, 네이티브 테스트 기본 제외. 근거: `docs/UNSAFE_BOUNDARIES.md:7`, `:50`, `docs/perf/gpu.md:89`.
- `apps/secs2r/ui/src/state/store.ts:13-25`, `state/jobTracker.ts:78-106` — 세대 번호로 낡은 조회를 버리고, 시작 응답보다 먼저 온 완료를 보관합니다. 근거: `session.test.ts:23-27`, `sessionJobs.test.ts`, ADR 0009.
- `apps/secs2r/ui/src/screens/connect/ConnectScreen.tsx:432-434`, `screens/measure/MeasureScreen.tsx:516` — REW 입력 채널 읽기 전용, 스윕 반복은 REW만. 근거: R7, U15.
- `apps/secs2r/ui/src/styles/skins/matrix.css:1514-1520` — 스킨이 캡션을 숨기고 방향 기호로 대신합니다. 근거: `docs/skins/README.md:20`, `docs/DESIGN.md:256`, ADR 0010.
- `apps/secs2r/ui/vite.config.ts:47-50` — 갤러리 진입점은 앱 빌드에서 빠집니다. 근거: `package.json:11`의 `--mode app`, `docs/DESIGN.md:285`. knip이 갤러리 파일을 쓰이지 않는다고 본 것은 진입점을 몰라서입니다.

- `crates/secs-service/src/curves.rs:519` — [결과 값 가져오기]가 서브만 계산한 결과가 아니라 전체 보정 결과에서 값을 가져옵니다. 근거: 업스트림이 `self.settings.take_sub_values(self.run)`으로 같은 일을 합니다(`src/secs/wizard/step_calibration.py:1316`). BA 탭 표시만 `sub_run`을 우선합니다(`:2320`).
- `apps/secs2r/ui/src/screens/calibrate/Settings.tsx:565-573` — 서브가 한 대면 BA 카드의 고정 필터와 MIMO 서포트 선택까지 숨깁니다. 근거: 업스트림도 서브가 두 대 미만이면 서브 카드를 숨깁니다(`step_calibration.py:1323`, `:2142`).
- `crates/secs-service/src/measure/native.rs:141` — 자체 측정이 마이크 보정을 레벨 확인에만 넘기고 IR에는 합치지 않습니다. 근거: `crates/secs-audio/src/calibration.rs:7-9`의 계약(REW처럼 측정 IR은 그대로 두고 곡선만 넘김)이고, 업스트림도 보정 파일 이름만 출처 정보로 남깁니다(`src/secs/rewapi/rew.py:766`, `:902`).
- `crates/secs-engine/src/mimo/design.rs:922` — 서포트 후보가 없으면 MIMO 전체를 입력 오류로 끝냅니다. 근거: 오류가 타입 있는 입력 오류이고 문구가 원인을 말합니다. 업스트림은 이 경우를 따로 다루지 않습니다.
- `docs/MIMO.md:78`의 1 GB — 공식 최대 구성의 보장이 아니라 큰 사례의 합격 기준입니다. 근거: 같은 절 6의 측정 조건("큰 사례의 `mimo::compute` 전체"). 예약 누락 자체는 TD-24입니다.
## 관리자에게 묻는 것

없습니다. 처음 판의 질문 21개는 감사 이슈 #92에서 이렇게 정리했습니다.
- **유지보수자가 답한 것 둘**: 낡은 캐시의 mdat는 막고 [재보정]을 보입니다. 시작 지연과 스윕 길이의 정본은 측정 화면입니다.
- **나머지**:
  - 업스트림 동작으로 답했습니다: XO 추천 저장, 결과 값 가져오기, 단일 서브 BA 카드.
  - ADR과 저장소 문서로 답했습니다: 중첩 키 보존(ADR 0004), M3 1 GB의 범위, 1.0.0 조건(ARCHITECTURE 8절의 M6, `docs/THIRD_PARTY.md:25`).
  - 코드를 읽어 결함으로 확인한 것은 TD-90–TD-104가 되었습니다.
