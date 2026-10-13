# 변경 기록

SECS 2R의 변경 기록입니다. 번호는 major.minor.patch이고, 0.x는 모두 사전 출시판이며 정식 출시는 1.0.0부터입니다(docs/adr/0006). 각 절의 내용이 그 버전의 릴리스 본문이 됩니다. 업스트림 SECS 2의 변경 기록은 [`docs/upstream/CHANGELOG.md`](docs/upstream/CHANGELOG.md)에 있습니다.

## 0.1.0 - 2026-10-13

M1(형식과 수치 프리미티브)의 나머지입니다. 아직 화면에 연결되지 않았습니다.

### 새 기능

- 차분 진화와 NumPy 레거시 `RandomState`(`secs-dsp`): SECS 2의 BA가 정수 시드로 부르는 `scipy.optimize.differential_evolution`과 그 난수원(MT19937)을 옮겼습니다. 난수열은 모든 플랫폼에서, 진화 과정은 오라클과 같은 수학 라이브러리에서 비트 단위로 같습니다.

## 0.0.1 - 2026-10-13

SECS 2 Beta 1.2.0 기준. 설치와 배포 경로를 시험하는 첫 사전 출시판이고, 측정과 보정 기능은 아직 없습니다.

### 빌드·설정

- 러스트 워크스페이스와 크레이트 뼈대(엔진, 수치, 형식, REW, 자체 측정, CamillaDSP, 서비스, 격리 크레이트)를 세웠습니다.
- 정책 게이트: unsafe 격리(`secs-sys`, 예산 0), 운영체제 의존성 위치, 기능 등록부, 서브프로세스 허용 목록, IPC 메서드 등록, AVX 상태를 더럽히는 커널 금지, 업스트림 트리 확인.
- CI: 포맷, clippy(Windows 대상 포함), 정책 게이트, Linux·Windows·macOS 테스트, 화면 타입 검사·테스트·빌드와 IPC 바인딩 일치, 세 운영체제 설치본, rustdoc, cargo-deny(GPL 호환 라이선스).
- 릴리스: main에 머지하면 Windows 설치본·포터블 zip, macOS dmg, Linux AppImage·deb를 사전 출시판으로 냅니다.
- 업스트림 감시: 업스트림에 새 커밋이 생기면 바뀐 파일과 닿는 크레이트를 적은 이식 이슈를 엽니다.
- 오라클 골든: 저장소의 업스트림 트리를 오라클로 돌려 골든을 남기는 도구(`oracle/`)와 그것을 읽는 테스트 전용 크레이트(`secs-oracle`), 수치 크레이트의 FFT 모듈.

### 새 기능

- REW `.mdat` 읽기·쓰기(`secs-jos`, `secs-mdat`): SECS 2와 바이트 단위로 같은 왕복과 빌드. 아직 화면에 연결되지 않았습니다.
- 수치 프리미티브(`secs-dsp`): 주파수 격자, 분수 옥타브 평활, 최소 위상, 필터 응답, 버터워스 SOS, FIR, 피크 찾기, 저속 투영, 경계 있는 TRF `least_squares`. 업스트림이 쓰는 NumPy·SciPy 결과와 골든으로 맞췄습니다.
- 빈 Tauri 셸: 앱 버전, 기준 업스트림 판, 플랫폼을 보여 줍니다. 한국어와 영어.
- `secs2r-cli bootstrap`: 같은 정보를 JSON으로 출력합니다.
