# 편차 대장

SECS 2R이 업스트림 SECS 2와 일부러 다르게 동작하는 자리의 목록입니다(0004). 이식은 먼저 골든을 맞추고, 아래 항목은 그다음 PR에서 넣습니다. 출력을 바꾸는 항목은 골든 방 모두에서 어느 채널도 성적표 지표가 나빠지지 않아야 채택합니다.

번호의 D 숫자는 2026-10-12 엔진 조사의 번호를 그대로 씁니다. 엔진 밖의 항목은 영역 머리글자(F 형식, R REW, U 화면, S 보안)를 붙입니다. 출처의 파일 위치는 업스트림 Beta 1.2.0(60ac53a) 기준입니다.

상태: **계획**(아직 넣지 않음), **적용**(넣고 테스트가 지킴), **보류**(실험 뒤 결정), **닫힘**(업스트림이 고쳐 골든으로 돌아감).

## 엔진

| 번호 | 업스트림 동작 | 바꾸는 동작 | 출처 | 상태 |
|---|---|---|---|---|
| D1 | 메인·서브 버스 지연의 목적 함수가 코히런트 합과 상쇄를 똑같이 벌해 직교 정렬을 고릅니다. 이상적인 LR4 80 Hz에서 −2.75 ms를 골라 합에 −2.8 dB 딥이 생깁니다. | 코히런트 합을 보상하는 목적 함수. 2026-10-13 유지보수자 결정. | `correction_engine.py:1063-1095` | 계획 |
| D2 | 샘플레이트를 기준 위치에서만 확인하고 다른 위치는 48 kHz로 취급합니다. | 모든 IR을 확인하고 다른 레이트는 위치·역할·레이트와 함께 `EngineError::Input`으로 거절합니다. 준비와 긴 창 모두 `secs-engine::rejects_invalid_measurements_at_every_position_d2`가 확인합니다. | `correction_engine.py:494-497` | 적용 |
| D3 | `to_low`가 0.366 Hz 격자에서 0.417 Hz 격자로 선형 보간해 시간 가중과 앨리어싱을 만듭니다. T60 1 s 모드의 0.9 s 지점이 −3.9 dB, 1.15 s 지점이 −14 dB로 줄어 MIMO 목적과 가드가 늦은 에너지를 덜 봅니다. | 48 kHz irfft 뒤 안티앨리어싱 필터와 데시메이션. | `mimo.py:69-74` | 계획 |
| D4 | `realize`의 위상 적분이 0 Hz 항을 포함해 상수 위상 −2π·lift·df가 돕니다(저지연 −1.05°, 표준 −3.7°, 초고지연 200 ms −26.1°). | 0 Hz 항을 빼고 적분합니다. | `correction_engine.py:1230` | 계획 |
| D5 | 음수 시작 시각이 2.73 s 주기로 원형 이동하고, 다른 주기의 저속 투영에서는 직접음이 잘립니다. | 측정 모듈의 시간 축 규약 하나로 처리합니다. 골든으로 먼저 확인합니다. | `correction_engine.py:512, 560` | 보류 |
| D6 | `stage_responses`가 `self.H`를 잠깐 바꿔치기하는 동안 화면 스레드가 같은 배열을 읽습니다. | 순수 함수와 바뀌지 않는 결과 값. | `correction_engine.py:573-575` | 계획 |
| D7 | 진행률이 BA 뒤와 첫 채널에서 두 번 오르고 끝을 알리지 않습니다. | 단계별 진행과 완료 보고. | `correction_engine.py:1386, 1432` | 계획 |
| D8 | `compute()`를 두 번 부르면 묶음 PEQ가 두 번 걸립니다. | 결과 값을 새로 만들고 입력을 바꾸지 않습니다. | `correction_engine.py:945-946` | 계획 |
| D9 | 묶음 PEQ의 부스트가 6 dB로 고정되어 '부스트 상한' 설정을 무시합니다. | PEQ 맞춤의 이름 붙은 한도 묶음이 부스트 상한을 따릅니다. | `correction_engine.py:941` | 계획 |
| D10 | 서브 버스 PEQ의 밴드 중심이 2 kHz까지 미끄러질 수 있습니다(업스트림 주석이 문제를 적어 둠). | 밴드 중심을 버스 맞춤 범위 안에 묶습니다. | `correction_engine.py:252, 777-781` | 계획 |
| D11 | BA 차분 진화의 변수 7개 가운데 올패스 4개가 쓰이지 않아 평가를 2.3배 낭비합니다. 고치면 난수 흐름이 바뀝니다. | 쓰이지 않는 변수를 뺍니다. | `calibration.py:567-576`, `correction_engine.py:738-747` | 계획 |
| D12 | MIMO 위반 함수가 dB와 꼬리 에너지 비율을 섞어 경로 제거 순서를 정합니다. | 단위를 맞춘 위반 척도. 골든으로 영향을 먼저 잽니다. | `mimo_iir.py:413` | 보류 |
| D13 | FIR MIMO 서포트(167 ms)에 공통 지연이 없어 표준 모드에서 쓸 수 있는 구간이 약 50 ms, 초고지연에서는 거의 없습니다. | 실험으로 확인한 뒤 공통 지연을 더합니다. | `export_camilladsp.py:142, 295-299` | 보류 |
| D17 | 계산 취소가 결과만 버리고 계산 스레드는 계속 돕니다. 다시 누르면 계산이 겹치고, 다시 계산하면 MIMO 결과가 사라집니다. 프로세스 종료는 비공개 속성에 기댑니다. | 협조적 취소, 무리마다 잡 하나, 이전 결과 보존과 서명 기반 낡음 표시. `secs-service::{jobs_push_ordered_events_and_exclude_the_same_group,cancellation_finishes_within_one_second,cancelled_calibration_preserves_previous_results,room_dba_mimo_runs_and_becomes_stale_after_recalibration}`가 잡 경계, 취소, 이전 결과 보존, MIMO 낡음을 지킵니다. | `wizard/step_calibration.py:1836-1846`, `mimo_iir.py:180-195` | 적용 |
| D37 | 보정 결과의 낡음을 정하는 측정 서명이 위치, 역할, t0만 봅니다. 같은 위치를 다시 재어 t0가 같으면(자체·모의 측정, 같은 시간 원점의 mdat) 파형이 바뀌어도 결과가 최신으로 보입니다. | 서명에 측정마다의 UUID를 더하고, 자체·모의 측정에도 UUID를 매깁니다. `secs-service::measuring_another_position_marks_calibration_stale`가 같은 위치 재측정과 다른 위치 측정이 모두 결과를 낡게 하는지 확인합니다. | `calibration.py measurement_signature` | 적용 |
| D38 | REW 캘 파일의 `Sens Factor`가 숫자로 바뀌지 않으면 환경 수집 예외가 측정 전체를 실패시킵니다. | 측정은 남기고 그 측정의 환경만 생략합니다. | `rew_mdat/source_info.py from_rew_measurement`, `rewapi/rew.py _source_info` | 적용 |
| D22 | 엔진 값 15개 안팎을 환경 변수로만 바꿀 수 있고, 워커가 호출마다 다시 읽습니다. | 명시적 튜닝 값, 화면의 전문가 표시. | `correction_engine.py`, `mimo.py`, `mimo_iir.py` 머리 | 계획 |
| D24 | mdat 내보내기의 시간 원점이 `final` 응답과 시프트만큼 어긋납니다. | 신호 사슬 값 하나에서 시뮬레이터와 내보내기가 같은 원점을 씁니다. | `calibration.py:1101, 1321` | 계획 |
| D27 | 퇴화한 IR에서 `long_spectra`가 0으로 나눕니다. | 측정 모듈이 입력을 확인합니다. | `correction_engine.py:558-559` | 계획 |
| D36 | MIMO IIR의 직렬 경로(다중 스레드 BLAS)와 병렬 경로(BLAS 1스레드)가 같은 비트를 보장하지 않습니다. | 결정적 병렬(0007). | `mimo_iir.py:370-391` | 계획 |

