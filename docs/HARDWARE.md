# 하드웨어 프로브 기록

이 문서는 실제 오디오 장치에서 자체 측정 어댑터를 확인한 기록을 남기는 곳입니다. CI에는 오디오 장치가 없으므로 유지보수자 기계에서 프로브를 실행하고, 출력된 Markdown 보고서를 아래 기록 절에 붙입니다.

## 실행 방법

1. 출력 장치와 측정 마이크를 연결합니다. 루프백 레이턴시도 확인하려면 첫 출력 채널을 별도 입력 채널에 연결합니다.
2. 저장소 루트에서 장치와 지원 형식만 확인합니다.

   ```sh
   cargo run -p secs-sys --example hardware_probe --locked
   ```

3. 표에 나온 장치 이름이나 식별자의 고유한 일부를 골라 한 역할을 측정합니다. 채널 번호는 1부터 시작합니다.

   ```sh
   cargo run -p secs-sys --example hardware_probe --locked -- \
     --output "출력 장치 이름" --input "입력 장치 이름" \
     --channel 1 --input-channel 1
   ```

4. 유선 루프백이 있으면 `--loopback`에 그 입력 채널을 지정합니다. 공유 모드를 고정해 거부 동작을 확인하려면 `--share exclusive` 또는 `--share shared`를 씁니다. 기본 `auto`는 exclusive를 먼저 열고 형식 거부일 때만 shared 자동 변환으로 폴백합니다.

   ```sh
   cargo run -p secs-sys --example hardware_probe --locked -- \
     --output "출력 장치 이름" --input "입력 장치 이름" \
     --channel 1 --input-channel 1 --loopback 2 --share auto
   ```

5. 표준 출력 전체를 이 문서의 새 기록 절에 붙입니다. 장치, 형식, 실제로 열린 모드, 레이턴시, SNR, 드롭아웃을 함께 남깁니다. 불연속이나 출력 언더런이 있으면 측정을 통과한 것으로 기록하지 않습니다.

## 진단 변수 (Windows WASAPI)

Windows의 WASAPI 어댑터(`crates/secs-sys/src/wasapi.rs`)는 진단용 환경 변수 두 개를 읽습니다. PA05 실측에서 공유 모드 재생의 언더런을 추적하려고 둔 것이라 평소 측정에는 설정하지 않습니다. macOS와 Linux의 cpal 어댑터는 이 변수를 읽지 않습니다. 세션을 열 때마다 프로세스 환경에서 읽으므로, 앱이나 프로브를 실행하기 전에 같은 셸에서 설정합니다.

### `SECS_PA05_SHARED_PERIODS`

- 용도: 공유 모드 스트림에 요청할 버퍼 길이를 장치 주기의 몇 배로 할지 정합니다. 공유 모드로 여는 재생·녹음 세션에 쓰이고, 공유 모드 탐색에서 IsFormatSupported가 거절해 시험 초기화를 할 때도 쓰입니다. exclusive 모드에는 영향이 없습니다.
- 허용값: `2`, `3`, `4`. 앞뒤 공백 없이 숫자만 씁니다.
- 기본값: 설정하지 않으면 `4`입니다. PA05의 10회 짝 시험에서 2주기와 3주기가 언더런을 냈기 때문입니다.
- 다른 값(빈 값, `5`, 숫자가 아닌 값)이면 세션 열기와 탐색이 `shared periods must be 2, 3 or 4` 오류로 실패합니다.

### `SECS_PA05_TRACE_PREFIX`

- 용도: 재생·녹음 세션마다 CSV trace를 남깁니다. 열기 단계별 소요 시간, 캡처 패킷, 렌더 쓰기와 대기, 해제가 한 행씩 기록되고, 첫 행은 열 이름(`elapsed_us,category,event,...,detail`)입니다. 측정 동작은 바뀌지 않지만 기록이 세션 동안 메모리에 쌓이므로 진단할 때만 씁니다.
- 허용값: 절대 경로 접두사입니다. 상대 경로면 세션 열기가 `SECS_PA05_TRACE_PREFIX must be an absolute path` 오류로 실패합니다. 접두사가 가리키는 폴더는 미리 있어야 합니다.
- 기본값: 설정하지 않으면 trace를 남기지 않습니다.
- 파일 이름: `<접두사>-<render|capture>-<프로세스 id>-<순번>.csv`. 순번은 한 프로세스 안에서 trace를 남긴 세션마다 0부터 하나씩 늘어납니다. 접두사가 `C:\traces\pa05`이면 `C:\traces\pa05-render-4120-0.csv`, `C:\traces\pa05-capture-4120-1.csv`처럼 생깁니다.
- 파일은 세션을 닫을 때 씁니다. 만들거나 쓰지 못해도 측정 결과는 그대로이고, 세션마다 한 번 stderr에 `backend error: create PA05 trace <경로>: <원인>`처럼 남깁니다(`create` 자리는 `write`, `flush`일 수 있습니다).

### 설정과 해제

PowerShell에서는 이렇게 설정하고 해제합니다.

```powershell
$env:SECS_PA05_SHARED_PERIODS = "3"
$env:SECS_PA05_TRACE_PREFIX = "C:\traces\pa05"
cargo run -p secs-sys --example hardware_probe --locked -- `
  --output "출력 장치 이름" --input "입력 장치 이름" `
  --channel 1 --input-channel 1 --share shared
Remove-Item Env:SECS_PA05_SHARED_PERIODS, Env:SECS_PA05_TRACE_PREFIX
```

명령 프롬프트(cmd)에서는 `set SECS_PA05_TRACE_PREFIX=C:\traces\pa05`로 설정하고, `set SECS_PA05_TRACE_PREFIX=`처럼 값을 비워 해제합니다. 해제하면 공유 모드 버퍼는 기본 4주기로 돌아가고 trace는 남지 않습니다.

## 기록

아직 실제 장치에서 실행한 기록이 없습니다. `sys.hardware_probe`와 `measure.native`는 유지보수자 실측이 끝날 때까지 부분 구현으로 둡니다.
