# 0018. CamillaDSP와 Equalizer APO에 보정을 적용하는 단계

상태: 채택, 2026-10-29

## 배경

지금 SECS 2R은 CamillaDSP 설정과 Equalizer APO 설정을 파일로 내보내기만 합니다. 적용은 사용자가 합니다. CamillaDSP는 설정 파일을 직접 고르고 다시 읽혀야 하고, Equalizer APO는 FIR WAV와 설정 파일을 설정 폴더에 옮긴 뒤 `config.txt`에 한 줄을 더해야 합니다.

테스터는 Dirac Live 자리를 SECS 2R과 CamillaDSP로 바꾸려 하면서, SECS 2R이 실행 중인 CamillaDSP에 설정을 바로 올리거나 CamillaDSP를 띄워 관리하는지 물었습니다(B-1). 이슈 #108은 Equalizer APO XT에도 같은 단계를 두자고 합니다. 설치를 감지하면 설정 폴더에 필터를 보내고 `config.txt`에 모듈을 더하는 단계, 그리고 서비스가 그 상태를 지키는 단계입니다.

유지보수자는 단계를 설정으로 고르게 하고, 프로세스 관리까지 허용하되 별도의 서비스와 Tauri 셸로 만들라고 정했습니다. 자식 프로세스 관리에는 유지보수자의 [win-custody](https://github.com/115dkk/Win-Custody.rs)를 쓰고, 문제가 있으면 그 크레이트를 고쳐도 됩니다(2026-10-27).

## 결정

### 단계

대상(CamillaDSP, Equalizer APO)마다 적용 단계를 하나 고릅니다. 기본은 '파일만'입니다.

| 단계 | CamillaDSP | Equalizer APO |
|---|---|---|
| 파일만 | 지금처럼 내보내기 폴더에 씁니다. | 지금처럼 내보내기 폴더에 씁니다. |
| 적용 | 사용자가 띄운 CamillaDSP의 웹소켓에 접속해 설정을 올리고 상태를 읽습니다. | 설치를 감지해 설정 폴더에 필터를 쓰고 `config.txt`에 한 줄을 더합니다. |
| 관리 | 서비스가 CamillaDSP를 사용자 계정으로 띄우고, 멈추면 다시 띄우고, 고른 설정을 유지합니다. | 서비스가 `config.txt`를 지켜보다가 그 한 줄이 빠지면 다시 넣습니다. |

'적용'은 앱이 켜져 있을 때 사용자가 누를 때만 일합니다. '관리'는 앱이 꺼져 있어도 서비스가 일합니다. '관리'는 Windows에서만 고를 수 있고, CamillaDSP '적용'은 웹소켓만 쓰므로 모든 운영체제에서 됩니다.

### CamillaDSP 적용

- 주소는 설정에 두고 기본은 `127.0.0.1:1234`입니다. 루프백이 아닌 주소를 고르면 화면이 그 뜻을 한 줄로 알립니다.
- 올리는 순서는 내보내기(설정 폴더에 yml 쓰기), `ValidateConfig`(내용), `SetConfigFilePath`, `Reload`입니다. `Reload`가 실패하면 앞서 `GetConfigFilePath`로 읽어 둔 경로로 되돌리고 실패를 보고합니다.
- 상태는 `GetVersion`, `GetState`, `GetStopReason`, `GetConfigTitle`, `GetConfigFilePath`, `GetCaptureRate`, `GetClippedSamples`, `GetProcessingLoad`, `GetSignalLevels`로 읽습니다. 올라간 설정이 이 프로젝트의 어느 설계인지는 설정의 title과 `secs-design.json`(B-2)으로 판단해 '이 설계', '다른 설계', '알 수 없음'으로 보여 줍니다.
- 지원하는 CamillaDSP는 3.0 이상이고, CI의 재생 검사는 4.1.3입니다.

### Equalizer APO 적용

- 설치는 레지스트리 `HKLM\SOFTWARE\EqualizerAPO`의 `ConfigPath`와 `InstallPath`로 감지합니다. 공식판과 XT가 같은 키를 쓰는지, XT를 구별할 값이 무엇인지는 구현할 때 XT 소스와 설치본으로 확인해 이 ADR에 적습니다.
- 감지하면 `ConfigPath\SECS2R\`에 FIR WAV와 `SECS.txt`를 쓰고, `config.txt` 끝에 다음 두 줄을 더합니다. 표식 주석이 있는 줄만 SECS 2R의 것으로 봅니다. 표식은 Equalizer APO가 어느 판이든, 파일 인코딩이 무엇이든 그대로 다룰 수 있게 ASCII로 씁니다.

  ```text
  # SECS2R: managed by SECS 2R (this line and the next)
  Include: SECS2R\SECS.txt
  ```

- `config.txt`의 다른 줄은 바꾸지 않습니다. 처음 바꿀 때 원본을 `config.txt.secs2r-backup`으로 남기고, '되돌리기'는 표식 두 줄만 지웁니다. 같은 설계를 다시 적용하면 파일을 바꾸지 않습니다.
- 쓰기는 임시 파일에 쓴 뒤 이름을 바꾸는 방식으로 하여, Equalizer APO가 다 쓰지 않은 파일을 읽는 일이 없게 합니다. Equalizer APO는 설정 폴더가 바뀌면 다시 읽으므로 재시작은 하지 않습니다.
- 설정 폴더에 쓸 권한이 없으면 실패를 그대로 알리고, 권한을 올려 다시 시도하지 않습니다.

### 관리 서비스와 셸

- **`secs2r-host`(서비스).** `apps/secs2r-host`에 Windows 서비스 실행 파일을 둡니다. LocalSystem으로 돌고, CamillaDSP는 win-custody의 `PrimaryToken::for_session_user`로 로그온한 사용자 계정과 세션에서, `kill_on_close` 작업 개체 안에서 띄웁니다. CamillaDSP가 멈추면 1, 2, 4, 8초 간격으로 다시 띄우고, 5분 안에 다섯 번 멈추면 멈춘 채로 두고 이유를 기록합니다. Equalizer APO는 `config.txt`를 지켜보다가 표식 두 줄이 빠지면 다시 넣고 그 일을 기록합니다.
- **`secs2r-host-shell`(Tauri 셸).** `apps/secs2r-host-shell`에 작은 Tauri 앱을 둡니다. 알림 영역 아이콘과 창 하나로 서비스 상태, 지금 유지하는 설계, CamillaDSP 상태, 최근 기록을 보여 주고, 서비스 설치와 제거, 시작과 정지를 맡습니다. 설치와 제거만 UAC로 권한을 올립니다.
- **통신.** 본 앱과 셸은 이름 있는 파이프로 서비스와 메시지를 주고받습니다. 파이프 서버는 win-custody의 `PipeServer`로 만들고, 보안 설명자는 SYSTEM과 로그온한 사용자만 허용하며, 상대 프로세스 ID를 확인합니다. 메시지는 길이를 앞에 붙인 JSON이고 타입은 `secs-types`에 둡니다. 서비스는 파이프로 받은 경로만 쓰며, CamillaDSP 실행 파일 경로는 사용자가 셸이나 본 앱에서 고른 것만 씁니다.
- **배포.** 서비스와 셸은 Windows 설치본과 포터블 zip에 함께 싣습니다. 설치만으로는 서비스를 등록하지 않고, 사용자가 '관리'를 고르거나 셸에서 설치할 때 등록합니다. 앱을 제거하면 서비스도 제거합니다.

### 크레이트와 의존성

- **`secs-apply`(새 크레이트).** 운영체제 의존성 없이 `#![forbid(unsafe_code)]`로 둡니다. CamillaDSP 웹소켓 명령과 응답 타입, `tungstenite` 위의 동기 클라이언트, `config.txt` 편집(표식 줄 더하기, 지우기, 확인)의 순수 함수, 재시작 간격 같은 관리 규칙을 담습니다. crates.io에는 올리지 않습니다.
- **`secs-sys`.** 레지스트리 읽기, 파일 감시, 서비스 등록과 제어, 파이프와 자식 프로세스(win-custody)를 더합니다. `windows-registry`, `windows-service`, `notify`, `win-custody`는 `secs-sys`만 가집니다. unsafe 예산은 3 그대로입니다. 이 크레이트들의 unsafe는 그 크레이트 안에 있습니다.
- `secs-service`는 '적용'을 맡는 IPC 메서드를 더하고, 운영체제 기능은 앱이 `secs-sys`로 만든 구현을 트레이트로 넘겨 씁니다(측정 장치와 같은 방식).
- 두 앱(`secs2r-host`, `secs2r-host-shell`)은 `apps/` 아래에 있으므로 `secs-sys`에 의존할 수 있습니다.

## 결과

- 기본이 '파일만'이므로 지금 사용자의 동작은 바뀌지 않습니다.
- '적용'은 앱이 켜져 있을 때만 일합니다. 앱을 닫은 뒤 다른 프로그램(Peace 같은 편집기)이 `config.txt`를 바꾸면 그 한 줄이 사라질 수 있고, '관리'를 고르면 서비스가 그 줄을 다시 넣습니다.
- 서비스는 LocalSystem으로 돌기 때문에 파이프의 접근 제한과 경로 규칙이 보안의 경계입니다. 서비스는 사용자가 고른 실행 파일을 사용자 권한으로만 띄우고, 자기 권한으로는 띄우지 않습니다.
- Windows 설치본이 커지고, CI는 서비스와 셸을 빌드하고 서비스의 순수 규칙을 테스트합니다. 실제 서비스 등록과 장치 재생은 CI에서 돌리지 않습니다.
- 출력 레이트 '자동'(ADR 0017)은 '적용'이나 '관리'에서 CamillaDSP 설정의 `samplerate`를 읽어 정할 수 있게 됩니다.