## GPU 계산

| 번호 | 업스트림 동작 | 바꾸는 동작 | 출처 | 상태 |
|---|---|---|---|---|
| G1 | MIMO의 정칙화 선형계를 CPU로 풉니다. | FIR 모드의 큰 선형계에 CUDA f64 spectral 풀이와 한 번의 보정을 허용합니다. CPU 기준·λ 선택·음향 가드는 유지하며, GPU 결과는 비트 동일성 대신 수치 허용오차로 확인하고 실행 이력을 기록합니다. IIR은 목표 설계부터 CPU를 유지합니다. 장치 정책·전체 CPU 재시작·취소 테스트와 [실제 GPU 음향 비교](perf/gpu.md)가 범위를 검증합니다. | `mimo.py`, [ADR 0008](adr/0008-native-gpu-acceleration.md) | 적용 |

## 형식과 저장

| 번호 | 업스트림 동작 | 바꾸는 동작 | 출처 | 상태 |
|---|---|---|---|---|
| F1 | `.secs` 저장이 원자적이지 않고 측정 스레드와 화면 스레드가 잠금 없이 함께 씁니다. | 임시 파일과 이름 바꾸기, 쓰는 쪽 하나. `secs-project::interrupted_save_leaves_old_secs_intact_f1`이 지킵니다. | `model.py:926-931`, `rewapi/session.py:332` | 적용 |
| F2 | 위치 측정 중 오류가 나면 그 위치에서 이미 잰 IR까지 버립니다. | 채널마다 바로 기록하고, 다시 재면 그 위치를 바꿉니다. `secs-project::test_cancel_then_resume_skips_done_positions_and_partial_position_is_not_done_f2`, `secs-project::test_remeasure_replaces_entry`가 지킵니다. | `rewapi/session.py:418-427` | 적용 |
| F3 | mdat 파서가 깊이 200 근처에서 RecursionError를 냅니다. | 명시적 작업 스택과 깊이 상한(상위 클래스 사슬도 깊이를 씁니다). `secs-jos::class_hierarchy_obeys_depth_limit`, 변형 입력 3000개 시험. | `rew_mdat/serialization/parser.py:46-55` | 적용 |
| F4 | mdat 빌더의 UUID가 무작위이고 원 측정 시각이 없거나 계산값이면 현재 시각을 읽어 같은 입력에서 다른 바이트가 나옵니다. | UUID 출처와 한 파일의 시각을 주입합니다(`secs_mdat::build`의 `uuid`, `now_ms`). 앱은 무작위 UUID와 실제 시각, 테스트는 고정값. `secs-mdat::builds_match_bytes_except_nine_one_ulp_caches`. | `rew_mdat/builder.py` | 적용 |
| F5 | `.secs`의 IR 경로를 절대 경로와 `..`까지 그대로 따릅니다. | 프로젝트의 `.irs` 안으로 제한하고, 벗어나면 이주 경고를 띄웁니다. 절대 경로·`..`·심볼릭 링크 탈출과 정상 경로를 `secs-project::f5_absolute_parent_and_symlink_escapes_are_rejected_but_in_tree_reads`가 지킵니다. | `rewapi/session.py:361-381` | 적용 |
| F6 | CamillaDSP 내보내기가 WAV를 쓴 뒤에야 ASCII 검사에서 실패하고, NaN을 그대로 YAML에 씁니다. | 모든 검사를 첫 파일을 쓰기 전에 합니다. | `export_camilladsp.py:364-365, 402` | 계획 |
| F7 | mdat 파서의 상한이 배열·문자열 하나씩만 셉니다. 같은 배열·서명을 참조로 여러 번 가리키거나 상위 클래스가 순환하면 메모리를 계속 씁니다. | 해석한 메모리 전체를 `max_file_bytes` 예산으로 세고, 상위 클래스 순환을 거부합니다. 정상 파일과 오라클의 오류 사례는 그대로입니다. `secs-jos::shared_signature_copies_obey_aggregate_allocation_budget`, `secs-jos::declared_array_sizes_do_not_allocate_before_truncation_checks`. | `rew_mdat/serialization/parser.py`, `rew_mdat/limits.py` | 적용 |
| F8 | 파이썬 문자열은 짝 없는 UTF-16 서러게이트를 담습니다. | 러스트 문자열에는 담을 수 없어 글자 API는 U+FFFD로 보여 주고, 해석한 스트림은 원래 바이트를 지녀 고치지 않은 글자는 그대로 왕복합니다. UTF-16 단위 API로 잃지 않고 읽고 쓸 수 있습니다. `secs-jos::utf8_and_lone_surrogates`. | `rew_mdat/serialization/modified_utf8.py` | 적용 |
| F9 | `.secs`의 `\uD800` 같은 짝 없는 서러게이트 이스케이프를 읽어 파이썬 문자열에 그대로 담습니다(저장할 때 UTF-8 인코딩에서 실패합니다). | 러스트 문자열에는 담을 수 없어 읽을 때 `ProjectError`로 거부합니다. SECS 2 자신이 이런 파일을 저장할 수 없으므로 손으로 고친 파일에서만 생깁니다. `secs-project::pyjson_regressions`. | `model.py:934-937` | 적용 |
| F10 | 손으로 고친 `.src.json`의 부분 변환 오류가 `apply`에서 내보내기 전체를 실패시키거나 Java `Integer` 범위를 넘는 값과 유한하지 않은 날짜를 뒤 단계까지 넘깁니다. | 숫자 문자열·불을 포함한 float32 배열 등 파이썬이 변환하는 값은 그대로 변환하고, 실제로 잘못된 부분만 생략해 내보내기를 계속합니다. i32 범위 밖 `Integer`와 NaN·무한대 날짜도 생략합니다. `secs-project::measurement_source_tolerates_partial_values_and_rejects_unsafe_narrowing`, `secs-project::measurement_source_builds_upstream_mdat_bytes_and_skips_upstream_exceptions_f10`. | `rew_mdat/source_info.py apply` | 적용 |

