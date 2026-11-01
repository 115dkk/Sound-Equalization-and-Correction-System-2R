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
- 올리는 순서는 내보내기(새 세대 폴더에 yml과 FIR 저장, 아래 '세대'), `ValidateConfig`(내용), `SetConfigFilePath`, `Reload`입니다. `Reload`가 실패하면 앞서 `GetConfigFilePath`로 읽어 둔 경로로 되돌리고 실패를 보고합니다.
- 상태는 `GetVersion`, `GetState`, `GetStopReason`, `GetConfigTitle`, `GetConfigFilePath`, `GetCaptureRate`, `GetClippedSamples`, `GetProcessingLoad`, `GetSignalLevels`로 읽습니다. 올라간 설정이 이 프로젝트의 어느 설계인지는 설정의 title과 `secs-design.json`(B-2)으로 판단해 '이 설계', '다른 설계', '알 수 없음'으로 보여 줍니다.
- 지원하는 CamillaDSP는 3.0 이상이고, CI의 재생 검사는 4.1.3입니다.

### Equalizer APO 적용

- 설치는 레지스트리 `HKLM\SOFTWARE\EqualizerAPO`의 `ConfigPath`와 `InstallPath`로 감지합니다. XT도 같은 키와 값 이름을 씁니다. XT는 Velopack 패키지 ID를 `EqualizerAPO-XT-<아키텍처>`로 정하므로 `InstallPath`의 디렉터리 이름에 `EqualizerAPO-XT`가 들어가고, 공식판은 `EqualizerAPO` 디렉터리를 씁니다. 이 이름으로 XT와 공식판을 구별하고 어느 쪽도 아니면 판을 알 수 없음으로 둡니다. XT는 `Include`와 `Convolution`의 상대 경로를 각 줄이 들어 있는 설정 파일의 디렉터리에서 풉니다(`IncludeFilterFactory.cpp`, `ConfigFileReference.cpp`). 따라서 `SECS2R\SECS.txt`가 다시 포함하는 필터 본문과 그 본문이 가리키는 FIR은 묶음 안의 상대 경로를 그대로 씁니다.
- 감지하면 `ConfigPath\SECS2R\` 아래의 새 세대 폴더에 필터 본문(`config.txt`)과 FIR WAV를 저장하고, `SECS2R\SECS.txt`를 그 세대를 포함하는 한 줄(`Include: <세대>\config.txt`)로 바꿉니다(아래 '세대'). `config.txt` 끝에는 다음 두 줄을 추가합니다. 표식 주석이 있는 줄만 SECS 2R의 것으로 봅니다. 표식은 Equalizer APO가 어느 판이든, 파일 인코딩이 무엇이든 그대로 다룰 수 있게 ASCII로 씁니다.

  ```text
  # SECS2R: managed by SECS 2R (this line and the next)
  Include: SECS2R\SECS.txt
  ```

- `config.txt`의 다른 줄은 바꾸지 않습니다. 처음 바꿀 때 원본을 `config.txt.secs2r-backup`으로 남기고, '되돌리기'는 표식 두 줄만 지웁니다.
- 세대 폴더는 다 만든 뒤에 이름을 바꿔 들여놓고, `SECS.txt`도 임시 파일을 만든 뒤 이름을 바꿔 교체합니다. 그래서 Equalizer APO가 반쯤 만들어진 파일을 읽는 일이 없습니다. Equalizer APO는 설정 폴더가 바뀌면 다시 읽으므로 재시작은 하지 않습니다.
- 설정 폴더에 쓸 권한이 없으면 실패를 그대로 알리고, 권한을 올려 다시 시도하지 않습니다.

### 세대 (2026-11-02에 추가함, 감사 #155 TD-22, TD-105, TD-106, TD-108, TD-117)

같은 경로의 파일을 덮어쓰면 소비자가 읽는 도중에 내용이 바뀌거나, CamillaDSP처럼 경로가 같으면 다시 읽지 않는 소비자가 옛 FIR을 계속 씁니다(0.45.1). 그래서 두 대상 모두 적용할 때마다 세대 폴더를 새로 만들고, 소비자가 보는 진입점 하나만 바꿉니다.

- **세대 폴더.** 이름은 `<설계 ID 앞 16자리>-<번호>`이고, 번호는 그 자리에 없는 가장 작은 수입니다. CamillaDSP는 `<앱 데이터>\applied\camilladsp\` 아래, Equalizer APO는 `ConfigPath\SECS2R\` 아래에 둡니다. 같은 설계를 다시 적용해도 새 세대를 만듭니다. 묶음은 대상 폴더 밖의 임시 폴더에서 다 만든 뒤 이름을 바꿔 세대 폴더로 들여놓고, 적용 결과의 `folder`는 그 세대 폴더입니다. 세대 폴더의 `secs-design.json`에는 들여놓은 파일마다 크기와 SHA-256이 들어 있습니다.
- **진입점.** CamillaDSP는 `SetConfigFilePath`로 새 세대의 yml을 고르고 `Reload`합니다. Equalizer APO는 `SECS2R\SECS.txt` 한 파일을 `Include: <세대>\config.txt`(CRLF)로 원자 교체합니다. Equalizer APO는 `Include`의 상대 경로를 그 줄이 있는 파일의 폴더에서 풀기 때문에, 세대 안의 FIR 경로는 세대 폴더 기준의 상대 경로 그대로 둡니다.
- **보존.** 적용에 성공하면 지금 세대와 바로 앞 세대를 남기고 나머지를 지웁니다. 실패했거나 결과가 불확실한 전환(되돌리기 실패, `config.txt` 복구 실패)에서 소비자가 참조했을 수 있는 세대는 그 대상의 세대 폴더 옆 `secs-retained-generations.txt`(UTF-8, 한 줄에 세대 이름 하나)에 기록하고, 다음 성공한 적용은 이 세대도 남긴 뒤 기록을 지웁니다. 기록을 읽거나 해석할 수 없으면 그 적용에서는 아무것도 지우지 않습니다. CamillaDSP가 지금 쓰는 세대는 서비스가 남긴 표식이 아니라 그 적용에서 CamillaDSP가 알려 준 이전 설정 경로로 정하고, 알 수 없으면 지우지 않습니다. 지우기에 실패해도 적용은 성공으로 끝납니다. 링크나 정션은 대상을 따라가지 않고 링크 자체만 지웁니다.
- **한계.** 소비자가 두 번 넘게 이어진 성공한 적용 동안 옛 세대의 파일을 계속 읽고 있으면, 그 파일은 지워집니다. 실패한 전환 바로 뒤에 보존 기록마저 저장하지 못하면, 다음 성공한 적용이 남겨야 할 세대를 지우는 경우도 있습니다. 이 실패는 세대 이름과 함께 오류로 기록합니다. 두 경우 모두 받아들인 한계입니다(유지보수자 위임에 따른 Claude의 판정, 2026-11-02).
- **이전 배치.** 0.46.0까지의 평면 배치(`SECS2R\SECS.txt`에 필터 본문을 둔 것)의 파일은 첫 세대 적용이 성공한 뒤 세대가 아닌 파일로 보고 지웁니다.

### 관리 서비스와 셸

- **`secs2r-host`(서비스).** `apps/secs2r-host`에 Windows 서비스 실행 파일을 둡니다. LocalSystem으로 돌고, CamillaDSP는 win-custody의 `PrimaryToken::for_session_user`로 로그온한 사용자 계정과 세션에서, `kill_on_close` 작업 개체 안에서 띄웁니다. CamillaDSP가 멈추면 1, 2, 4, 8초 간격으로 다시 띄우고, 5분 안에 다섯 번 멈추면 멈춘 채로 두고 이유를 기록합니다. Equalizer APO는 `config.txt`를 지켜보다가 표식 두 줄이 빠지면 다시 넣고 그 일을 기록합니다.
- **`secs2r-host-shell`(Tauri 셸).** `apps/secs2r-host-shell`에 작은 Tauri 앱을 둡니다. 알림 영역 아이콘과 창 하나로 서비스 상태, 지금 유지하는 설계, CamillaDSP 상태, 최근 기록을 보여 주고, 서비스 설치와 제거, 시작과 정지를 맡습니다. 설치와 제거만 UAC로 권한을 올립니다.
- **통신.** 본 앱과 셸은 이름 있는 파이프로 서비스와 메시지를 주고받습니다. 파이프 서버는 win-custody의 `PipeServer`로 만들고, 보안 설명자는 SYSTEM과 로그온한 사용자만 허용하며, 상대 프로세스 ID를 확인합니다. 메시지는 길이를 앞에 붙인 JSON이고 타입은 `secs-types`에 둡니다. 서비스는 파이프로 받은 경로만 쓰며, CamillaDSP 실행 파일 경로는 사용자가 셸이나 본 앱에서 고른 것만 씁니다.
- **배포.** 서비스와 셸은 Windows 설치본과 포터블 zip에 함께 싣습니다. 설치만으로는 서비스를 등록하지 않고, 사용자가 '관리'를 고르거나 셸에서 설치할 때 등록합니다. 앱을 제거하면 서비스도 제거합니다.

### 관리 단계의 세부 (2026-10-29에 더함)

- **등록과 권한.** Windows 설치본은 현재 사용자 설치(`installMode: currentUser`)라 권한을 올리지 않으므로, 설치본이 서비스를 등록할 수 없습니다. 그래서 `secs2r-host.exe install`과 `uninstall`이 서비스를 등록하고 지우며, 셸과 본 앱은 이 명령을 UAC로 권한을 올려 실행합니다. 서비스는 자동 시작으로 등록하고, 관리할 대상이 없으면 아무 일도 하지 않습니다. 켜고 끄는 일은 파이프의 요청으로 하므로 등록 뒤에는 권한이 필요 없습니다. 사용자별 설치 폴더는 사용자가 바꿀 수 있으므로, 서비스는 실행 파일을 Program Files에 복사하고 그 복사본으로 실행합니다. 서비스를 다시 설치하면 실행 파일 복사본과 서비스 설정을 갱신합니다.
- **프로세스를 띄우는 곳.** 정책 게이트(ADR 0002)는 외부 프로세스 실행을 `secs-sys`의 REW 모듈에만 허용합니다. 여기에 `secs-sys`의 `host` 모듈을 더합니다. 이 모듈만 win-custody의 `Launch`로 CamillaDSP를 띄우고, `runas` 크레이트로 `secs2r-host.exe install`을 권한을 올려 실행합니다. 두 크레이트의 unsafe는 그 크레이트 안에 있고, `secs-sys`의 unsafe 예산은 3 그대로입니다.
- **CamillaDSP 실행.** 실행 파일 경로는 앱 설정(`apply.camilla.exe_path`)에 사용자가 고른 것을 쓰고, 서비스는 `<exe> -p <포트> -a 127.0.0.1 <설정 경로>`로 로그온한 사용자의 세션에서 띄웁니다. 사용자가 로그오프하면 멈추고, 다시 로그온하면 다시 띄웁니다. 포트는 앱 설정의 주소에서 가져오고, 서비스가 띄운 CamillaDSP는 루프백 주소로만 접속을 받습니다. 관리하는 동안에는 서비스가 이 포트를 맡으며, 다른 CamillaDSP가 응답하면 `Exit` 명령으로 끝낸 뒤 서비스의 프로세스를 띄웁니다. CamillaDSP 4.1.3은 입력 스레드가 막히면(예를 들어 조용한 루프백) `Exit`을 받고도 처리만 멈추고 포트를 놓지 않으므로, 5초 안에 포트가 비지 않으면 그 포트를 가진 프로세스의 실행 파일이 `camilladsp.exe`일 때만 서비스가 끝내고 PID를 기록합니다. 다른 프로그램이 포트를 쓰면 끝내지 않고 `port_busy`를 남깁니다(유지보수자 결정, 2026-10-30). 실행 중인 CamillaDSP가 없어도 관리 시작 요청을 받아 서비스를 통해 새로 띄웁니다.
- **상태 저장.** 서비스는 관리 대상(CamillaDSP 실행 파일과 설정 경로, Equalizer APO 설정 폴더)을 `%ProgramData%\SECS2R\host.json`에 저장해, 컴퓨터를 다시 켜도 이어서 관리합니다. 최근 기록은 같은 폴더의 `host.log`에 크기 한도를 두고 씁니다.
- **파이프.** 이름은 `\\.\pipe\secs2r-host`이고 보안 설명자는 `D:P(A;;GA;;;SY)(A;;GRGW;;;IU)`(SYSTEM과 대화형 사용자)입니다. 요청은 상태 읽기, 관리 대상 정하기, 관리 끄기, 최근 기록 읽기입니다.
- **권한 경계.** 관리 대상을 정하거나 끄는 요청은 현재 콘솔 세션의 프로세스만 보낼 수 있으며, 서비스는 그 세션 사용자의 SID를 상태에 함께 저장하고, 콘솔 사용자가 같은 사람인지는 로그온할 때마다 달라지는 세션 번호가 아니라 SID로 확인합니다. 다른 사용자가 콘솔을 쓰면 CamillaDSP를 멈추고 `other_user`를 기록합니다. Equalizer APO 폴더는 HKLM의 `ConfigPath`와 일치해야 하며 `config.txt` 복구는 같은 사용자의 세션에서 실행한 보조 프로세스가 맡습니다. `%ProgramData%\SECS2R`은 SYSTEM이나 Administrators가 소유한 실제 디렉터리만 받아들이고, SYSTEM과 Administrators에는 모든 권한을, Users에는 읽기 권한만 주는 상속 차단 DACL을 적용합니다.
- **관리 전환 (2026-11-02에 추가함, 감사 #155 TD-111, TD-113, TD-114).** 서비스가 상태와 요청과 세션 이벤트를 받아 할 일을 정하는 규칙은 `apps/secs2r-host/src/transitions.rs` 한 곳에 있고, 실제 실행은 그 결과를 따르기만 합니다. 관리 대상을 정하는 사용자의 SID가 저장된 SID와 다르면 이전 사용자의 대상은 모두 버리고 새 요청에 든 대상만 남깁니다. 같은 사용자가 대상 하나만 바꾸면 다른 대상은 그대로 둡니다. `config.txt` 복구가 실패하면 그 실패는 Equalizer APO 대상의 오류로만 남고, CamillaDSP와 드라이버는 멈추지 않습니다. 복구는 1초, 2초 뒤에 다시 시도해 모두 세 번까지 합니다. 로그오프 이벤트는 그 세션에서 띄운 자식만 멈추므로, 다른 사용자가 로그오프해도 관리하는 세션의 CamillaDSP는 그대로 돕니다. 서비스가 메모리에 두는 최근 기록은 1000줄까지이고, 기록 읽기 응답은 960 KiB를 넘지 않습니다.

### 크레이트와 의존성

- **`secs-apply`(새 크레이트).** 운영체제 의존성 없이 `#![forbid(unsafe_code)]`로 둡니다. CamillaDSP 웹소켓 명령과 응답 타입, `tungstenite` 위의 동기 클라이언트, `config.txt` 편집(표식 줄 더하기, 지우기, 확인)의 순수 함수, 재시작 간격 같은 관리 규칙을 담습니다. crates.io에는 올리지 않습니다.
- **`secs-sys`.** 레지스트리 읽기, 파일 감시, 서비스 등록과 제어, 파이프와 자식 프로세스(win-custody)를 더합니다. `windows-registry`, `windows-service`, `notify`, `win-custody`는 `secs-sys`만 가집니다. unsafe 예산은 3 그대로입니다. 이 크레이트들의 unsafe는 그 크레이트 안에 있습니다.
- `secs-service`는 '적용'을 맡는 IPC 메서드를 더하고, 운영체제 기능은 앱이 `secs-sys`로 만든 구현을 트레이트로 넘겨 씁니다(측정 장치와 같은 방식).
- 두 앱(`secs2r-host`, `secs2r-host-shell`)은 `apps/` 아래에 있으므로 `secs-sys`에 의존할 수 있습니다.

