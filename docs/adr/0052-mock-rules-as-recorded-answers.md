# 0052. 모의 서비스의 규칙 사본을 기록 응답과 생성 표로 바꾼다

상태: 제안, 2026-11-06

근거: 감사 이슈 #186, 아키텍처 리뷰 A11. 해결하는 기술 부채 발견은 TD-173(모의 서비스의 규칙 사본), TD-174(손으로 옮긴 화면 상수), TD-235(모의 서비스가 모든 보정에 붙이는 메모와 견본의 빈 상태)입니다.

## 배경

ADR 0014는 모의 서비스(`apps/secs2r/ui/src/ipc/mock.ts`)가 봉투, 잡 스케줄러와 진행 이벤트, 실패 주입, 되돌리기, 열린 프로젝트의 상태 보관만 맡고, 서비스 규칙을 다시 구현한 함수는 생성 상수나 기록 응답으로 바꾼다고 정했습니다. 한도 상수는 `secs-types`의 `rules` module에서 `bindings/rules.ts`로 생성되어 옮겨졌지만, 규칙 자체는 그대로 남았습니다. 2026-11-06의 코드에서 다시 센 결과입니다.

- 계약에 속하는 부분: 봉투(`mock.ts:1390-1398`), 잡 스케줄러와 진행(`530-640`), 배타 묶음(`107-113`), 실패 주입(`370-372`, `1401-1404`), 되돌리기(`469-479`, `967-972`), 프로젝트 상태(`375-396`, `513-523`), 기록 응답 조회(`1241-1250`).
- 서비스·엔진·앱 규칙의 사본(약 600줄): `rcEditRefusal`(`213-243`)과 `editSettings`(`642-745`, 37개 arm)는 `crates/secs-service/src/settings_edit.rs:18-139, 391-552`의 복사이고, `rcLimitNote`(`153-169`)는 ADR 0021의 Schroeder 산술, `mockRcBandsNotes`(`191-202`)는 엔진의 구간 병합 규칙, `mockLowProtectNotes`(`313-326`)와 `mockInputHpNotes`(`328-331`)는 보정 메모, `dirty`(`450-460`)·`MIMO_ONLY_EDITS`(`333-334`)·`correctionSettings`(`120-138`)·`mimoExport`(`417-423`)는 낡음 판정, `1022-1031`은 XO 추천, `EXPORT_FOLDER_TAG`(`143-145`)와 `1102-1134`는 내보내기 폴더 이름과 파일 목록, `1071-1077`은 출력 레이트 해석, `771-777`은 MIMO 선행 조건, `1202-1222`는 DSP 규칙, `1263-1293`은 업데이터 전환(`apps/secs2r/src/update.rs:490-517`의 사본)입니다.
- 사본이 interface가 된 자리: `screens/calibrate/fixtures.ts:20`이 `mockRcLines`·`mockLowProtectNotes`·`mockInputHpNotes`를 가져와 견본을 만들고, `CalibrateScreen.test.tsx:10, 1573`이 `rcEditRefusal`을 정답으로 쓰며, `:1201-1210`의 154, 308, 616, 287 Hz는 `rcLimitNote`가 만든 합성 수치입니다. 사본을 고정하는 시험은 `mock.test.ts:687, 976, 1048, 1129`와 `mockWiring.test.ts`(18개 가운데 상당수)입니다.
- 기록기 `crates/secs-service/tests/recorded.rs:145-205`는 `mode_spec`, `config_apply_preset`, `target_apply_preset`, `calibration_settings_get` 넷만 기록합니다.

