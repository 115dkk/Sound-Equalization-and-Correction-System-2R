# 0043. 내보내기 계획 하나와 종류별 작성기 adapter

상태: 제안, 2026-11-06

감사 이슈 #186, 아키텍처 리뷰 A2. 해결하는 기술 부채 발견: TD-195(`export.rs`의 642줄 함수 부분), TD-196(`begin_commit`이 리샘플링보다 앞에 있음), TD-155(잠금 아래의 장치 열거와 사이드카 읽기), TD-211(상대 폴더). 관련 ADR: 0006(릴리스), 0016(공개 API), 0018(CamillaDSP·Equalizer APO 적용), 0021(설계 식별과 슬롯).

## 배경

내보내기 한 번이 무슨 파일을 만드는지 알려면 `crates/secs-service/src/export.rs:95-737`(`start_export_with`와 255줄의 잡 클로저), `export_run.rs:46-238`, `export_folder.rs:10-52`, `design_export.rs:177-618`, `deploy.rs:12-58`, `apply.rs:100-233, 254-339`, `results.rs:234-301`의 열 파일 약 1,500줄을 순서대로 읽어야 합니다.

`start_export_with`를 호출하려면 알아야 할 조건이 구현 내부만큼 많습니다. 호출자는 `params.folder`가 자기가 지운 staging 경로여야 하고, `design_name`이 `None`이어야 하며, 완료 클로저 `F: FnOnce(ExportResult, Option<String>)`에 오는 설계 ID가 필터 종류에서만 `Some`이라는 사실을 알아야 합니다. `job_kind`와 `record_export`는 늘 짝으로 다닙니다(`Export, true` 아니면 `Apply, false`). `apply.rs:113-129`와 `:267-275`는 `result.files`에서 `_bypass.yml`이 아닌 `.yml`을 찾아 주 설정을 되짚고, `Option<String>` 설계 ID를 `ok_or_else(Internal)`로 다시 확인합니다.

결과 장부 `ResultBook`(`results.rs`)이 낡음을 정하는데도 `export.rs:308-316`과 `:576-592`는 `run_key`를 손으로 두 번 다시 만들고, `open.results`를 아홉 번 따로 읽습니다. `begin_commit`은 종류마다 다른 자리(516, 537, 541, 555, 637)에서 불리고, CamillaDSP·필터 텍스트·Equalizer APO는 작성기가 FIR을 출력 레이트로 리샘플하는 동안 이미 커밋 상태라 취소할 수 없습니다(TD-196). 잠금 아래에서 운영체제 오디오 장치를 열거하고(`:195-199`) 모든 `.src.json` 사이드카를 읽으며(`:336`) `MimoPaths`를 최대 세 번 복제합니다(TD-155).

지난 감사의 High 여섯(TD-105, 106, 108, 110, 116, 133)은 모두 `export.rs`와 `apply.rs`의 순서 문제였고, 세대 설치(ADR 0018)로 증상은 고쳤지만 interface는 그대로입니다.

## 결정

내보내기를 '잠금 아래에서 계획 하나를 만들고, 종류별 작성기 adapter가 그 계획대로 디스크에 파일을 저장한다'는 두 단계로 나눕니다.

### ExportPlan

`crates/secs-service/src/export_plan.rs`에 둡니다.

```rust
pub(crate) struct ExportPlan {
    pub kind: ExportKind,
    pub source: DesignSource,
    pub mimo: Option<MimoSelection>,
    pub description: ExportDescription,
    pub destination: Destination,
    pub manifest: ManifestMeta,
    pub record: Option<LastExportRecord>,
    pub output_rate: u32,
    pub extra_rates: Vec<u32>,
    pub camilla_devices: Option<CamillaDevices>,
    pub mdat_scope: MdatScope,
    pub originals: Option<OriginalSnapshot>,
}

pub(crate) enum DesignSource {
    Live(Arc<CalibrationRun>),
    Cached(Arc<ChainSnapshot>),
    Replay { snapshot: Arc<CacheSnapshot>, expected_key: String },
}

pub(crate) struct Destination {
    pub staging: PathBuf,
    pub final_folder: PathBuf,
    pub index_root: Option<PathBuf>,
}
```

