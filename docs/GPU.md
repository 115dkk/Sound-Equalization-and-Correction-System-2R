# GPU 계산

SECS 2R은 지원되는 외장 GPU에서 MIMO FIR 필터를 설계할 때 큰 선형계를
배정밀도(f64)로 풉니다. CPU 기준 계산은 그대로 사용할 수 있습니다.
가속 단계는 이상 FIR의 정칙화 탐색이며, IIR 비선형 맞춤과 나머지 보정이
모두 GPU에서 실행되는 것은 아닙니다.

IIR 모드는 이상 FIR 목표를 만드는 단계부터 CPU 기준 계산을 유지합니다.
실제 방 샘플에서 GPU의 미세한 목표 차이가 후속 IIR 최적화에서 증폭됐기
때문입니다. GPU를 선택해도 IIR 결과에는 CPU 실행과 그 이유가 표시됩니다.

## 기본 선택

계산 장치의 기본 설정은 **자동**입니다. 사용할 수 있는 GPU 런타임과 장치
속성을 확인해 적격 외장 GPU를 고르고, 내장·구형·저성능·미지원 장치에서는
CPU를 사용합니다. 직접 선택한 CPU/GPU 설정은 앱 로컬 설정으로 보존하며,
프로젝트를 다른 컴퓨터로 옮겨도 하드웨어 선택을 프로젝트에 강제하지 않습니다.

화면의 선택 상태는 실행 전에 고른 정책입니다. 결과에는 실제로 수행한
가속 단계, 장치, CPU/GPU 처리 수와 CPU 복귀 여부가 별도로 남습니다.
자동 모드에서 작은 문제는 CPU로 처리할 수 있습니다.

자동 선택의 하드웨어 기준은 외장 NVIDIA GPU, Compute Capability 7.5 이상,
SM 16개 이상, 장치 메모리 2 GiB 이상입니다. 이는 모델명별 성능 보증이 아닌
보수적인 초기 정책입니다. 지원 세대의 저자원 장치는 명시적 GPU 선택으로
사용할 수 있으며 실행 중 메모리 예산 검사를 거칩니다.

자동 모드는 선형계 차수가 1024 미만이면 CPU를 사용합니다. RTX 2080 SUPER
실측에서 512·720차는 CPU가 빨랐고, 840차는 비슷했으며 1024차부터 GPU가
빨랐습니다. 기본 FIR의 12경로×70탭 문제는 CPU로 처리할 수 있습니다.
명시적 GPU 선택은 이 크기 기준을 재정의합니다.

네이티브 경계는 최대 4096차와 workspace 1 GiB·현재 여유 메모리의 절반을
상한으로 검사합니다. 범위를 넘거나 메모리가 부족하면 해당 입력을 CPU로
다시 계산합니다.

현재 네이티브 백엔드는 **CUDA 13의 f64 cuSOLVER/cuBLAS**입니다.
Windows와 Linux의 지원되는 NVIDIA GPU를 사용합니다. macOS, AMD/Intel
장치처럼 이 백엔드를 지원하지 않는 환경은 CPU로 정상 동작합니다.

## 필요한 구성 요소

NVIDIA 디스플레이 드라이버만으로는 cuSOLVER/cuBLAS가 설치되지 않습니다.
GPU 계산을 사용하려면 CUDA 13 계열 런타임 라이브러리가 필요합니다.
공식 CUDA Toolkit을 설치하면 필요한 라이브러리를 함께 설치할 수 있습니다.

- [Windows CUDA 설치 안내](https://docs.nvidia.com/cuda/cuda-installation-guide-microsoft-windows/index.html)
- [Linux CUDA 설치 안내](https://docs.nvidia.com/cuda/cuda-installation-guide-linux/index.html)
- [CUDA 드라이버 호환성](https://docs.nvidia.com/deploy/cuda-compatibility/minor-version-compatibility.html)

SECS 2R 기본 설치본은 이 별도 런타임을 포함하지 않습니다. 런타임이 없으면
자동 설정에서 CPU를 사용하고, CLI와 화면에 설치 안내를 표시합니다.
GPU를 명시적으로 요청했는데 사용할 수 없으면 사용 불가 오류를 반환합니다.
런타임을 설치하거나 라이브러리 경로를 바꾼 뒤에는 앱을 다시 실행합니다.

다른 위치에 런타임을 설치했다면 `SECS_CUDA_LIBRARY_PATH`에 라이브러리
디렉터리의 절대 경로를 지정합니다. Windows CUDA 13.4의 예입니다.

```powershell
$env:SECS_CUDA_LIBRARY_PATH = 'C:\Program Files\NVIDIA GPU Computing Toolkit\CUDA\v13.4\bin\x64'
secs2r-cli --compute auto compute-info
```

GPU 런타임은 NVIDIA에서 받은 일관된 버전의 파일을 사용합니다. SECS 2R은
필요한 라이브러리나 심벌을 읽을 수 없으면 CPU로 동작하며, 임의의 DLL을
내려받거나 별도의 Python 프로세스를 실행하지 않습니다.

## CLI와 화면

```text
secs2r-cli --compute auto compute-info
secs2r-cli --compute cpu compute-info
secs2r-cli --compute gpu compute-info
```

`--compute auto|cpu|gpu`는 해당 CLI 실행에만 적용됩니다. GUI에 저장한
선택을 덮어쓰지 않습니다. 보정 화면의 MIMO 설정에서는 같은 정책과 장치
상태를 확인하고 선택할 수 있습니다. 계산 중에는 선택 변경을 막습니다.

GPU 실행 중 장치 오류·메모리 부족·비정상 수치가 발생하면 해당 입력의
정칙화 탐색 전체를 CPU로 다시 수행합니다. 취소 요청은 CPU로 재시작하지
않고 작업을 끝냅니다. 실제 GPU 사용 또는 복귀가 있는 내보내기에는
`compute-provenance.json`이 함께 기록됩니다.

## 검증 범위

CPU 실행은 스레드 수와 무관한 기존 결정성을 유지합니다. GPU 실행은
부동소수점 순서가 다르므로 같은 바이트 대신 수치·음향 허용오차로 검증합니다.
선택한 GPU와 실제 가속 단계가 결과에 기록됩니다.

설계 근거와 격리 규칙은 [ADR 0008](adr/0008-native-gpu-acceleration.md),
구현 검증은 [GPU 성능 기록](perf/gpu.md), 네이티브 호출의 의무는
[unsafe 경계](UNSAFE_BOUNDARIES.md)를 참조합니다.