## REW

| 번호 | 업스트림 동작 | 바꾸는 동작 | 출처 | 상태 |
|---|---|---|---|---|
| R1 | `/measure` 4xx 본문에 'pro'가 들어가면(process, problem) Pro 없음으로 판정하고, 401·402·403은 어느 경로든 Pro 없음입니다. | 401·402·403은 업스트림처럼 경로와 무관하게 Pro 필요로 판정하고, 본문 문구로는 판정하지 않습니다. 0.19.0까지는 `/measure/command`의 상태만 보았는데, REW 5.40은 스윕 설정(`/measure/sweep/configuration`)에서 401을 주어 Pro 안내 대신 일반 REW 오류가 났습니다(2026-10-18 Windows 실기 검증). `secs-rew::test_r1_pro_required_uses_only_status`. | `rewapi/rew.py:286-292` | 적용 |
| R2 | 상태 확인의 차단기가 측정 스레드의 요청까지 6초 막습니다. | 상태·장치 조회에만 차단기를 적용하고, 측정 세션과 취소 요청에는 적용하지 않습니다. `secs-rew::test_r2_probe_circuit_breaker_does_not_block_measurement_requests`, `test_r2_full_position_ignores_offline_hold_including_device_lookups`. | `rewapi/rew.py:273-284, 317-329` | 적용 |
| R3 | 콜백 서버의 경로를 예측할 수 있고, 4 MB를 넘는 본문을 일부만 읽습니다. | 서버별 무작위 경로 토큰, 미등록 경로 404, 4 MB 초과 본문을 끝까지 버린 뒤 413. `secs-rew::test_r3_random_token_wrong_path_404_and_oversized_body_drained`는 같은 연결의 다음 요청까지 확인합니다. | `rewapi/callback.py:18, 47-59` | 적용 |
| R4 | FakeREW가 상위 폴더의 `tests/synth.py`를 불러옵니다. | `IrSource`로 IR·t0·SNR을 주입하며 제품에는 테스트 코드·샘플 데이터를 넣지 않습니다. `secs-rew::test_r4_fake_two_position_plan_uses_only_supplied_ir_source`. | `rewapi/fake.py:54-81` | 적용 |
| R5 | 합성 IR의 역할별 시드가 프로세스마다 달라지는 Python `hash(role)`에 기대어 실행 사이 바이트가 달라집니다. | UTF-8 역할 이름의 FNV-1a 64비트 값을 100000으로 나눈 나머지와 기준 시드 20260916을 작은 PCG 생성기에 넣어 어느 프로세스와 스레드 수에서도 같은 합성 IR을 만듭니다. `secs-service::sample_load_synthetic_structure_and_determinism`이 지킵니다. | `rewapi/fake.py:127-174` | 적용 |
| R6 | REW API에 라이선스를 알려 주는 곳이 없어 Pro를 모를 때도 Pro로 보고 '확인됨 · 자동 측정 가능'으로 표시합니다. | 연결 직후에는 '아직 모름'으로 두고, REW가 Pro 필요로 거절하면 'Pro 아님', 자동 스윕이 성공하면 '확인됨'으로 바꿉니다. 거절한 뒤에는 REW 방식을 고를 수 없고, REW가 꺼지거나 '다시 확인'을 누르면 '아직 모름'으로 돌아갑니다. `secs-service::rew_license_observation_tracks_rejection_success_and_launch_reset`. | `rewapi/rew.py pro_state·is_pro`, `wizard/step1_devices.py` Pro 배지 | 적용 |
| R7 | REW 방식에서도 입력·출력 장치와 입력 채널을 고르게 하지만 REW에 보내지 않습니다. REW는 자기 환경설정의 장치로 재고, 원 측정 환경에는 고른 이름이 들어갑니다. | REW 방식에서는 REW가 쓰는 입력 장치·입력 채널·출력 장치를 읽어 보여 주고, 바꾸는 곳이 REW 환경설정이라고 적습니다. REW API 도움말이 장치 지정 본문을 밝히지 않아 지정은 하지 않습니다. 원 측정 환경에는 REW가 쓴 장치 이름을 넣고, 읽지 못하면 REW 장치 목록에 있는 저장 이름만 씁니다. 저장된 값은 옛 프로젝트와 SECS 2 호환을 위해 그대로 둡니다. `secs-rew::{test_current_audio_reads_java_string_and_object_values,test_current_audio_reads_asio_device_once_for_both_directions,test_current_audio_tolerates_errors_empty_and_unknown_values}`, `secs-service::rew_source_devices_fall_back_only_to_names_in_rew_lists`. | `wizard/step1_devices.py` 장치 칸, `rewapi/rew.py` | 적용 |

