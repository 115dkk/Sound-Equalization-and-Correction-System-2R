# P53. 실제 f64 GPU 가속, 자동 선택과 CLI·화면

기준: `codex/gpu-acceleration`, 0.19.1 이후.
작업 공간: 이 채팅의 별도 managed worktree. 원래 E: 체크아웃은 바꾸지 않습니다.
결정: ADR 0008. upstream Python 트리는 수정하지 않습니다.

## 분담과 소유

- 조정자: ADR·공용 타입/시그니처, policy gates, 기능 등록부, 문서·변경 기록,
  최종 검토·검증·PR·머지·릴리스 확인.
- 수치 워커(ASTRA): `crates/secs-cuda-sys/**`, `crates/secs-gpu/**`,
  `crates/secs-engine/src/mimo/**`, 필요한 엔진 테스트·예제. Cargo.lock도
  이 워커가 새 의존성(libloading 0.9)을 잠급니다.
- 서비스 워커: `crates/secs-service/**`, `apps/secs2r-cli/**`,
  `apps/secs2r/src/main.rs`, 두 앱의 Cargo.toml.
- 화면 워커: `apps/secs2r/ui/src/**` 중 생성된 `ipc/bindings/**` 제외.
  기존 CSS 체계와 ko/en 키를 사용하고 설치된 make-interfaces-feel-better를
  읽습니다. 바인딩은 조정자가 secs-types 테스트로 생성합니다.

같은 작업 공간을 공유합니다. 다른 담당자의 변경을 되돌리지 않고,
계약 변경은 먼저 서로 알립니다. 워커는 커밋/PR을 하지 않습니다.

## 공용 계약

`secs-types::compute`가 `ComputePreference(Auto,Cpu,Gpu)`,
`ComputeBackend(Cpu,Cuda)`, `ComputeStatus`, `GpuDeviceInfo`,
`ComputeReason`, `ComputeReport`, `ComputeStage(IdealFirSweep)`를 가집니다.
정본 파일의 필드를 그대로 사용합니다.

- `AppSettings.compute`: serde 기본 Auto, 앱 로컬 영속 설정.
- `Request::ComputeStatus` / `Response::ComputeStatus(ComputeStatus)`.
- `Request::OpenGpuHelp` / `Response::OpenGpuHelp(())`: 현재 플랫폼의
  공식 CUDA 설치 안내를 Host opener로 엽니다. 임의 URL을 입력받지 않습니다.
- `MimoSummary.compute`: serde 기본 CPU report.
- `ErrorCode::ComputeUnavailable`: 명시적 GPU에 사용 가능한 장치가 없을 때.
- `secs-engine::mimo::AccelerationSelection`:
  `status: ComputeStatus`, `backend: Option<Arc<dyn RidgeBackend>>`.
- `secs-gpu::select(ComputePreference) -> AccelerationSelection`.
- `Deps.acceleration`:
  `Arc<dyn Fn(ComputePreference) -> AccelerationSelection + Send + Sync>`.
  기본 Deps는 CPU-only 테스트용입니다. 앱은 `secs_gpu::select`를 주입합니다.
- `mimo::compute_with_acceleration`: 기존 `compute`와 같은 인자 뒤에
  `&AccelerationSelection`을 받습니다. 기존 CPU 진입점은 유지합니다.
  반환 `MimoRun.compute: ComputeReport`는 실제 수행 경로입니다.
- `RidgeBackend: Debug + Send + Sync`:
  `factor<'a>(&'a self, ata: &'a [f64], atb: &'a [f64], cancel: &CancelToken)`
  `-> Result<Box<dyn ShiftedSystem + 'a>, AccelerationError>`.
  `ShiftedSystem::solve(&mut self, lambda: f64, cancel: &CancelToken)`
  `-> Result<Vec<f64>, AccelerationError>`. 계산 객체에는 Send를 요구하지
  않습니다. CPU는 기존 RidgeSweep의 산술을 유지합니다. 필요한 작은
  상태/이름/크기 정책 메서드는 조정자와 협의해 더합니다.

## 수치 구현

네이티브 CUDA runtime/cuSolver/cuBLAS 함수표를 선택적으로 동적 로드합니다.
SDK 없는 시스템에서도 Rust 빌드·기동이 되어야 하고, missing library/symbol은
Result로 처리합니다. 별도 Python, subprocess, nvcc/NVRTC 실행을 요구하지
않습니다. f64 고유분해로 같은 19개 λ 탐색을 처리합니다. 차원·버퍼 크기,
유한 값, 메모리 상한과 오버플로를 호출 전에 검사합니다.

