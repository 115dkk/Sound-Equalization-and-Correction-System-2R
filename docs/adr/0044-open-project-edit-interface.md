# 0044. 열린 프로젝트의 편집 interface

상태: 제안, 2026-11-06

감사 이슈 #186, 아키텍처 리뷰 A3. 해결하는 기술 부채 발견: TD-213(`result_lines`의 재잠금). 관련 ADR: 0009(프로젝트 교체와 티켓), 0013(세션 잠금 밖 읽기).

## 배경

`crates/secs-service/src/session.rs:22-34`의 `OpenProject`는 `path`, `store`, `dirty`, `tutorial`, `tutorial_folder`, `results`, `undo`를 모두 `pub` 필드로 둡니다. `identity`만 private이고, `commit`과 `apply`(`:270-293`)만 `run_change`를 거쳐 클로저의 `(T, bool)`로 `dirty`를 정합니다(`:296-305`).

그래서 더티를 정하는 규약이 둘입니다. 즉시 편집 핸들러는 필드에 직접 씁니다. `open.dirty = true`가 `project_edit.rs`에 12번, `measurement.rs`에 2번, `curves.rs`·`settings_edit.rs`·`export_folder.rs`·`monitor.rs`·`mic_calibration.rs`에 한 번씩 있습니다. undo 스택은 `session.rs:312-335`가 쌓고 `measurement.rs:673-717`이 바깥에서 훑어 `remove`합니다. `open.store.project_mut()`은 열 파일에서 불립니다(`project_edit.rs` 40회, `measurement.rs` 22회, `mic_calibration.rs` 20회).

잠금 순서도 흩어져 있습니다. `app_settings` 잠금(`acceleration()`) 뒤에 `session` 잠금이라는 순서를 `apply.rs:419`, `calibration.rs:55, 215, 261`, `curves.rs:259, 498`, `export.rs:106`의 일곱 자리가 각자 지키고, `export_folder.rs:11`이 다시 잠그기 때문에 `export.rs:224, 426`이 `drop(session)`을 손으로 합니다. `curves.rs:496-500`의 `result_lines`는 `self.results()`(잠금, 요약, 해제)를 부른 뒤 세션을 다시 잠가, 그 사이에 커밋이 들어오면 요약의 `notes`·`stale`과 선이 다른 상태를 표시합니다(TD-213).

`OpenProject`는 seam이지만 불변식을 지키는 메서드가 없어서, 핸들러 파일 열여섯이 `Service`와 `OpenProject`의 필드 배치에 묶여 있습니다.

## 결정

열린 프로젝트의 편집, 더티 판정, undo, 잠금 순서를 `session.rs` 한 곳에 둡니다.

### OpenProject의 interface

```rust
impl OpenProject {
    pub(crate) fn project(&self) -> &Project;
    pub(crate) fn store(&self) -> &MeasurementStore;
    /// Applies `change` and marks the project dirty.
    pub(crate) fn edit<T>(&mut self, change: impl FnOnce(&mut Project) -> T) -> T;
    /// Applies `change`; the second value says whether anything changed.
    pub(crate) fn edit_if<T>(&mut self, change: impl FnOnce(&mut Project) -> (T, bool)) -> T;
    /// The measurement store's writes (record, replace_source, remove) go through here.
    pub(crate) fn store_mut(&mut self) -> StoreEdit<'_>;
    pub(crate) fn record_undo(&mut self, payload: UndoPayload) -> String;
    pub(crate) fn take_undo(&mut self, token: &str) -> Option<UndoPayload>;
    pub(crate) fn is_dirty(&self) -> bool;
    pub(crate) fn mark_saved(&mut self);
}
```

- `dirty`, `undo`, `store`는 private으로 내립니다. `path`, `tutorial`, `tutorial_folder`, `results`는 읽기용 `pub(crate)`로 남습니다. `StoreEdit`는 `MeasurementStore`의 쓰기 메서드를 그대로 가진 얇은 래퍼이고, 드롭될 때 더티를 정합니다. 그래서 측정을 기록하거나 지우는 핸들러가 `dirty`를 잊을 수 없습니다.
- `commit`과 `apply`의 `run_change`는 `edit_if`를 부르는 것으로 바뀌고, 더티 판정은 `edit`·`edit_if`·`StoreEdit` 세 메서드만 합니다. `open.dirty = true` 19곳은 모두 사라집니다.
- undo 스택의 상한 16과 토큰 형식(`<identity>-<n>`)은 그대로이고, `measurement.rs:673-717`의 `undo` 핸들러는 `take_undo(token)`을 불러 페이로드만 받습니다.

### Service의 접근자