지난 감사(#155)의 TD-123과 TD-125는 이 사본이 서비스와 어긋난 사례였고, 수정은 사본을 다시 맞추는 일이었습니다. deletion test로 보면 `rcEditRefusal`과 `editSettings`를 지웠을 때 화면 시험 넷이 같은 규칙을 다시 요구하므로, 사본은 서비스 규칙의 두 번째 adapter입니다. ADR 0014의 '결과' 절은 기록을 입력이 없는 응답에만 쓴다고 했는데, 그 제한이 사본이 남은 직접적인 이유입니다.

## 결정

### 기록 범위를 '입력 하나짜리' 응답까지 넓힌다

ADR 0014의 결과 절을 이렇게 고칩니다. 기록은 입력이 없는 응답과, 매개변수 하나(또는 한 묶음)를 키로 하는 응답에 씁니다. 기록 파일은 지금처럼 `{ params, data }`의 목록이고, 모의 서비스는 `params`가 같은 항목의 `data`를 돌려줍니다. 기록은 `recorded.rs`가 결정적 `Deps`(고정 시계, 순차 UUID)로 서비스를 띄워 만듭니다.

새로 기록하는 응답은 다음입니다.

| 기록 파일 | 요청 | 키 | 기록하는 상황 |
|---|---|---|---|
| `calibration_settings_edit.json` | `calibration_settings_edit` | `edit` | 새 프로젝트에서 `SettingEdit`의 종류마다 대표 편집 하나와 한도 밖 편집 하나. 받아들인 설정 전체 또는 거절 봉투(`field`, `reason`, `min`, `max`, `index` 등)를 기록합니다. |
| `results.json`, `result_lines.json` | `results`, `result_lines` | 보정에 쓴 설정의 `engine_key`와 `tab`·`channel` | 견본 방(`project_new sample` + 모의 측정 9위치)에서 기본 설정, `rc_hi_basis` 세 가지와 `rc_hi_transition` 1·2·4, 수동 상한, 부피 없음, 극저역 보호, 입력 저역 컷, 추가 구간 둘을 켠 보정의 요약과 메모 줄. |
| `export.json` | `export` | `kind`, `design_name`, `extra_rates` | 같은 견본 방에서 종류마다 한 번. 폴더 이름과 파일 목록, 출력 레이트와 메모. |
| `update_transitions.json` | 앱의 `app_settings_set`·`update_*` | `policy`, `phase.kind` | `apps/secs2r`의 업데이터 시험이 `update.rs`의 전환 표(정책 × 단계 → 다음 단계)를 기록합니다. 서비스가 아니라 앱의 규칙이므로 기록 주체는 앱 크레이트의 시험입니다. |

### 생성 표를 `rules.rs`에 추가한다

`secs-types/src/rules.rs`에 다음을 추가하고, `cargo test -p secs-types`를 실행하면 `bindings/rules.ts`에 저장됩니다. 모의 서비스와 화면은 이 값을 가져다 쓰고 같은 값을 따로 두지 않습니다.

- `ENGINE_KEY_FIELDS`, `MANUAL_KEY_FIELDS`, `MIMO_KEY_FIELDS`: `CalibrationSettings::engine_key`·`manual_key`·`mimo_key`가 보는 필드 이름 목록. 모의 서비스의 `correctionSettings`와 `MIMO_ONLY_EDITS`가 이 목록으로 낡음을 판정합니다.
- `JOB_GROUPS`: `jobs.rs`의 배타 묶음(`device`, `compute`, `export`, `status_watch`)과 잡 종류의 대응.
- `SCALAR_EDITS`: `(kind, field, limit key, 정책)`의 표. 정책은 `clamp`(한도로 자름)나 `refuse`(거절)입니다. 이 표는 ADR 0055가 정하는 설정 선언에서 나옵니다.
- `EXPORT_FOLDER_TAGS`, `MIMO_MIN_POSITIONS`, `DESIGN_RATE`, `OUTPUT_RATES`, `SUB_LIMITS`, `INFRA_DEFAULT_HZ`, `ALLPASS_Q_DEFAULT`, `TRANSITION_FACTORS`, `AGREEMENT_MIN_POSITIONS`, `REPETITIONS`, `NEW_PROJECT_PRESETS`, `INPUT_HP_START`: TD-174가 짚은 손 사본 열 개의 정본.

### 모의 서비스에 남는 것과 사라지는 것

남는 것은 ADR 0014가 정한 계약입니다. 봉투, 잡 스케줄러와 진행 이벤트, `JOB_GROUPS`로 판정하는 배타, 실패 주입, 되돌리기, 열린 프로젝트의 상태, 대화상자 큐, 그리고 기록을 찾아 돌려주는 조회입니다. 스칼라 설정 편집은 `SCALAR_EDITS`를 조회해 적용하고, 거절은 기록된 거절 봉투를 돌려줍니다. 서브 항목, 묶음 배치, 인프라, XO 맵처럼 구조를 바꾸는 편집은 손 코드로 남깁니다. 값을 만들어 내는 규칙이 아니라 상태를 옮기는 일이기 때문입니다.

사라지는 것은 `rcLimitNote`, `mockRcBandsNotes`, `mockLowProtectNotes`, `mockInputHpNotes`, `rcEditRefusal`, `correctionSettings`, `mimoExport`, 내보내기 폴더 이름과 파일 목록 생성, 출력 레이트 해석, MIMO 선행 조건 판정, DSP 편집 규칙, 업데이터 전환입니다. 각각 기록 조회나 생성 표로 바뀝니다. 모의 서비스의 보정은 기록된 `results`·`result_lines` 가운데 지금 설정의 `engine_key`와 같은 항목을 돌려주고, 기록에 없는 설정 조합은 기본 설정의 기록에 `stale` 표시만 바꿔 돌려줍니다. 그래서 모의 서비스의 메모는 서비스가 실제로 낸 값이 되고, TD-235가 짚은 '모든 보정에 극성 경고'도 기록된 상황에서만 나타납니다.

화면 견본(`screens/calibrate/fixtures.ts`)은 기록된 `results`와 `result_lines`에서 만들고, 합성 곡선만 지금처럼 만듭니다. `CalibrateScreen.test.tsx:1201-1210`의 수치는 기록된 메모의 값으로 바뀝니다.

### ADR 0014의 수정

ADR 0014의 결과 절 넷째 항목을 '기록은 입력이 없는 응답과 매개변수를 키로 하는 응답에 씁니다. 측정 파형과 곡선처럼 프로젝트의 측정에 기대는 응답은 모의 서비스의 픽스처가 만들고, 그 값은 화면을 위한 견본일 뿐 서비스 규칙의 사본이 아닙니다'로 고칩니다.

## 시험

- `recorded.rs`의 `no_input_responses_match_the_recorded_copies`를 `responses_match_the_recorded_copies`로 넓혀 새 기록 넷을 같은 방식(저장소 사본과 비교, `SECS_RECORD=1`로 다시 기록)으로 지킵니다. 견본 방 보정은 서비스 시험이 이미 돌리는 경로라 시간이 크게 늘지 않습니다.
- `secs-types`의 rules 시험이 `ENGINE_KEY_FIELDS`가 실제 `engine_key` 구현이 읽는 필드와 같은지, `JOB_GROUPS`가 `jobs.rs`의 표와 같은지 확인합니다. 두 표를 서비스 코드가 직접 쓰게 하여 사본이 생기지 않게 합니다.
- 화면 시험은 모의 서비스가 기록과 같은 값을 돌려주는지만 확인합니다. `mock.test.ts:687, 976, 1048, 1129`처럼 사본을 정답으로 고정하던 시험과 `mockWiring.test.ts`에서 모의 서비스를 자기 자신과 비교하던 시험은 지웁니다.
- 갤러리 견본이 기록된 메모로 표시되는지는 `ui-gallery.yml`의 리뷰 이슈로 확인합니다.

## 이행

커밋 크기의 단계입니다. 순서대로 하되 2와 3은 나란히 할 수 있습니다.

1. `rules.rs`에 표와 상수를 추가하고 `bindings/rules.ts`를 다시 생성합니다(TD-174).
2. `recorded.rs`에 네 기록을 추가하고 JSON을 커밋합니다. 앱 크레이트의 업데이터 시험이 전환 표를 기록합니다.
3. `mock.ts`를 조회로 바꾸고 사본 함수를 지웁니다. `fixtures.ts`와 `CalibrateScreen.test.tsx`가 기록을 읽습니다.
4. 사본을 고정하던 시험을 지우고 ADR 0014의 결과 절을 고칩니다.

## 결과

- 서비스 규칙이 바뀌면 `recorded.rs`나 `cargo test -p secs-types`가 먼저 실패하므로 모의 서비스가 낡은 규칙을 쥔 채 지나가지 않습니다. ADR 0014의 약속이 실제로 성립합니다.
- `mock.ts`는 약 600줄 줄고, 사본을 고정하던 시험이 사라집니다.
- 기록 파일이 넷 늘고 크기가 커집니다. 특히 `results.json`은 설정 조합마다 요약 하나를 담으므로 조합을 위의 목록으로 한정합니다. 기록에 없는 조합은 모의 서비스가 기본 기록을 돌려주므로, 화면 시험이 그 조합의 메모 값을 단언하려면 먼저 기록에 추가해야 합니다.
- 견본의 수치가 합성값에서 서비스의 값으로 바뀌므로 갤러리 화면이 달라집니다. 계산과 내보낸 파일은 바뀌지 않으므로 편차 항목은 없습니다.
- 다음 감사는 모의 서비스의 구조 편집(서브, 묶음, 인프라, XO 맵)이 손 코드인 것을 사본으로 다시 제안하지 않습니다. 그것은 규칙이 아니라 상태 이동입니다.
