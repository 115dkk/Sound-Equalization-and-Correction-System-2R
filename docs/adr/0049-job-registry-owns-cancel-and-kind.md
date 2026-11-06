# 0049. 잡 레지스트리가 보조 취소 토큰과 '실행 중인 종류'를 소유

상태: 제안, 2026-11-06

감사 이슈 #186, 아키텍처 리뷰 A8. 함께 해결하는 기술 부채: 직접 해결하는 항목은 없고, TD-207의 `host.rs`·`curves.rs` 삭제(A20, A21)와 같은 PR에서 `ipc.rs`의 특례가 사라집니다.

## 배경

잡의 수명과 취소는 `JobRegistry`(`crates/secs-service/src/jobs.rs`)가 맡습니다. 레코드마다 `CancelToken`과 측정용 두 번째 토큰 `measurement_cancel`을 가지고(`jobs.rs:49-55`), `cancel(id)`가 둘을 함께 취소합니다(`jobs.rs:239-243`).

그런데 '잡을 취소한다'가 한 곳이 아닙니다.

- 레벨 모니터는 `LevelState`에 잡 ID를 거울처럼 복사하고(`monitor.rs:16-26, 89-102`), 재생 요청마다 세 번째 토큰 `PlayRequest.cancel`을 만듭니다(`monitor.rs:29-34, 160-180`). 그래서 `JobCancel` 요청은 `ipc.rs:44-47`에서 `cancel_level_monitor_job`을 먼저 불러 재생 토큰을 취소한 뒤에야 레지스트리를 부릅니다. `stop_level_monitor`(`monitor.rs:105-113`)도 같은 두 단계입니다.
- 상태 감시는 `WatchState.job`을 잡 안(`watch.rs:22`)과 밖(`watch.rs:44`)에서 두 번 기록하고 끝나면 지웁니다. `stop_status_watch`는 그 사본으로 레지스트리를 부릅니다.

deletion test를 적용하면 `LevelState.job`과 `WatchState.job`을 지웠을 때 다시 나타나야 하는 것은 '이 종류의 잡이 지금 돌고 있는가'라는 질문 하나뿐입니다. 레지스트리는 이미 `running_in_device_group`과 `running_measurement`로 비슷한 질문에 답합니다(`jobs.rs:260-278`). 2026-11-06 현재 코드도 리뷰가 적은 그대로입니다.

## 결정

### 레지스트리가 보조 취소 토큰을 받습니다

```rust
impl JobContext {
    /// 이 잡의 취소에 함께 취소될 토큰을 등록합니다. 다시 부르면 이전 토큰을 대체합니다.
    pub fn attach_cancel(&self, token: crate::measure::CancelToken);
}
```

레코드에 `attached: Option<crate::measure::CancelToken>`이 생기고, `JobRegistry::cancel`은 `cancel`, `measurement_cancel`, `attached`를 모두 취소합니다. 레벨 모니터는 재생 요청을 받을 때(`level_play`) 새 토큰을 만들어 `job.attach_cancel(token.clone())`으로 등록합니다. 재생이 끝나면 등록을 지울 필요가 없습니다. 취소된 잡은 끝나고, 다음 요청이 대체합니다.

`PlayRequest.cancel`은 남습니다. '모니터는 두고 재생만 멈추기'(`level_stop`)는 잡 취소가 아니기 때문입니다. 사라지는 것은 `cancel_level_monitor_job`, `ipc.rs:45`의 특례, `stop_level_monitor`의 두 단계입니다. `stop_level_monitor`는 `jobs.cancel` 한 번이 됩니다.

### 레지스트리가 '실행 중인 종류'에 답합니다

```rust
impl JobRegistry {
    /// 끝나지 않은 그 종류의 잡. 종류마다 하나만 돌 수 있는 그룹 규칙 덕에 하나입니다.
    pub fn running(&self, kind: JobKind) -> Option<JobId>;
}
```

