# 0048. 커밋 셋과 스냅샷 셋을 하나로

상태: 제안, 2026-11-06

감사 이슈 #186, 아키텍처 리뷰 A7. 함께 해결하는 기술 부채: TD-213의 `MimoRun` struct 부분(인자 아홉의 `compute_and_commit_mimo`), TD-149(A-2)가 지우는 `original_measurements`의 자리.

## 배경

서비스의 무거운 작업은 모두 같은 틀로 돌아갑니다. 세션 잠금 아래에서 경로와 프로젝트만 잡고(스냅샷), 잠금 밖의 잡에서 파일을 읽고 계산한 뒤, 티켓으로 잠금을 다시 잡아 결과를 커밋합니다(ADR 0024의 `ProjectTicket`, TD-37). 이 틀이 다섯 모양으로 쓰여 있습니다.

- 커밋 함수가 셋입니다(`crates/secs-service/src/session.rs:253-294`). `commit`은 `job.begin_commit()`으로 잡을 더는 취소할 수 없게 만든 뒤 바꾸고, `commit_phase`는 취소 여부만 확인해 다음 단계가 취소될 수 있게 두며, `apply`는 티켓만 봅니다. 셋의 차이는 취소 정책뿐인데 호출자는 레지스트리의 `cancellable` 깃발(`jobs.rs:126-139`)을 이해해야 맞는 것을 고릅니다. `calibration.rs:127-131`은 `run_mimo`로, `measurement.rs:606-612`는 `CommitBoundary`로 고릅니다.
- 스냅샷 struct가 셋이고 `load` 서명도 셋입니다. `OriginalSnapshot`·`MeasurementSnapshot`(`session.rs:571-583, 627-724`, `load(self, &JobContext, &MemoryBudget)`), 보정의 `Snapshot`(`calibration.rs:24-44, 398-416`, `load(self, &JobContext)`). 셋 다 `path`와 `project`를 들고 있습니다. `original_measurements`(`session.rs:726-769`)는 `OriginalSnapshot::load`와 같은 40줄 루프를 취소 없이 잠금 안에서 한 번 더 돕니다. 측정과 모니터는 `(project, ticket)` 튜플을 손으로 잡습니다(`measurement.rs:527-533, 869-875`, `monitor.rs:70-76`).
- 캐시 저장은 `persist_cache(cached, path, ticket, session, owner)`(`calibration.rs:1126-1166`)이고, 호출자마다 커밋 클로저 안에서 `(open.path.clone(), open.results.cache_snapshot())`를 돌려주는 의식을 되풀이합니다.
- 메모리는 잡 밖에서 `self.deps.memory.available_bytes()`로 읽어 잡 안의 `compute_context(&job, memory, background)`로 전달합니다. 다섯 번 같은 두 줄입니다(`calibration.rs:62/66, 162/175, 241/322`, `export.rs:202/231, 202/485`).

## 결정

### 커밋은 하나이고 정책은 이름으로 고릅니다

`session.rs`에 `Finality`를 둡니다.

```rust
pub(crate) enum Finality {
    /// 결과를 커밋하고 잡은 더는 취소되지 않습니다(`begin_commit`).
    Final,
    /// 끝난 단계를 커밋하되 다음 단계는 취소될 수 있습니다. 이 커밋은 되돌리지 않습니다.
    Phase,
    /// 돌아가면서 쓰는 잡(레벨 모니터, 역할마다 기록하는 REW 측정)의 쓰기. 티켓만 확인합니다.
    Streaming,
}

pub(crate) fn commit<T>(
    session: &Mutex<Session>,
    ticket: ProjectTicket,
    job: &JobContext,
    finality: Finality,
    change: impl FnOnce(&mut OpenProject) -> Result<(T, bool), IpcError>,
) -> Result<T, IpcError>;
```

`commit_phase`와 `apply`는 지웁니다. `apply`를 쓰던 자리는 모두 잡 안이므로 `Finality::Streaming`으로 바뀝니다. 취소된 뒤의 `Phase` 커밋이 `Cancelled`로 끝나는 규칙(`session.rs:265-280`)은 enum 변형의 문서가 됩니다.

### 결과 커밋은 캐시 저장까지 맡습니다

```rust
pub(crate) fn commit_results<T>(
    session: &Arc<Mutex<Session>>,
    ticket: ProjectTicket,
    job: &JobContext,
    finality: Finality,
    owner: CacheOwner,
    change: impl FnOnce(&mut OpenProject) -> Result<(T, bool), IpcError>,
) -> Result<T, IpcError>;
```

잠금 아래에서 `change`를 돌린 뒤 `(open.path.clone(), open.results.cache_snapshot())`를 스스로 잡고, 잠금을 놓고 `cache::save`를 부르며, 성공하면 `replace_cache`를, 실패하면 `cache_save_failed` 알림(TD-38)을 같은 티켓으로 반영합니다. 지금 `persist_cache`의 본문이 그대로 이 함수의 뒷부분이 되고, 호출자의 의식은 사라집니다. 보정, MIMO, 서브 전용 보정이 이것을 씁니다.

### 스냅샷은 하나입니다

```rust
pub(crate) struct ProjectSnapshot {
    pub ticket: ProjectTicket,
    pub path: PathBuf,
    pub project: Project,
}
impl Session {
    pub(crate) fn snapshot(&self) -> Result<ProjectSnapshot, ServiceError>;
}
impl ProjectSnapshot {
    pub(crate) fn load_measurements(&self, job: &JobContext, budget: &MemoryBudget) -> Result<MeasurementSet, IpcError>;
    pub(crate) fn load_originals(&self, job: &JobContext, budget: &MemoryBudget) -> Result<LoadedOriginals<'_>, IpcError>;
}
```