CUDA 리소스는 소유권으로 정리하고 GPU lease는 19solve가 끝날 때까지
유지합니다. 대기는 취소 가능하며 중간 장치 오류는 입력의 λ 탐색 전체를
CPU로 재실행합니다. 취소는 CPU fallback 없이 반환합니다. 실제 backend,
stage, device, GPU/CPU 시스템 수와 fallback 수를 기록합니다. CPU의 기존
golden/결정성에 GPU를 자동 삽입하지 않습니다.

자동 적격 경계는 코드·문서·테스트에 명시합니다. integrated, 구형 세대,
낮은 SM 수/메모리, 미지원 OS, 누락 runtime, 디바이스 오류를 구분합니다.
최소 검증 장치는 RTX 2080 SUPER이며 실제 하드웨어 테스트/성능 비교를
다시 수행합니다. 여러 장치는 적격 여부를 먼저 고려한 안정적 순위로
선택합니다. 사용자의 CPU 선택은 강제 유지합니다.

## 서비스·CLI

GPU 상태 API, 설정 영속성과 마이그레이션, 실제 report를 서비스에 연결합니다.
명시적 GPU 사용 불가에는 ComputeUnavailable, 실행 중 실패에는 CPU로 복구한
실제 결과와 notice를 반환합니다. 취소 오류는 별도 유지합니다.
계산 선택은 MIMO cache/낡음 판정에 반영하고, 내보내기 주석/메타데이터에도
GPU stage와 CPU fallback을 남깁니다. 기존 CPU export golden은 유지합니다.

CLI에 `--compute auto|cpu|gpu`와 장치/선택 상태를 보여주는 `compute-info`를
연결합니다. CLI의 임시 선택 때문에 GUI의 저장 설정을 덮어쓰지 않습니다.
자동 기본값과 CPU 강제, GPU 강제 실패를 subprocess 통합 테스트로 검증합니다.
서비스 fake provider로 supported GPU, iGPU, legacy, runtime 없음, 변경 후
낡음, report/cache roundtrip을 검증합니다.

## 화면

보정 화면의 MIMO 설정에 계산 장치를 고르는 작은 제어를 둡니다.
자동/CPU/GPU와 실제 선택 장치·사유를 표시합니다. 지원하지 않는 GPU는
선택할 수 없고 자동은 CPU로 정상 동작합니다. 실행 중에는 선택 변경을
막습니다. 설정은 app_settings_set으로 보존합니다. 선택과 실제 결과를
혼동하지 않고 결과의 GPU stage/CPU 복귀를 필요할 때 보여줍니다.

장치 내부 키·원시 오류 대신 ko/en 문구를 쓰고, 오류 원문은 기존 detail에
둡니다. GPU API가 아직 로드 중인 상태, 지원 없음, 적격 GPU 기본값, CPU
수동 선택, 실행 중, fallback 상태를 테스트/화면 확인합니다. 기존 14px
UI 하한, 한 화면의 채운 기본 동작 하나, 키보드 포커스와 히트 영역을
지킵니다. 생성 바인딩을 손으로 고치지 않습니다.

## 완료 검증

실제 CPU/GPU 수치와 release 성능, fallback·취소·메모리·unsafe 경계 검토,
서비스·CLI 통합 테스트, UI typecheck/test/build 및 화면 검증을 수행합니다.
CLAUDE.md의 전 게이트와 PR의 OS별 CI를 통과시킨 뒤 머지합니다. release.yml
완료와 UI 갤러리 리뷰 링크도 확인합니다. 구현되지 않은 IIR GPU 경로를
구현됨으로 표시하지 않습니다.

## 끝단 검증에 따른 범위 확정

2026-10-19 실제 CUDA 비교에서 FIR 모드 세 방의 최종 SPL·늦은 에너지·
가드·경로 수가 일치했습니다. IIR 모드는 작은 이상 목표 차이가 후속
비선형 최적화에서 증폭되어 기준을 통과하지 못했습니다. 출하는 FIR 모드만
GPU를 적용하고, IIR은 목표 설계부터 CPU를 유지하며 `IirReference` 사유를
기록합니다. UI의 다음 계산 예고·지난 결과와 CLI report에 같은 범위를
표시합니다. 자동 크기 기준은 실측에 따라 1024차를 유지합니다.