```rust
impl Service {
    /// Locks app settings before the session (the one place that order lives)
    /// and hands the handler the open project and the compute identity.
    pub(crate) fn with_open<T>(
        &self,
        f: impl FnOnce(&mut OpenProject, &ComputeIdentity) -> Result<T, IpcError>,
    ) -> Result<T, IpcError>;
    /// The same for a job that holds a ticket (ADR 0009).
    pub(crate) fn with_open_ticket<T>(&self, ticket: ProjectTicket, f: …) -> Result<T, IpcError>;
    /// A read-only view that never re-locks: summary and lines come from one lock.
    pub(crate) fn read_open<T>(&self, f: impl FnOnce(&OpenProject, &ComputeIdentity) -> T) -> Result<T, IpcError>;
}
```

- `acceleration()`을 먼저 부르는 관례 일곱 자리가 `with_open` 하나로 모입니다. `export_folder.rs:11`의 재잠금은 `with_open` 안에서 경로만 정하는 함수로 바뀌어 `export.rs`의 손 `drop(session)` 둘이 사라집니다.
- `result_lines`는 `read_open` 안에서 `open.results.summary(..)`와 선을 함께 만듭니다(TD-213). `compute_and_commit_mimo`의 아홉 인자는 `MimoRun { design, settings, selection, calibration_key, background }` struct 하나가 됩니다(ADR 0048의 커밋 셋과 함께 정리합니다).

바뀌지 않는 것: wire, 프로젝트 파일, 결과 장부의 판정, 잡의 티켓 규칙(ADR 0009).

## 시험

`OpenProject`의 interface에서 먼저 씁니다(`session.rs`의 시험 module).

- `edit`는 더티로 표시하고 `edit_if`는 둘째 값이 `false`면 표시하지 않습니다. `mark_saved` 뒤 `is_dirty`는 `false`입니다.
- `store_mut()`으로 측정을 기록하면 드롭 뒤 더티입니다.
- `record_undo`는 토큰을 돌려주고 `take_undo`는 같은 토큰에 한 번만 페이로드를 돌려주며, 열일곱째 항목에서 가장 오래된 것이 밀려납니다.
- `with_open` 안에서 `self.acceleration()`을 다시 불러도 교착하지 않습니다(지금 `tests/service.rs:4589-4736`의 잠금 순서 회귀 시험이 하는 일을 접근자 하나의 시험으로 줄입니다).
- `result_lines`의 요약과 선이 같은 잠금에서 나온다는 시험: 잠금 사이에 커밋을 끼워 넣는 기존 시험 패턴(`tests/service.rs:4008`)을 재사용합니다.
- 핸들러의 기존 시험은 바뀌지 않습니다. 관측 가능한 동작이 같기 때문입니다.
- 지우는 시험: `tests/service.rs`의 잠금 순서 회귀 시험 가운데 접근자 시험이 덮는 것.

## 이행

1. `OpenProject`에 메서드를 추가하고 시험을 먼저 씁니다. 필드는 아직 `pub`입니다. 커밋 하나.
2. 핸들러 파일을 하나씩 `edit`·`edit_if`·`store_mut`으로 옮깁니다. `project_edit.rs`, `measurement.rs`, `mic_calibration.rs`, 나머지 다섯 파일 순서로 커밋 넷. 각 커밋 뒤 `cargo test -p secs-service`가 그대로 통과해야 합니다.
3. `dirty`, `undo`, `store`를 private으로 내립니다. 컴파일러가 남은 직접 접근을 모두 지목합니다. 커밋 하나.
4. `with_open`·`with_open_ticket`·`read_open`을 두고 일곱 자리의 잠금 순서와 `export_folder.rs`의 재잠금, `result_lines`를 옮깁니다. 커밋 하나(TD-213).

작업 분배: Daybreak(서비스). Claude가 커밋마다 디프를 검토하며, 검토가 보는 것은 옮긴 핸들러에서 더티가 되는 조건이 전과 같은지(전에 `dirty = true`였던 자리는 `edit`, 조건부였던 자리는 `edit_if`), 잠금이 두 번 잡히는 자리가 남지 않았는지입니다.

## 결과

- 더티 판정, undo, 잠금 순서가 `session.rs` 한 곳에 모이고, 잠금 순서 위반과 더티 누락은 컴파일러가 막는 형태가 됩니다.
- 핸들러 파일은 `Service`와 `OpenProject`의 필드 배치를 모릅니다. ADR 0043의 `plan_export`와 ADR 0048의 커밋 셋은 이 접근자 위에 올라갑니다.
- 계산과 파일은 바뀌지 않으므로 편차 항목은 없습니다.
- 위험: `StoreEdit`가 드롭 때 더티를 정하므로 쓰기 메서드를 부르지 않고 드롭만 해도 더티가 됩니다. 쓰기 메서드가 실제로 불렸는지 플래그로 세어 그때만 더티로 표시합니다.
- 다음 감사는 `Deps`(`lib.rs:377-427`)가 넓은 것을 다시 제안하지 않아야 합니다. 그것은 생성자 다섯에 인자 21개가 다시 나타나는 것을 막는 자리이고, ADR 0053(네이티브 임베더 하나)이 넓어지는 속도를 늦춥니다.