## 결과

- 기본이 '파일만'이므로 지금 사용자의 동작은 바뀌지 않습니다.
- '적용'은 앱이 켜져 있을 때만 일합니다. 앱을 닫은 뒤 다른 프로그램(Peace 같은 편집기)이 `config.txt`를 바꾸면 그 한 줄이 사라질 수 있고, '관리'를 고르면 서비스가 그 줄을 다시 넣습니다.
- 서비스는 LocalSystem으로 돌기 때문에 파이프의 접근 제한과 경로 규칙이 보안의 경계입니다. 서비스는 사용자가 고른 실행 파일을 사용자 권한으로만 띄우고, 자기 권한으로는 띄우지 않습니다.
- Windows 설치본이 커지고, CI는 서비스와 셸을 빌드하고 서비스의 순수 규칙을 테스트합니다. CI의 `manage-e2e`는 Windows 러너에 앱을 설치하고 GUI에서 실제 서비스를 등록한 뒤, 판을 고정한 CamillaDSP와 Equalizer APO XT로 인수, 재시작, 설정 복구, 관리 끄기와 서비스 제거를 검사합니다.
- 출력 레이트 '자동'(ADR 0017)은 '적용'이나 '관리'에서 CamillaDSP 설정의 `samplerate`를 읽어 정할 수 있게 됩니다.