## 화면

| 번호 | 업스트림 동작 | 바꾸는 동작 | 출처 | 상태 |
|---|---|---|---|---|
| U1 | 레벨 체크의 출력 슬라이더가 측정 스윕 레벨과 이어져 있지 않고, 레벨 결과는 튜토리얼 밖에서 쓰이지 않습니다. 건너뛰면 그럴듯한 가짜 값을 씁니다. | 서비스의 `level_check` 권고를 `level_apply`로 스윕 설정에 저장합니다. `secs-service::mock_flow_level_apply_and_calibration`이 실제 모의 레벨, 권고 적용·반복 적용, 측정과 보정을 확인합니다. 화면 연결은 M5입니다. | `wizard/step3_level.py`, `rewapi/rew.py configure` | 적용 |
| U2 | DSP 프로필 칸 여섯 개를 엔진이 읽지 않습니다(MIMO FIR 예산 문구만 씀). | '내보낼 DSP'로 바꾸고 예산 경고에만 쓴다고 화면에 적습니다. | `wizard/step2_config.py:1090-1125` | 계획 |
| U3 | 단계별 REW mdat 내보내기가 숨겨져 닿을 수 없습니다. | 내보내기의 고급에 되살립니다. | `wizard/step_calibration.py` 내보내기 카드 | 계획 |
| U4 | 마지막 단계의 '완료'가 아무 일도 하지 않습니다. | 없앱니다. | `wizard/window.py advance` | 계획 |
| U5 | 측정을 지우는 동작(프리셋·서브 수·배치 변경, 마지막 위치 다시)이 확인 없이 측정을 지웁니다. | 프리셋·서브 수·배치 변경은 측정이 있으면 먼저 `confirm_required`를 돌려주고, 확인한 변경만 측정을 지웁니다. 이 변경과 `position_clear`는 프로젝트별 최근 16개 단일 사용 토큰으로 기록을 복원하며, `position_clear`의 즉시 저장·깨끗한 상태는 그대로 두고 구성·배치 복원은 저장 전 더티 상태로 둡니다. `secs-service::{destructive_config_and_layout_edits_require_confirmation_and_undo_once,position_clear_undo_is_byte_exact_and_single_use}`가 확인, 복원, 토큰 재사용 거절과 기존 `.secs` 바이트 복원을 지킵니다. | `wizard/step2_config.py`, `step4_layout.py`, `step5_measure.py` | 적용 |
| U6 | SNR 경고 문턱이 코드는 20 dB, 도움말은 40 dB입니다. | 업스트림 의도를 확인한 뒤 하나로 맞춥니다. | `wizard/step5_measure.py`, `tutorial_text.py` | 보류 |
| U7 | 영어 화면이 꺼져 있습니다(`ENGLISH_UI=False`). | 한국어와 영어 카탈로그를 처음부터 함께 냅니다. | `i18n.py:25`, `window.py:564-570` | 계획(M5) |
| U8 | 기다림 표시가 작업 패널, 로딩 카드, 물방울 셋이고, 로딩 카드의 [취소]는 이어져 있지 않습니다. | 작업 트레이 하나에 잡마다 진행과 실제로 멈추는 취소를 둡니다. | `busy.py`, `loading.py:218-338`, `window.py:181-252` | 계획(M5) |
| U9 | 초고지연 모드가 설명 줄을 3초 안에 다섯 번 눌러야 열립니다. | '전문가 표시'에서 고릅니다. | `step_calibration.py:1111-1145` | 계획(M5) |
| U10 | 새 프로젝트가 측정 방법을 저장하지 않아 서비스의 옛 프로젝트 호환 기본값(`native`)으로 즉시 측정할 수 있습니다. | 새 프로젝트만 `devices.method = "none"`을 저장해 장치를 고르기 전 측정을 막습니다. 방법이 없는 기존 프로젝트는 계속 `native`로 읽습니다. `secs-service::new_projects_store_none_measurement_method`가 새 프로젝트의 명시값을 지킵니다. | `wizard/step1_devices.py`, `wizard/model.py` | 적용 |
| U11 | 업스트림은 화면 테마 하나만 제공합니다. | 시스템 설정, 밝게, 어둡게 가운데 고른 테마를 앱 설정에 저장합니다. `secs-service::app_settings_are_versioned_bounded_and_capped`가 왕복과 이전 설정 파일의 시스템 기본값을 확인합니다. | `wizard/` 화면 전체 | 적용 |
| U12 | 로고는 확정안(`logo_b8.svg`) 하나이고 사이드바 마크, 창·작업 표시줄 아이콘, exe 아이콘에 씁니다. | 실행 파일·설치본 아이콘과 기본 로고는 같은 확정안('전문가 모드')입니다. 앱 설정에서 SECS 2R이 따로 그린 '킹반인 모드' 로고(`apps/secs2r/icons/logo/civilian.svg`: 측정 응답과 목표선 위의 보정 응답, PEQ 손잡이)를 고르면 레일 로고와 창 아이콘만 바뀝니다. macOS의 Dock 아이콘과 설치한 파일·바로 가기의 아이콘은 그대로입니다. `secs-service::app_settings_are_versioned_bounded_and_capped`가 저장과 이전 설정 파일의 기본값을, `secs2r::{windows_icon_size_follows_the_display_scale,civilian_pictures_are_square_and_match_their_names}`가 창 아이콘 그림을, 화면의 `src/state/flow.test.tsx`가 고른 로고가 다시 켠 뒤에도 남는지 확인합니다. | `wizard/branding.py`, `window.py _sidebar`, `SECS.spec icon=` | 적용 |
| U13 | 스피커 구성 프리셋은 2.0, 2.1, 2.2, 5.1, 7.1.4, 9.1.6입니다. 센터 없는 4채널(쿼드러포닉)과 높이 채널 없는 7.1은 빈 구성에서 손으로 만들어야 합니다. | 4.0, 4.1, 4.2 프리셋을 2.2와 5.1 사이에, 7.1 프리셋을 5.1과 7.1.4 사이에 더합니다. 4.x는 5.1에서 센터를 뺀 L, R, Ls, Rs에 서브우퍼를 0~2개 두고, 7.1은 7.1.4에서 탑 넷을 뺀 L, R, C, SW1, Ls, Rs, Lb, Rb입니다. 순서와 출력 번호는 업스트림 프리셋과 같은 규칙(앞 채널 다음 서브, 그 뒤 서라운드와 백)을 따릅니다. `.secs`에는 프리셋 이름이 문자열로만 남으므로 SECS 2도 이 파일을 열고 같은 바이트로 다시 저장합니다. 그쪽 2단계 화면에서는 켜진 프리셋 버튼이 없을 뿐입니다. `secs-project::{quad_presets_are_5_1_without_the_centre,seven_one_is_7_1_4_without_the_heights,added_presets_survive_save_and_reopen}`이 역할, 출력 번호, 바이앰프 대역, 저장 왕복을, 화면의 `SystemScreen.test.tsx`가 버튼 순서를 확인합니다. | `model.py PRESETS`, `wizard/step2_config.py` 프리셋 줄 | 적용 |

## 보안

| 번호 | 업스트림 동작 | 바꾸는 동작 | 출처 | 상태 |
|---|---|---|---|---|
| S1 | 프로젝트를 열면 `.irs/calibration_cache.pkl`을 제한 없이 역직렬화합니다. 남이 보낸 프로젝트 폴더로 코드를 실행할 수 있습니다. | 8 MiB로 제한한 버전 2 JSON 데이터 캐시(`secs_run_cache.json`)를 원자 교체합니다. 옛 피클은 열지 않습니다. 캐시는 CamillaDSP에 필요한 사슬·실행 라벨·MIMO 경로를 보관하고, 스펙트럼이 필요한 mdat는 다시 연 프로젝트에서 취소 가능한 `design` 단계를 먼저 계산합니다. `secs-service::{round_trip_and_ignore_pickle,corrupt_and_oversized_files_are_ignored,unknown_cache_version_is_ignored,exports_after_reopening_match_pre_close_bytes}`가 지킵니다. | `calibration.py:1144-1187, 1226-1236` | 적용 |