`LevelState.job`과 `WatchState`는 지웁니다. `stop_level_monitor`는 `running(JobKind::LevelMonitor)`, `stop_status_watch`는 `running(JobKind::StatusWatch)`를 씁니다. `monitor.rs:89-102`의 '잡 안에서 기록, 끝나면 비교해 지움' 의식과 `watch.rs:22, 36-39, 44`의 같은 의식이 사라집니다. `running_measurement`는 `running`으로 다시 쓰되 '레벨 모니터를 뺀 장치 그룹' 의미는 그대로 둡니다.

`LevelInner.play`와 `next_generation`은 모니터의 상태(재생 큐)이지 잡의 신원이 아니므로 남습니다.

### 바뀌지 않는 것

잡 이벤트와 스냅샷, `JobCancel`의 응답, 그룹 규칙(`group(kind)`), 측정 토큰 `measurement_cancel`의 의미, wire.

## 시험

깊어진 interface에서 먼저 씁니다.

- `jobs.rs` 단위 시험: `running(kind)`가 돌고 있는 잡의 ID를 돌려주고 끝난 뒤에는 `None`인지. `attach_cancel`로 등록한 토큰이 `cancel`에서 함께 취소되는지, 두 번 등록하면 마지막 것만 취소되는지(첫 토큰은 그대로).
- `tests/monitor.rs`: `JobCancel` 한 요청만으로 재생 중이던 `level_play`가 `level_play_end`(`stopped`)로 끝나고 모니터 잡이 종료 상태가 되는지. 지금 `tests/monitor.rs:838`의 사례가 두 단계 취소를 전제로 한다면 이 사례로 바꿉니다.
- `tests/monitor.rs`: `level_stop`은 재생만 멈추고 모니터 잡은 계속 도는지(지금 동작의 고정).
- 상태 감시: `stop_status_watch`가 `running`으로 찾은 잡을 취소하고, 돌지 않을 때 `None`을 돌려주는지.
- 얕은 시험 삭제: `LevelState.job`·`WatchState.job`의 기록을 보던 시험이 있으면 지웁니다(이번 조사에서는 없었습니다).

## 이행

1. `JobRegistry::running`, `JobContext::attach_cancel`, 레코드의 `attached` 필드와 `cancel`의 처리. 단위 시험 둘. 한 커밋(Daybreak).
2. 레벨 모니터: `attach_cancel` 등록, `LevelState.job`·`cancel_level_monitor_job` 삭제, `stop_level_monitor` 단순화, `ipc.rs:44-47` 특례 삭제. `tests/monitor.rs` 사례 교체. 한 커밋.
3. 상태 감시: `WatchState` 삭제, `running`으로 교체. 한 커밋.

각 단계 뒤 `cargo test -p secs-service --locked`가 통과해야 합니다. 리뷰에서는 `ipc.rs`의 `JobCancel` arm이 `service.jobs.cancel` 한 줄인지, `monitor.rs`에 잡 ID를 들고 있는 필드가 남지 않았는지, `level_stop`의 동작(모니터 유지)이 바뀌지 않았는지 확인합니다.

## 결과

- 잡의 수명과 취소가 `jobs.rs` 한 곳에 있습니다. 새 잡 종류가 보조 토큰을 쓰려면 `attach_cancel` 한 줄입니다.
- `ipc.rs`의 특례 하나, 모니터·감시의 거울 필드 둘, 취소 도우미 하나가 사라집니다.
- 위험: 종류마다 잡이 하나라는 `running`의 전제는 그룹 규칙(`group(kind)`, 같은 그룹은 동시에 하나)에 기댑니다. 같은 종류의 잡이 둘 돌 수 있게 그룹 규칙을 바꾸는 날에는 `running`이 `Vec`를 돌려줘야 합니다. 이 ADR이 그 전제를 기록합니다.
- 편차 항목은 없습니다.
- 다음 리뷰가 다시 제안하지 않을 것: `PlayRequest.cancel`이 레지스트리 밖에 남는 것(재생만 멈추는 동작이 이유), `measurement_cancel`이 별도 토큰으로 남는 것(오디오와 REW 두 백엔드의 토큰 묶음이 이유).