- `Service::plan_export(&self, params: ExportParams, purpose: ExportPurpose) -> Result<ExportPlan, IpcError>`가 세션 잠금 아래에서 계획을 만듭니다. `ExportPurpose::{Export, Apply}`가 지금의 `(job_kind, record_export)` 짝을 대신합니다. `Apply`면 `record`는 `None`이고 `index_root`도 `None`이라 설계 색인을 만들지 않습니다(TD-153과 같은 결과).
- 잠금 아래에서 하는 일은 결과 장부에서 `DesignSource`를 고르고, 경로와 메타를 정하고, 사이드카 경로 목록을 잡는 것까지입니다. 장치 열거(`resolve_output_rate`)는 짧은 잠금으로 `dsp.output_rate`만 읽은 뒤 잠금 밖에서 하고, 사이드카 파일은 잡 안에서 읽습니다(TD-155). FIR을 가진 값은 `Arc`로 감싸 작성기·manifest·설명이 한 사본을 공유합니다.
- 상대 `ExportParams::folder`는 `plan_export`가 `InvalidRequest`(`reason=relative_folder`)로 거절합니다(TD-211).

### ExportWriter adapter

```rust
pub(crate) trait ExportWriter: Send {
    /// Everything that can fail or take long: resampling, rendering, mdat bytes.
    fn prepare(&mut self, plan: &ExportPlan, job: &JobContext) -> Result<(), IpcError>;
    /// Stores the prepared bytes under `plan.destination.staging`; the job is committed.
    fn write(self: Box<Self>, plan: &ExportPlan, job: &JobContext) -> Result<ExportResult, IpcError>;
}

pub(crate) struct ExportResult {
    pub folder: PathBuf,
    pub files: Vec<PathBuf>,
    pub main_config: Option<PathBuf>,
    pub design_id: Option<String>,
    pub bytes: u64,
}

pub(crate) fn writer_for(kind: ExportKind) -> Box<dyn ExportWriter>;
```

- adapter는 여섯입니다. `CamillaWriter`, `StagedMdatWriter`, `BeforeAfterMdatWriter`, `FilterTextWriter`, `EqapoWriter`, `ConvolutionWriter`. 여섯이므로 진짜 seam입니다.
- `Service::start_export(plan, events, complete)`가 잡을 시작합니다. 잡 본문은 `writer.prepare(&plan, &job)?; job.begin_commit()?; writer.write(plan, &job)` 세 줄이고, `begin_commit`은 한 번만 부릅니다. 리샘플링과 렌더는 `prepare`에 있으므로 커밋 전에 취소할 수 있습니다(TD-196).
- 완료 클로저는 `FnOnce(ExportResult) -> Result<JobResult, IpcError>`입니다. `apply.rs`는 `result.main_config`와 `result.design_id`를 읽고, 파일 목록을 훑거나 `Option`을 다시 확인하지 않습니다. 필터 종류의 작성기만 `design_id`에 `Some`을 넣으며, 그것은 작성기가 보장하는 사후 조건입니다.
- 설치(`deploy::install`), manifest, 색인은 작성기 뒤의 공용 단계 `install_bundle(&plan, result)`에 있고 호출자는 보지 않습니다.

### 결과 장부가 키를 만들어 갖습니다

`crates/secs-service/src/results.rs`에 둡니다.

```rust
pub(crate) struct ProjectKeys { pub config: String, pub signature: String, pub target: String }
impl ProjectKeys { pub(crate) fn of(project: &Project) -> Self; pub(crate) fn run(&self, settings: &CalibrationSettings) -> String; }
impl ResultBook {
    pub(crate) fn inputs_unchanged_since(&self, keys: &ProjectKeys, settings: &CalibrationSettings) -> bool;
}
```

`calibration.rs:1168-1240`의 `config_key`, `run_key`, `signature_key`, `target_key_string`은 이 struct의 메서드가 되고, `export.rs`는 키를 다시 만들지 않습니다. 키 문자열 자체는 바뀌지 않습니다(TD-166은 따로 정책만 문서화합니다).

바뀌지 않는 것: 내보낸 파일의 바이트(골든), 파일 이름과 폴더 규칙(ADR 0021), 세대 설치(ADR 0018), `ExportKind`와 `ExportParams`의 wire, 공개 API의 `export` 메서드.

## 시험

작성기 interface에서 먼저 씁니다.