`OriginalSnapshot`, `MeasurementSnapshot`, `original_snapshot()`, `measurement_snapshot()`은 지웁니다. 보정의 `Snapshot`은 `ProjectSnapshot`에 설정 둘(`project_settings`, `settings`)을 추가한 struct로 남고, `load`는 `load_measurements`를 부른 뒤 `check_rc_agreement`와 키 계산만 합니다. 측정과 모니터의 `(project, ticket)` 튜플도 `snapshot()`으로 바뀝니다. `original_measurements`는 A-2(TD-149)가 곁 mdat을 잡으로 옮기면서 지웁니다. 그 잡은 `load_originals`를 씁니다.

### 메모리는 잡이 압니다

`JobRegistry::start`가 `Deps.memory`를 받아 시작 시점의 `available_bytes()`를 `JobContext`에 보관합니다(`JobContext::available_memory() -> u64`). `compute_context(job, background)`는 거기서 읽습니다. 서비스에는 `Service::start_job(kind, events, body)`를 두어 레지스트리에 `deps.memory`를 전달하는 자리를 하나로 합니다. 메모리를 읽는 시점은 지금과 같이 잡 시작 직전입니다.

### MIMO 호출은 struct 하나입니다

```rust
pub(crate) struct MimoRun {
    pub design: Arc<Design>,
    pub settings: CalibrationSettings,
    pub selection: AccelerationSelection,
    pub calibration_key: String,
    pub background: bool,
}
fn compute_and_commit_mimo(session, ticket, job, run: MimoRun) -> Result<(), IpcError>;
```

`#[allow(clippy::too_many_arguments)]`는 사라집니다.

### 바뀌지 않는 것

잡 이벤트, 알림 코드(`cache_save_failed`), 취소 의미(끝난 단계는 되돌리지 않음), `.secs`와 캐시 파일 형식, wire.

## 시험

깊어진 interface에서 먼저 씁니다. `session.rs:1073-1274`의 커밋 시험 여섯은 `Finality` 세 경우로 다시 묶습니다.

- `Final`: 낡은 티켓은 `begin_commit`과 `change` 전에 실패, 취소된 잡은 `change` 전에 `Cancelled`, 유효한 티켓은 `begin_commit` 뒤 `change`를 돌리고 `dirty`가 반환값을 따릅니다. 낡은 티켓이 취소보다 먼저 보고됩니다.
- `Phase`: 취소된 잡은 `Cancelled`로 끝나고 잡은 `cancellable`로 남습니다. 취소되지 않은 `Phase` 커밋 뒤 취소가 오면 그 커밋은 남고 다음 `Phase`가 실패합니다.
- `Streaming`: 티켓만 봅니다. 취소된 잡도 쓸 수 있습니다(레벨 모니터가 마지막 레벨을 기록하는 경우).
- `commit_results`: 캐시 저장 실패가 `owner`의 알림에 `cache_save_failed` 하나로 남고 성공한 다음 저장이 그것을 지웁니다. 커밋 뒤 다른 프로젝트가 열렸으면 메모리 사본은 바꾸지 않습니다(`open.path != path`).
- `ProjectSnapshot::load_originals`: 파일 사이마다 취소를 보고(`read_measurements` 진행) 역할 순서가 계획 순서인지. 지금 `OriginalSnapshot` 시험이 있으면 이름만 옮깁니다.
- `JobContext::available_memory`: 시작 시점의 값을 돌려주는지.
- 얕은 시험 삭제: `apply_checks_the_ticket_without_begin_commit`은 `Streaming` 사례로 대체됩니다.

## 이행

1. `Finality`와 `commit` 하나, `apply`·`commit_phase` 삭제, 호출자 교체(`calibration.rs`, `measurement.rs`, `monitor.rs`, `export.rs`). 한 커밋(Daybreak).
2. `commit_results`와 `persist_cache` 삭제. 한 커밋.
3. `ProjectSnapshot`과 두 `load`, 스냅샷 둘 삭제, 보정 `Snapshot` 축소. A-2(TD-149)가 먼저 `original_measurements`를 지운 뒤 진행합니다. 한 커밋.
4. `JobContext::available_memory`, `Service::start_job`, `compute_context` 서명 변경. 한 커밋.
5. `MimoRun`. 한 커밋.

각 단계 뒤 `cargo test -p secs-service --locked`(서비스 시험 전부, 60개 묶음 포함)가 통과해야 합니다. 리뷰에서는 `begin_commit`이 불리는 자리가 `Final`에만 있는지, `Streaming` 자리가 지금의 `apply` 자리와 일치하는지, 캐시 저장이 잠금 밖에서 일어나는지 확인합니다.

## 결과

- '스냅샷 → 잡 → 커밋' 틀을 함수 셋(`snapshot`, `commit`, `commit_results`)으로 쓸 수 있습니다. 새 잡은 이 셋만 알면 됩니다.
- `persist_cache`의 매개변수 다섯과 호출자의 의식, 스냅샷 struct 둘, 같은 루프 하나(`original_measurements`)가 사라집니다.
- 위험: `Streaming`이 취소된 잡의 쓰기를 허용하는 것은 지금 `apply`와 같은 의미이므로 동작 변화는 없지만, 새 호출자가 `Final` 대신 `Streaming`을 고르면 취소 뒤 쓰기가 생깁니다. 리뷰 체크리스트에 둡니다.
- 편차 항목은 없습니다. 업스트림에는 잡이 없습니다.
- 다음 리뷰가 다시 제안하지 않을 것: `commit_results`가 캐시 저장을 맡는 것(잠금 밖 저장이 이유), 보정 `Snapshot`이 `ProjectSnapshot` 위에 설정을 추가한 struct로 남는 것(설정 덮어쓰기는 보정만의 일).
