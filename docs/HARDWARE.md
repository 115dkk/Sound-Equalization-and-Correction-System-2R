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

## 기록

아직 실제 장치에서 실행한 기록이 없습니다. `sys.hardware_probe`와 `measure.native`는 유지보수자 실측이 끝날 때까지 부분 구현으로 둡니다.