- `crates/secs-service/src/export_plan_tests.rs`: 오라클 견본 프로젝트(`oracle/goldens/service_new/sample_project.secs`)로 종류마다 `ExportPlan`을 만들고 `writer_for(kind)`가 파일을 저장한 폴더의 파일 목록, `main_config`, `design_id`를 지금 `tests/service.rs`의 내보내기 시험(1031-1500, 2355-2535, 3901-4007)이 단언하던 리터럴과 비교합니다. 바이트는 그 시험들이 이미 골든과 맞추고 있으므로 같은 골든을 씁니다.
- `plan_export`의 거절 사례: 상대 폴더, mdat 종류에 `design_name`, CamillaDSP가 아닌 종류에 `camilla_devices`, 튜토리얼 세션. 각각 `reason` 매개변수까지 단언합니다.
- 취소 사례: `prepare` 도중 취소한 잡은 staging 폴더에 파일을 하나도 남기지 않습니다(TD-196의 시험).
- `ResultBook::inputs_unchanged_since`의 시험: 구성·설정·타겟이 바뀐 뒤 `false`, 그대로면 `true`. `ProjectKeys::of`의 문자열은 리터럴로 고정합니다.
- 옮기는 시험: `tests/service.rs`의 긴 내보내기 시험은 종류별 파일(`tests/export.rs`)로 옮기고 첫 `reopen`에서 끊습니다(TD-167). 시험 수는 줄지 않습니다.

## 이행

1. `ProjectKeys`와 `inputs_unchanged_since`를 `results.rs`에 두고 시험을 먼저 씁니다. `export.rs`의 두 `run_key` 재계산을 바꿉니다. 커밋 하나.
2. `ExportResult`에 `main_config`와 `design_id`를 두고 작성기 호출 뒤에 값을 넣습니다. `apply.rs`의 `.yml` 되짚기와 `ok_or_else`를 지웁니다. 커밋 하나.
3. `ExportPlan`과 `plan_export`를 만들고 `start_export_with`의 앞 300줄을 옮깁니다. 장치 열거와 사이드카 읽기를 잠금 밖으로 냅니다. 커밋 하나(TD-155, TD-211).
4. 작성기 adapter를 종류 하나씩 뽑습니다. CamillaDSP, mdat 둘, 필터 텍스트, Equalizer APO, 컨볼루션 순서이고 커밋 여섯. 매 커밋 뒤 `cargo test -p secs-service -p secs-camilla -p secs-filters -p secs-export`의 골든이 모두 통과해야 합니다.
5. `start_export_with`와 잡 클로저를 지우고 `start_export(plan, …)`만 남깁니다. CLI(`apps/secs2r-cli`)와 공개 API(`secs-api`)는 같은 `plan_export`를 거칩니다. 커밋 하나.

작업 분배: Daybreak(서비스). Claude가 커밋마다 디프를 검토하며, 검토가 보는 것은 작성기 안으로 들어간 코드가 글자 그대로 옮겨졌는지(수치·바이트를 만드는 줄은 바뀌지 않아야 합니다), `begin_commit`이 한 자리에만 남았는지, `apply.rs`가 파일 이름을 더는 보지 않는지입니다.

## 결과

- 파일 묶음, 세대 설치, manifest, 색인이 작성기 adapter 뒤로 들어가고, 내보내기 한 번이 만드는 것은 `ExportPlan` 하나와 `writer_for` 하나를 읽으면 알 수 있습니다.
- 공개 API와 CLI가 같은 계획을 받으므로 세 입구의 동작이 같아집니다.
- 내보낸 바이트는 바뀌지 않으므로 편차 항목은 없습니다.
- 위험: 작성기로 옮기는 동안 커밋 시점이 뒤로 가므로(리샘플링 뒤) 취소 시험이 반드시 있어야 하고, `apply.rs`가 `design_id`를 작성기의 사후 조건으로 믿으므로 필터 작성기 셋의 시험이 `design_id.is_some()`을 단언해야 합니다.
- 다음 감사는 `make_config_observed`(`secs-camilla`, 약 780줄)를 나누라고 다시 제안하지 않아야 합니다. 그 함수는 바이트 단위 골든이 고정하는 YAML 한 장이고, 이 ADR은 그 앞에 adapter를 둘 뿐 안을 나누지 않습니다(ADR 0045가 도우미만 옮깁니다).
