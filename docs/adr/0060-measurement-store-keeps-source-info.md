# 0060. 측정 저장소가 출처 정보를 보관합니다

상태: 제안, 2026-11-06

감사 이슈 #186, 아키텍처 리뷰 A19. 기술 부채 감사의 TD-184(사이드카를 흐름마다 세 번 넘게 파싱함)와 TD-183의 일부(건너뛴 사이드카를 사용자에게 알리지 않음)를 해결합니다.

## 배경

ADR 0015는 측정의 마이크 보정 곡선을 IR 옆의 원 측정 환경 파일(`*.src.json`, mdat의 `SourceInfo`)에 두고 계산할 때 뺀다고 정했습니다. 그 파일을 읽는 자리가 저장소 한 곳이 아니라 흐름마다 있습니다. 아래 줄 번호는 main `885f0535` 기준입니다.

- `MeasurementStore::open`(`crates/secs-project/src/record.rs:213-232`)은 프로젝트를 읽고 이주하고 `load_project_sources`로 모든 사이드카를 파싱한 뒤, 보정 지문(`calibration_fingerprint`)만 `project.measurement_calibration`에 남기고 `SourceInfo`는 버립니다.
- `source_of(&mut self, role, pos)`(`:530-548`)는 호출마다 사이드카 하나를 디스크에서 다시 읽고 파싱합니다(`load_source`, `:665-686`). 마이크 보정의 `status`(`crates/secs-service/src/mic_calibration.rs:114-141`)와 `attach`(`:368-398`)가 (위치, 역할)마다 이것을 세션 잠금 안에서 부르고, `attach`는 잠금 안에서 사이드카를 다시 저장합니다. `calibration.rs:554`의 `measured_ir`도 IR을 읽을 때마다 부릅니다.
- `load_project_sources`(`:732-816`)는 모든 사이드카를 64 MiB 예산(`MAX_PROJECT_SOURCE_BYTES`, 감사 #155 TD-112) 안에서 파싱합니다. `open` 말고도 `session.rs:655`(`OriginalSnapshot::load`, 곁 mdat 잡), `:733`(`original_measurements`, 저장의 곁 mdat), `export.rs:336`(mdat 내보내기)이 다시 부릅니다. 흐름 하나가 같은 파일을 세 번 넘게 파싱하고 예산도 세 번 씁니다.
- `load_source`는 모든 오류를 `None`으로 펴고, `load_project_sources_within`의 `exhausted` 깃발은 함수 밖으로 나가지 않습니다(`:739, 770, 785`). 너무 크거나 깨졌거나 예산이 바닥난 사이드카는 조용히 빠지고, `open`은 그 측정의 보정 지문을 잃어 엔진이 보정 없는 IR로 다룹니다. 편차 F23은 '없음'을 허용하지만 사용자에게 진단을 주지 않습니다(TD-183).
- `load_project_irs`와 `LoadedIr`(`:818-855`, `:641-646`)은 시험 밖 호출자가 없습니다(TD-219).

deletion test로 보면 세 호출 자리를 지웠을 때 지금은 호출자마다 다시 나타나고, 저장소가 맵을 가지면 어디에도 나타나지 않습니다. 열기는 깊은 interface인데 열 때 읽은 것을 버려서 얕아졌습니다.

## 결정

### `MeasurementStore`가 `SourceInfo`를 가집니다

```rust
pub struct MeasurementStore {
    // 기존 필드 그대로
    sources: BTreeMap<(i64, String), Arc<SourceInfo>>,   // (위치, 역할) → 사이드카
    source_budget: SourceBudget,                          // 남은 바이트와 고갈 여부
}

impl MeasurementStore {
    pub fn source_of(&self, role: &str, pos: i64) -> Option<Arc<SourceInfo>>;        // 디스크를 읽지 않음
    pub fn sources_by_position(&self) -> BTreeMap<u32, BTreeMap<String, Arc<SourceInfo>>>;
}
```

- `open`이 지금의 `load_project_sources_within`으로 맵을 채웁니다. 그 함수는 `pub(crate)`가 되고, 결과로 `LoadedSources { by_entry, exhausted, skipped: Vec<SourceSkip> }`를 돌려줍니다. `SourceSkip { pos, role, reason: TooLarge | Unreadable | Invalid | Budget }`는 `open`이 `MigrationWarning { code: "measurement.source.skipped" | "measurement.source.budget", entry: "position N, role R" }`로 바꿔 `warnings()`에 넣습니다. `session.rs:655, 733`이 열 때의 경고를 지금처럼 `ProjectInfo`의 알림으로 전달하므로 사용자가 봅니다(TD-183). `load_source_checked`의 `Result`가 그 재료이고, `load_source`는 지웁니다.
- 맵을 유지하는 곳은 저장소의 쓰기 세 군데뿐입니다. `record`는 저장한 `source_info`를 넣거나 지우고, `replace_source`는 바꾸고, `remove_position`은 그 위치의 항목을 `RemovedPosition`에 넣어 두었다가 `restore_position`이 되돌립니다. 지문(`measurement_calibration`)을 갱신하는 자리와 같은 자리라 둘이 어긋날 수 없습니다. 디스크의 사이드카를 바깥 프로그램이 바꾸는 경우는 지금도 다음 열기까지 모르는 것과 같습니다.
- `source_of`는 `&self`가 되고 `Option<Arc<SourceInfo>>`를 돌려줍니다. `mic_calibration.rs`의 두 자리와 `calibration.rs:554`는 복제 없이 참조 수만 올립니다. `attached_calibration(&info)`의 서명은 그대로입니다.
- `sources_by_position`이 지금 `load_project_sources`가 돌려주던 모양(`BTreeMap<u32, BTreeMap<String, _>>`)을 `Arc`로 돌려주므로 `original_measurements`(`session.rs:726-769`)와 `export.rs:336`은 호출만 바꿉니다. `OriginalSnapshot`은 잠금 아래에서 `sources_by_position()`을 함께 잡고 `load`에서 다시 읽지 않습니다(TD-149가 곁 mdat을 잡으로 옮길 때 이 스냅샷을 씁니다).
- `load_project_sources`(pub)와 `load_project_irs`, `LoadedIr`은 지웁니다. `secs-project`는 게시 크레이트이지만 0.x의 minor 판은 호환이 깨질 수 있고(ADR 0016) 바깥 호출자가 없으므로 `#[deprecated]` 한 판을 두지 않습니다. 변경 기록에 밝힙니다.

### 메모리

맵은 열린 프로젝트와 같이 삽니다. 크기는 지금도 `open`이 한 번 쓰는 64 MiB 예산 안이고, 세 흐름이 각자 64 MiB까지 쓰던 것이 하나로 줄어듭니다. `OriginalSnapshot`과 내보내기는 `Arc`만 복제합니다. 예산이 바닥나면 지금처럼 나머지 사이드카를 건너뛰되 경고로 보이게 합니다.

## 시험

깊어진 interface인 저장소에서 먼저 씁니다. `record.rs`의 `SOURCE_PARSE_COUNT` 시험 훅이 파싱 횟수를 셉니다.

- 사이드카 셋이 있는 프로젝트를 `open`하면 파싱이 세 번이고, 그 뒤 `source_of`를 몇 번 불러도 더 늘지 않습니다. `sources_by_position`이 `open` 전의 `load_project_sources` 결과와 같은 값을 돌려줍니다(골든 `service_mdat`의 입력이 바뀌지 않음).
- `record`로 사이드카가 있는 측정을 넣으면 `source_of`가 그것을 돌려주고 디스크의 파일과 같습니다. `replace_source(.., None)`은 맵과 파일에서 함께 사라집니다. `remove_position` 뒤 `source_of`는 `None`이고 `restore_position` 뒤 돌아옵니다.
- 16 MiB를 넘는 사이드카, JSON이 아닌 사이드카, 예산을 바닥내는 사이드카 묶음을 열면 `warnings()`에 `measurement.source.skipped` 또는 `measurement.source.budget`과 그 항목 이름이 있고, 나머지 측정의 지문은 그대로입니다(TD-183).
- `secs-service`: 마이크 보정 `attach`가 세션 잠금을 쥔 동안 사이드카 파싱 횟수가 0이고(저장만 함), 곁 mdat 잡의 `OriginalSnapshot::load`가 파싱을 하지 않습니다. mdat 내보내기 골든은 바이트가 같습니다.

## 이행

1. 저장소에 맵과 `SourceBudget`, `sources_by_position`, 새 `source_of`를 추가하고 `open`이 채우게 합니다. 옛 `load_project_sources`는 그대로 두어 호출자가 아직 돕니다. 파싱 횟수 시험.
2. `record`, `replace_source`, `remove_position`, `restore_position`의 유지와 시험.
3. 서비스 호출자 다섯 자리를 맵으로 바꾸고 `OriginalSnapshot`에 소스를 싣습니다.
4. `SourceSkip`과 경고(TD-183, A 항목).
5. `load_project_sources`, `load_project_irs`, `LoadedIr` 삭제와 변경 기록.

## 결과

- 사이드카 해석이 한 곳에서 한 번 일어나고 예산도 한 번 적용됩니다. 큰 프로젝트의 mdat 내보내기와 저장이 사이드카를 다시 읽지 않습니다.
- 건너뛴 사이드카가 보입니다. 그 측정은 지금처럼 보정 없이 계산되지만 사용자가 왜 그런지 압니다.
- 바뀌지 않는 것: `.src.json`의 형식, `.secs`의 바이트, 엔진이 받는 IR, mdat 골든.
- 위험: 바깥 프로그램이 열린 프로젝트의 사이드카를 바꾸면 다음 열기까지 옛 값을 씁니다. 지금도 지문은 그랬으므로 새 동작이 아니고, 측정 폴더는 앱이 소유한다고 F5가 정합니다.
- 편차 항목은 없습니다. 업스트림은 사이드카를 쓰지 않습니다(ADR 0015). F23의 행에 경고 코드 둘을 덧붙입니다.
- 다음 감사는 `MeasurementStore`가 열 때 사이드카 전체를 읽는 것을 다시 지적하지 않습니다. 그것이 설계입니다.
