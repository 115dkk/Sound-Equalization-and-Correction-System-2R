# M5 도달표: 업스트림 기능이 새 화면에서 닿는 자리

[`M5-inventory.md`](M5-inventory.md)의 항목마다 새 화면에서 그것에 닿는 화면 요소의 id(`data-ui` 또는 같은 값의 `id`)를 적습니다. M5는 이 표에 빈칸이 없어야 끝납니다([`M5.md`](M5.md) 6절). 화면 테스트가 표의 id들이 실제로 그려지는지 확인합니다.

- 한 줄은 목록의 한 항목입니다. 항목 이름은 목록의 낱말을 짧게 옮기고, 자리 표시는 `M5-inventory.md`의 소절을 따릅니다.
- **id**는 화면 요소의 안정된 id입니다. 여럿이면 쉼표로 잇습니다. 역할이나 위치처럼 바뀌는 부분은 `<role>`, `<n>`으로 적습니다.
- **편차**는 업스트림과 다르게 닿는 자리입니다([`DEVIATIONS.md`](DEVIATIONS.md)의 번호). 없앤 기능(U4)은 id 칸에 `—`를 적고 편차 번호를 답니다.
- **작업서**는 그 줄을 채운 작업서입니다. 연결(P33) 뒤에 id가 바뀌면 그 작업서가 고칩니다.

### 출력 레이트의 갤러리 견본(P116, P117)

| 견본 id | 확인할 상태 |
|---|---|
| `screen.system.dsp.rate-48k` | CamillaDSP의 출력 레이트 48 kHz, 변경 가능 |
| `screen.system.dsp.rate-auto` | 자동을 골라도 CamillaDSP 프로파일 유지 |
| `screen.system.dsp.rate-minidsp` | miniDSP의 출력 레이트 96 kHz 고정 |
| `screen.export.rate.96k` | 필터 형식별 출력 레이트 96 kHz |
| `screen.export.rate.extra` | 추가 레이트 목록 펼침, 44.1 kHz와 192 kHz 선택 |
| `screen.export.rate.fallback` | 지원하지 않는 장치 레이트 대신 48 kHz로 저장, 시스템으로 이동 |
| `screen.calibrate.mimo-budget-96k` | 96 kHz로 환산한 MIMO FIR의 예산 초과 |
| `screen.calibrate.mode-fir-over-96k` | 96 kHz로 환산한 채널 FIR의 한도 초과 |

화면 검증은 `screens/system/SystemScreen.test.tsx`, `screens/system/useSystemScreen.test.tsx`, `screens/system/outputRate.test.ts`, `screens/calibrate/CalibrateScreen.test.tsx`, `screens/calibrate/useCalibrateScreen.test.tsx`, `screens/export/ExportScreen.test.tsx`, `screens/export/useExportScreen.test.tsx`에서 합니다. 모의 서비스와 저장 순서는 `ipc/mock.test.ts`, `state/writers.test.ts`에서 확인합니다.

## 1. 장치·마이크 (Step 1) — P30

| 항목 | id | 편차 | 작업서 |
|---|---|---|---|
| 측정 방식: 자체 측정, REW, 모의 측정, mdat 파일만(새 화면, `M5.md` 2절) | `connect.method`, `connect.method.native`, `connect.method.rew`, `connect.method.mock`, `connect.method.mdat` | 없음 | P30 |
| REW 상태 알약(연결 안 됨 / API 연결됨 · 127.0.0.1:4735), 확인·실행 중 표시 | `connect.rew.status` | 없음 | P30 |
| 실행 파일 경로(찾지 못함, 알 수 없음) | `connect.rew.executable` | 없음 | P30 |
| 버전 | `connect.rew.version` | 없음 | P30 |
| Pro 알약: 확인됨 / 연결 후 확인 / Pro 아님 · 수동 측정 | `connect.rew.pro` | 없음 | P30 |
| blocking 모드 | `connect.rew.blocking` | 없음 | P30 |
| [REW API 모드로 실행] / [다시 확인] (`rew_launch`) | `connect.rew.launch` | 없음 | P30 |
| [설정 읽기]: REW의 샘플레이트, 스윕, 레벨을 안내 줄에 | `connect.rew.read_settings`, `connect.rew.settings` | 없음 | P30 |
| REW 상태별 안내: 꺼짐, API 없이 실행, 실행 파일 없음, 띄울 수 없음, 시간 초과, 실행 실패 | `connect.rew.hint` | 없음 | P30 |
| 입력 장치("이름 · N채널") | `connect.input_device` | 없음 | P30 |
| 입력 채널(REW 채널 이름) | `connect.input_channel` | 없음 | P30 |
| 마이크 보정 알약: 알 수 없음 / 불러온 파일 없음 / 파일 이름 | `connect.mic_cal` | 없음 | P30 |
| 출력 드라이버(시스템의 측정 출력 장치) | `connect.output_device` | 없음 | P30 |
| 샘플레이트(읽기 전용) | `connect.sample_rate` | 없음 | P30 |
| 스윕 길이 64k…4M "길이 · 초", 기본 256k(측정·가져오기의 측정 옵션에서만 정함, P58) | `measure.options.sweep` | 없음 | P30, P86 |
| REW가 떠 있는데 장치 목록이 비면 다시 읽기(2초마다, 15번) | `connect.input_device`, `connect.devices.retry` (다시 읽기는 `status_watch`의 `MeasureStatus`, P33) | 없음 | P30 |
| 장치 공유: 자동 / 독점 / 공유(자체 측정의 장치 카드에서 펼침) | `connect.devices.details`, `connect.share_mode` | 없음 | P30, P58 |
| 진행 조건: REW 실행과 Pro("REW 를 켜 주세요.") | `connect.rew.hint`, `connect.rew.launch` | 없음 | P30 |
| [장치 확인 건너뛰기] | `connect.method.mdat`, `connect.mdat`, `connect.next` | 없음 | P30 |

## 2. 구성 (Step 2) — P30

| 항목 | id | 편차 | 작업서 |
|---|---|---|---|
| 프리셋 2.0 / 2.1 / 2.2 / 5.1 / 7.1.4 / 9.1.6(SECS 2R은 4.0 / 4.1 / 4.2 / 7.1을 더함), 지금 프리셋 강조, 측정 지움 | `system.preset`, `system.preset.<name>`, `system.confirm` | U5, U13 | P30 |
| 빈 구성 / 사용자 정의 | `system.preset.empty`, `system.preset.custom` | U5 | P30 |
| 서브 증감기 0–8, 측정 지움 | `system.subs`, `system.confirm` | U5 | P30 |
| 바이앰프 시작 체크박스(업스트림 `BIAMP_UI=False`) | — | 숨김 유지(`M5.md` 5절) | P30 |
| 측정 출력 장치와 채널 수 알약(미확인 / N개 초과 / 채널 N개) | `system.device` | 없음 | P30 |
| 표 묶음: 프론트, 센터, 서브, 서라운드, 백, 와이드, 탑, 하이트, 기타 | `system.patch` | 없음 | P30 |
| 채널: 역할("코드 · 돌비 이름", 사용자 정의), 서브 역할이면 SUB 대역 | `system.patch.<role>.role` | 없음 | P30 |
| 크기: 소형 · XO 적용 / 대형 · 풀레인지, 서브는 알약 | `system.patch.<role>.size` | 없음 | P30 |
| 앰프 구성과 대역 칩(바이앰프와 함께 숨김) | `system.speaker` (읽기 전용 대역별 상세); 앰프 구성 — | 앰프 구성 숨김 유지(`M5.md` 5절) | P30, P58 |
| 측정 출력 1..N, "(범위 밖)" 항목, 중복 알약 | `system.patch.<role>.out` | 없음 | P30 |
| 극성 정상 / 반전 | `system.patch.<role>.pol` | 없음 | P30 |
| 빼기 | `system.patch.<role>.remove` | 없음 | P30 |
| 요약: 스피커 수, 출력과 가장 높은 출력, 중복 알약, 서브 알약 | `system.summary` | 없음 | P30 |
| [+ 스피커 추가] | `system.patch.add` | 없음 | P30 |
| [출력 번호 자동 정렬] | `system.patch.renumber` | 없음 | P30 |
| [패치 시트 CSV] (`secs_patch_sheet.csv`) | `system.patch.csv` | 없음 | P30 |
| 오류 상자: `validate()` 앞의 4개와 "외 N개" | `system.notices`, `system.notices.more` | 없음 | P30 |
| 방 배치도: 스크린, 소파, 역할 표식과 출력, 역할 미지정 줄, 눌러 고르기 | `system.room`, `system.room.speaker.<role>`, `system.room.unassigned.<n>` | 없음 | P30 |
| 고른 스피커: 이름, 역할, 종류, 대역마다 "BAND → 출력 → DSP 입력 → 앰프 [반전]" | `system.speaker` | 없음 | P30 |
| DSP 장치: CamillaDSP/BruteFIR, Tesira SERVER-IO, Q-SYS Core, miniDSP, 사용자 지정 | `system.dsp`, `system.dsp.device` | U2 | P30 |
| 채널당 FIR 탭 512…65536 ("선형 위상 최대 x ms") | `system.dsp.advanced`, `system.dsp.fir_taps` | U2 | P30 |
| 총 FIR 탭 예산 0–4,194,304, 1024 단계 | `system.dsp.fir_budget` | U2 | P30 |
| IIR 밴드, 올패스 밴드, BM 경로 지연 0–64 | `system.dsp.iir_bands`, `system.dsp.allpass_bands`, `system.dsp.bm_path_delays` | U2 | P30 |
| 출력 레이트 이외의 값을 직접 고치면 사용자 지정, "FIR 길이 ≈ x ms · 주파수 간격 ≈ y Hz" | `system.dsp.note` | U2 | P30, P117 |
| 내보낼 DSP의 출력 레이트: CamillaDSP와 사용자 지정은 자동 또는 여섯 고정값, 나머지는 장치 고정값 | `system.dsp.output_rate` | ADR 0017 | P116, P117 |
| 프리셋 값(탭 / 예산 / IIR / AP / BM) | `system.dsp.device` (`dsp_set { preset }`) | U2 | P30 |
| 진행 조건: 스피커 하나 이상, `validate()` 통과 | `frame.header.blocked`, `system.next` | 없음 | P30 |

## 3. 레벨 체크 (Step 3) — P30

| 항목 | id | 편차 | 작업서 |
|---|---|---|---|
| 채널 줄(`ordered_speakers` 순서): 재생/멈춤, 역할, 이름, 미터, 값; 저장된 값 미리 채움 | `connect.level`, `connect.level.row.<role>`, `connect.level.meter.<role>` | 없음 | P30 |
| 재생(핑크 노이즈, 서브 50 Hz 사인), 다른 재생 단추 잠금, 2.5초 뒤 멈춤 | `connect.level.play.<role>` | 없음 | P30 |
| 레벨 기록(`extra['levels']`, `extra['spl']`), 단계가 보이는 동안 모니터 | `connect.level.meter.<role>`, `connect.mic.state` | 없음 | P30 |
| 마스터 출력 −40…0 dBFS, 0.5 단계, 기본 −20; "재생 중 · 코드" 알약 | `connect.level.master`, `connect.level.playing` | 없음 | P30 |
| [전 채널 자동 순차 재생] | `connect.level.play_all`, `connect.level.stop` | 없음 | P30 |
| 마이크 입력 라벨(채널 · 장치) | `connect.mic.input` | 없음 | P30 |
| 내장 마이크 미터: 미터, dBFS, 피크, 상태, [피크 지우기] | `connect.mic`, `connect.mic.meter`, `connect.mic.peak`, `connect.mic.state`, `connect.mic.clear_peak` | 없음 | P30 |
| 떠 있는 마이크 미터(Ctrl+M; 단축키는 P33) | `frame.mic` (`MicMeterPanel`의 `floating`) | 없음 | P30 |
| 진행 조건: 레벨이 없는 채널("레벨을 아직 못 잰 채널") | `connect.status` | 없음 | P30 |
| [레벨 건너뛰기 (기본값 적용)] | `connect.level.skip` | 없음 | P30 |

## 4. 배치 (Step 4) — P30

| 항목 | id | 편차 | 작업서 |
|---|---|---|---|
| 배치 카드 1 / 9 / 13 / 17(작은 배치도, "위치 N곳"), 다른 카드를 누르면 측정 지움 | `system.layout`, `system.arrangement`, `system.arrangement.<n>`, `system.confirm` | U5 | P30 |
| 측정 배치도: 스크린, 1 m 원, 의자·소파, 머리, 30 cm 축척, 번호 표식(위치 1, ±20 cm, 완료·지금) | `system.arrangement.map`, `system.arrangement.map.position.<n>` | 없음 | P30 |
| 측정 시작 지연: 끔 / 5 / 10 / 15초(측정·가져오기의 측정 옵션에서만 정함, P58) | `measure.options.delay` | 없음 | P30, P86 |
| 위치당 스윕 | `system.sweeps` | 없음 | P30 |
| 예상 시간: 위치당과 전체(`position_seconds`) | `system.estimate` | 없음 | P30 |
| 타이밍 기준: 음향(기준 스피커) / 루프백(케이블) / 없음. REW로 측정할 때만, '고급' 안에 | `system.timing.advanced`, `system.timing` | U15 | P30 |
| 기준 출력: 스피커(`timing_ref_role`, 기본 R) 또는 채널(`timing_ref_output`) | `system.timing.role`, `system.timing.output` | U15 | P30 |
| 진행 조건: 스피커가 있음 | `frame.header.blocked`, `system.next` | 없음 | P30 |

## 5. 측정 (Step 5) — P31a

| 항목 | id | 편차 | 작업서 |
|---|---|---|---|
| 머리 "위치 k / N" 또는 모두 측정함, "이 위치 약 …" | `measure.where` | — | P31a |
| [그래프 숨기기] / [그래프 보기] (배치도가 커짐) | `measure.graph.toggle` | — | P31a |
| 고를 수 있는 배치도(클릭, 화살표 키), 측정 중에는 무시 | `measure.map`, `measure.map.position.<n>` | — | P31a |
| 그래프와 축 단추, 범례(채널 칩, 서브 굵게, "굵은 선: 위치 1"), 위치 칩(켜고 끄기, 커서 강조, 위치 1곳 이하면 없음) | `measure.plot`, `measure.plot.axes`, `measure.plot.legend.role.<role>`, `measure.plot.legend.position.<n>`, `measure.graph.empty` | — | P31a |
| 위치 목록 "NN 위치 설명", 완료·대기, 툴팁의 원본 파일, 누르면 고름 | `measure.positions`, `measure.positions.<n>` | — | P31a |
| 원본 메모 "불러옴: a.mdat (위치 1–9) · 측정함 (위치 10)" | `measure.sources` | — | P31a |
| 역할 진행 막대 | `measure.run`, `measure.run.progress` | U8: 트레이의 `jobs.job.measure`에도 보임 | P31a |
| [이 위치 측정] (잰 위치면 다시 측정, 모두 끝나면 끔): 카운트다운, 프로젝트 경로, 계획 확인, 측정 | `measure.position.start`, `measure.next`, `measure.path.note` | 모두 측정하면 끄는 대신 주 동작이 "보정으로"(`measure.next`). 경로가 없으면 `~/Documents/SECS/측정 …`을 몰래 만들지 않고 저장 대화상자를 먼저 엶(작업서 P31a, 편차 번호 없음) | P31a |
| [측정 취소]: 카운트다운은 바로, 스윕은 지금 스윕 뒤 멈춤 | `measure.cancel` | U8: 트레이의 `jobs.cancel.measure`도 같은 잡을 멈춤 | P31a |
| [마지막 위치 다시]: 마지막으로 끝낸 위치를 지움 | `measure.redo`, `measure.clear`, `measure.clear.confirm`, `measure.clear.undo` | U5: 확인 뒤 지우고 알림에서 되돌림 | P31a |
| 옵션: 시작 지연, 스윕 길이(1절과 4절의 같은 설정도 이곳에서만 정함, P58), 반복 1/2/4/8(REW에서 음향 기준이 아닐 때만) | `measure.options.delay`, `measure.options.sweep`, `measure.options.repeats` | U15 | P31a, P86 |
| [mdat 불러오기] | `measure.mdat.load` | — | P31a |
| [파일 관리] | `measure.files.open` | — | P31a |
| [데모 데이터] (튜토리얼만) | `measure.sample.load` | — | P31a |
| 바쁨 물방울과 경과 시간 | `measure.status`, `measure.run` | U8: 경과·예상 시간은 트레이의 `jobs.job.measure` | P31a |
| 로그 패널: 대기, 역할·위치 진행, 카운트다운, 낮은 S/N 경고, 오류, 새 프로젝트 경로, mdat 줄 | `measure.run`, `measure.warning` | U8: 줄마다의 기록은 트레이의 `jobs.history`, 화면에는 지금 진행과 이 위치의 마지막 경고만. U6: 경고 문턱은 알림이 실어 오는 20 dB(툴팁의 40 dB 없음) | P31a |
| MdatAssignDialog: 측정마다 제목, 내용(읽을 수 없음 / 저역만 · 서브로 보임 / 전대역), 채널(건너뜀 또는 "코드 · 이름"), 여러 위치면 위치; 스캔으로 미리 채움; 문제가 있으면 가져오기 끔 | `measure.mdat.dialog`, `measure.mdat.row.<n>.position`, `measure.mdat.row.<n>.role`, `measure.mdat.problem`, `measure.mdat.import` | — | P31a |
| MdatFilesDialog: 위치, 파일(또는 측정함 / 없음), 채널 있음/전체, [가져오기…], [지우기] | `measure.files`, `measure.files.<n>.load`, `measure.files.<n>.clear` | U5: 지우기는 `measure.clear` 확인과 되돌리기 | P31a |
| load_mdat: 파일 대화상자, 덮개 아래에서 해석, 파일의 위치·채널만 바꿈 | `measure.mdat.load`, `measure.mdat.dialog` | U8: 덮개 대신 트레이의 `jobs.job.mdat_import` | P31a |
| 진행 조건: 측정 중이거나 측정이 없으면 막힘 | `frame.header.blocked` | 측정 중 `measuring`, 다른 장치 잡 `device_busy`, 측정 방식 없음 `no_path`. 측정 없음은 보정 단계의 `no_measurements` | P31a |
| 건너뛰기 [측정 건너뛰기 (mdat 불러오기)] | `measure.mdat.load`, `measure.import.note` | mdat 경로(연결·점검 건너뜀)에서는 단계 머리의 주 동작이 mdat 가져오기 | P31a |

## 6. 보정 (screen 9) — P31b

| 항목 | id | 편차 | 작업서 |
|---|---|---|---|
| 채널 고르기(역할 코드, 계산 전에는 위치 1의 역할, BA에서 끔, XO에서 그 채널의 그룹으로 바꿈), 성적표 줄로도 고름 | `calibrate.channel`, `calibrate.scorecard`, `calibrate.scorecard.sort` | 없음 | P31b |
| 탭 SPL / GD / XO / BA(서브 2대 이상) | `calibrate.tab.spl`, `calibrate.tab.gd`, `calibrate.tab.xo`, `calibrate.tab.ba` | 없음 | P31b |
| SPL 타겟 편집: 점 끌기, 빈 곳 두 번 눌러 더하기, 점 오른쪽 클릭(또는 Delete)으로 빼기, 빈 곳 오른쪽 클릭으로 타겟 메뉴(그래프 위 '타겟' 단추로도 열림) | `calibrate.target.point.<n>`, `calibrate.plot`, `calibrate.target.menu` | 없음 | P31b |
| XO 탭의 XO 선 끌기(40–200 Hz, 끄는 동안 곡선이 따라오고 놓으면 저장) | `calibrate.xo.line.handle` | 없음 | P31b |
| 탭별 범례(보정 전, 보정 후, 타겟, MIMO 적용, 메인, 서브, 합, 정렬 전, 정렬 후, 서브 합) | `calibrate.plot.legend.<key>` | 없음 | P31b |
| 지표 줄 "±1 dB (20–200 Hz) a % → b %" · "GD 산포 (20–100 Hz) a → b ms"(판정 카드는 전체, 성적표는 채널별) | `calibrate.verdict.within`, `calibrate.verdict.gd`, `calibrate.verdict.spread`, `calibrate.scorecard` | 없음 | P31b |
| 성적표: 보정 전·후·MIMO 후 비교 범례, 기준 위치의 늦은 저음(20–150 Hz, dB)과 설명(견본 `screen.calibrate.mimo-scorecard`, `screen.calibrate.mimo-stale-scorecard`, `screen.calibrate.mimo-scorecard.min`) | `calibrate.scorecard.key`, `calibrate.scorecard.late` | 없음 | P110, P111 |
| 위치별 늦은 저음: 접힌 표, 위치 번호순과 기준 위치 표시, 현재 MIMO 결과가 있으면 세 값 비교(견본 `screen.calibrate.late-positions`, `screen.calibrate.one-position`) | `calibrate.scorecard.positions`, `calibrate.scorecard.positions.table` | 없음 | P110, P111 |
| 필터 줄 "PEQ n · FIR n탭 \| FIR 없음 · XO n Hz · MIMO 서포트 n"(고른 채널) | `calibrate.plot.filters` | 없음 | P31b |
| 전체 지연과 엔진 메모(메모는 접힌 목록, 업스트림처럼 늘 보임) | `calibrate.verdict.delay`, `calibrate.verdict.notes` | 없음 | P31b |
| BA 탭: 서브 합 평탄도(lo–hi Hz), 좌석 편차 또는 한 위치 | `calibrate.ba.metrics` | 없음 | P31b |
| BA 탭: 표(서브 \| 방식 \| 게인 \| 딜레이 \| 극성 \| 올패스, 서브 이름은 곡선 색) | `calibrate.ba.table` | 없음 | P31b |
| BA 탭: 고정 필터와 레이아웃 줄 | `calibrate.ba.filters` | 없음 | P31b |
| BA 탭: 빨간 경고(`sub_filter_warnings`, `sub_gain_warnings`) | `calibrate.ba.warnings` | 없음 | P31b |
| 첫 계산 전: 위치 1 미리보기와 안내(계산이 할 일, XO 추천을 먼저 함, 모드) | `calibrate.verdict.plan`, `calibrate.plot.preview` | 없음 | P31b |
| 모드: 최소 지연(`zero`), 낮은 지연(`mixed`, 기본), 표준(`linear`) | `calibrate.mode.zero`, `calibrate.mode.mixed`, `calibrate.mode.linear` | 없음 | P31b |
| 초고지연(`ultra`, 사양 줄 다섯 번 누르기로 열림)과 지연 한도 30–200 ms, 10 단위 | `calibrate.mode.ultra` (모드 목록에 항상 표시), `calibrate.mode.ultra_ms` | U9 | P31b, P58 |
| 사양 줄(`cal.mode_spec`) | `calibrate.mode.spec` | 없음 | P31b |
| 출력 레이트로 환산한 채널 FIR의 한도 초과와 FIR 없는 모드 안내 | `calibrate.mode.fir_over` | ADR 0017 | P116, P117 |
| [계산](첫 계산에서 XO 추천이 없으면 추천부터, 서비스가 정함) | `calibrate.compute` | 없음 | P31b |
| [취소](결과를 버림)와 XO·MIMO 카드의 취소 | `jobs.cancel.calibrate`, `jobs.cancel.recommend_xo`, `jobs.cancel.mimo` | U8 | P31b |
| 상태 줄: 계산 중, XO 추천 중, 실패, 취소, 대기, 측정 없음, 측정 바뀜, 설정 바뀜, 완료 | `calibrate.running`, `calibrate.verdict.failed`, `calibrate.verdict.stale`, `calibrate.stale`, `calibrate.verdict.plan`, `jobs.history` | U8 | P31b |
| 타겟 메뉴의 프리셋 플랫, 하만, 시네마 | `calibrate.target.preset.flat`, `calibrate.target.preset.harman`, `calibrate.target.preset.cinema` | 없음 | P31b |
| '사용자 지정' 표시 | `calibrate.target.custom` | 없음 | P31b |
| 타겟 JSON 저장·불러오기 | `calibrate.target.save`, `calibrate.target.load` | 없음 | P31b |
| 타겟 점 한계(10–24000 Hz, ±30 dB, 두 점 이상), 고치면 프리셋이 '사용자 지정'으로 바뀜 | `calibrate.target.point.<n>`, `calibrate.target.custom` | 없음 | P31b |
| 결과의 수명: 캐시에서 복원, 설정을 바꾸면 낡음만 표시, 새 계산이 MIMO와 서브만 결과를 비움 | `calibrate.verdict.stale`, `calibrate.stale`, `calibrate.verdict.mimo`, `calibrate.ba.subs_only` | 없음 | P31b |
| 이 단계의 숨은 내보내기 카드 | — | U3 | P31b |
| 진행 조건: 언제나(새 레일은 측정이 없거나 측정 중이면 막고, 지금 결과가 있으면 완료) | `calibrate.compute` | 레일이 결과 전에는 내보내기를 막고 이유를 보임(`no_result`). 업스트림은 내보내기를 열고 "계산 결과가 없습니다"를 보임(`step_calibration.py:2477`) | P31b |

## 7. 보정 설정 (inventory §5) — P31b

| 항목 | id | 편차 | 작업서 |
|---|---|---|---|
| RC 하한 20–200 Hz, 5 단위, 기본 20(직접 입력한 상한 이상이면 거부, 이유는 입력 옆) | `calibrate.range.lower` | D48 | P31b, P148 (ADR 0024) |
| RC 상한 기준(엄격한·너그러운 슈뢰더, 측정 위치 일치도, 직접 입력), 기본 엄격한 슈뢰더 | `calibrate.range.basis` | 없음 | P140 (ADR 0021) |
| RC 전이 구간 ×1·×2·×4(슈뢰더 기준일 때), 기본 ×1 | `calibrate.range.transition` | 없음 | P140 (ADR 0021) |
| RC 상한 100–20000 Hz, 50 단위(직접 입력일 때, 하한 이하이면 거부, 이유는 입력 옆) | `calibrate.range.upper` | 없음 | P31b, P148 (ADR 0024) |
| 보정 구간 추가(추가 구간이 없으면 이 단추만 보임, 4개를 채우면 숨김), 새 구간은 기본 구간과 다른 구간에 겹치지 않는 15000–20000 Hz부터 | `calibrate.range.extra.add` | D48 | P148 (ADR 0024) |
| 추가 구간 1~4: 하한과 상한 20–20000 Hz(1 kHz 아래 5 단위, 그 위 50 단위), 빼기, 거부된 값은 그 입력 옆에 이유를 보이고 저장된 값은 그대로 | `calibrate.range.extra.<n>`, `calibrate.range.extra.<n>.lower`, `calibrate.range.extra.<n>.upper`, `calibrate.range.extra.<n>.remove` | D48 | P148 (ADR 0024) |
| 룸 볼륨 0–100000 m³(0 = 없음) | `calibrate.range.volume` | 없음 | P31b |
| 방 치수 가로 × 세로 × 높이 0–100 m, 0.1 단위(셋 다 넣으면 볼륨) | `calibrate.range.dims.w`, `calibrate.range.dims.d`, `calibrate.range.dims.h` | 없음 | P31b |
| 부스트 상한 0–10 dB, 기본 10 | `calibrate.range.boost` | 없음 | P31b |
| 롤오프 아래 부스트 막기(극저역 보호), 기본 꺼짐 | `calibrate.range.low_protect`, 근거 줄은 계산 메모, 막지 못한 채널은 `calibrate.verdict.low_protect` 경고 | 없음 | P145 (ADR 0022) |
| RC 상한이 정해진 방법(엔진 메모와 룸 볼륨) | `calibrate.range.notes` | 없음 | P31b |
| XO 그룹 | `calibrate.xo.group` | 없음 | P31b |
| XO 40–200 Hz, 5 단위, 기본 80 | `calibrate.xo.hz` | 없음 | P31b |
| XO PEQ, 기본 켬 | `calibrate.xo.peq` | 없음 | P31b |
| XO PEQ 하한·상한 0–1000 Hz(0 = 자동: XO ½, XO × 4) | `calibrate.xo.peq_lo`, `calibrate.xo.peq_hi` | 없음 | P31b |
| "추천 N Hz" | `calibrate.xo.recommended` | 없음 | P31b |
| [XO 추천 계산](60/80/100/120 Hz), 자체 취소와 상태 | `calibrate.xo.recommend`, `calibrate.xo.state`, `jobs.cancel.recommend_xo` | U8 | P31b |
| 서브가 없으면 "서브가 없어 XO가 없습니다" | `calibrate.xo.none` | 없음 | P31b |
| 인프라 서브(없음 또는 서브), 인프라 XO 10–300 Hz(처음 25), 종류 LR24/LR12/LR48/BW12/BW24, 인프라 최대 부스트 0–24 dB | `calibrate.ba.infra`, `calibrate.ba.infra_hz`, `calibrate.ba.infra_type`, `calibrate.ba.infra_boost`, `calibrate.ba.infra_note` | 없음 | P31b |
| 서브마다 방식: 자동, 고정, 묶음 A, 묶음 B | `calibrate.ba.mode.<role>` | 없음 | P31b |
| 고칠 서브 고르기(업스트림 서브 콤보) | `calibrate.ba.edit` | 없음 | P31b |
| 묶음 레이아웃(상대값 고정, DBA, 멀티서브), DBA 줄 앞·뒤, 뒤 지연 보정 −20…20 ms | `calibrate.ba.layout`, `calibrate.ba.dba_row`, `calibrate.ba.rear_trim` | 없음 | P31b |
| 값(고정, 상대값 고정 묶음): 게인 −60…+20 dB 0.5 단위, 딜레이 0–500 ms 0.1 단위, 극성, 올패스 1–2(f 0–500 Hz, Q 0.1–20) | `calibrate.ba.gain`, `calibrate.ba.delay`, `calibrate.ba.polarity`, `calibrate.ba.allpass.<n>.hz`, `calibrate.ba.allpass.<n>.q` | 없음 | P31b |
| 고정 필터(모든 방식): HP·LP 0–300 Hz와 종류(종류를 고르면 HP 20, LP 80), PEQ 1–3(f, Q, 게인 −30…+12 dB) | `calibrate.ba.hp_type`, `calibrate.ba.hp`, `calibrate.ba.lp_type`, `calibrate.ba.lp`, `calibrate.ba.peq.<n>.hz`, `calibrate.ba.peq.<n>.q`, `calibrate.ba.peq.<n>.gain` | 없음 | P31b |
| 메인 추가 지연 0–100 ms(고정 서브가 있을 때) | `calibrate.ba.main_delay` | 없음 | P31b |
| MIMO 서포트로 쓰기, 기본 켬 | `calibrate.ba.mimo_support` | 없음 | P31b |
| [서브만 계산] | `calibrate.ba.compute_subs` | 없음 | P31b |
| [결과 값 가져오기] | `calibrate.ba.take_values` | 없음 | P31b |
| 서브별 요약과 빨간 경고 | `calibrate.ba.subs`, `calibrate.ba.settings_warnings` | 없음 | P31b |
| 서브가 2대 미만이면 BA 없음 | `calibrate.ba.none` | 없음 | P31b |
| [MIMO 사용], 기본 끔, 위치 3곳 미만이면 끄고 이유 | `calibrate.mimo.on` | 없음 | P31b |
| 서포트 주파수 상한 50–150 Hz, 5 단위, 기본 150 | `calibrate.mimo.fsiso` | 없음 | P31b |
| 하한 자동(기본) 또는 15–150 Hz(기본 20) | `calibrate.mimo.low_auto`, `calibrate.mimo.low` | 없음 | P31b |
| 입력당 최대 서포트 0–12(0 = 제한 없음) | `calibrate.mimo.max` | 없음 | P31b |
| FIR 서포트(끄면 IIR: PEQ 9 + AP 3, 켜면 8,000탭 FIR) | `calibrate.mimo.fir` | 없음 | P31b |
| [MIMO 계산](보정 결과 필요), 취소 | `calibrate.mimo.compute`, `calibrate.mimo.reason`, `jobs.cancel.mimo` | U8 | P31b |
| MIMO 상태: 계산 중, 먼저 보정 계산, 계산 전, 설정 바뀜, 완료 "FIR\|IIR 서포트 N · 입력당 ≤ cap · FIR 예산" | `calibrate.mimo.state`, `calibrate.running` | U2 | P31b |
| 그 밖의 입력: 타겟 곡선, DSP 프로필(MIMO FIR 예산), 새 프로젝트의 기본 위상 | `calibrate.target.menu`, `calibrate.mimo.state`, `calibrate.mode.mixed` | U2 | P31b |

## 8. 그래프 (inventory §4) — P31a, P31b

| 항목 | id | 편차 | 작업서 |
|---|---|---|---|
| 측정 그래프: 로그 x 5–22000 Hz와 고정 눈금, y −35…+20 dB, 이동·확대 없음, (역할, 위치)마다 곡선, 280점 1/12 옥타브, 모든 곡선에 같은 기준(위치 1의 첫 메인의 200–2000 Hz 평균, 서브만 있으면 첫 서브의 40–120 Hz 평균)을 적용하고 그 뜻을 `measure.plot.reference`에 표시 | `measure.plot`, `measure.plot.reference` | 업스트림은 곡선마다 자기 200–2000 Hz 평균으로 정규화(U17, #127). 평활과 정규화는 서비스(`measurement_curves`)가 하고 화면은 그리기만 | P31a, P143, P145 |
| 측정 곡선의 선 규칙: 채널 색, 서브 굵게, 위치 1 굵게, 다른 위치 가늘고 옅게, 칩에 커서를 대면 강조 | `measure.plot.legend.role.<role>`, `measure.plot.legend.position.<n>` | — | P31a |
| 측정 그래프의 축 범위 팝오버: 주파수·레벨 하한과 상한, [자동], [적용], "하한은 상한보다 낮아야", 실행 동안 기억 | `measure.plot.axes` | 범위는 `view.plotRange`이고 바꾸면 `setPlotRange`로 알림. P33이 실행 동안 보관(그래프를 숨기거나 단계를 옮겨도 유지) | P31a, P38 |
| 배치도 ArrayLayoutView (5단계) | `measure.map` | — | P31a |
| 보정 그래프(300–380 px 높이, 로그 x, 팬·줌 없음) | `calibrate.plot` | 없음 | P31b |
| SPL: 5–20000 Hz, 타겟 기준 dB(타겟 중앙 −30/+15), 보정 전, 보정 후, 타겟(점선), MIMO 적용 | `calibrate.tab.spl`, `calibrate.plot.legend.before`, `calibrate.plot.legend.after`, `calibrate.plot.legend.target`, `calibrate.plot.legend.mimo` | 없음 | P31b |
| GD: 5–5000 Hz, ms(1–99 백분위), 보정 전·후 | `calibrate.tab.gd` | 없음 | P31b |
| XO: 5–500 Hz, 타겟 기준 dB(−30/+12), 타겟·메인·서브·합(`cal.xo_preview`) | `calibrate.tab.xo`, `calibrate.plot.legend.main`, `calibrate.plot.legend.sub`, `calibrate.plot.legend.sum` | 없음 | P31b |
| BA: 5–300 Hz, 타겟 기준 dB(−40/+6), 서브별 정렬 전(점선)·후, 서브 합 전·후, 타겟 | `calibrate.tab.ba`, `calibrate.plot.legend.align_before`, `calibrate.plot.legend.align_after`, `calibrate.plot.legend.sub_sum` | 없음 | P31b |
| SPL: 타겟이 0 dB에 오도록 레벨 이동, 타겟 손잡이, RC 하한·상한 선과 라벨과 바깥 음영, 계산 전 위치 1 측정과 타겟, 표시 평활 1/24 옥타브 | `calibrate.target.point.<n>`, `calibrate.plot.band.lower`, `calibrate.plot.band.upper`, `calibrate.plot.preview`, `calibrate.plot.legend.measured` | 없음 | P31b |
| GD도 RC 선을 같이 씀, XO 평활 1/12 옥타브 | `calibrate.plot.band.lower`, `calibrate.plot.band.upper`, `calibrate.xo.line.handle` | 없음 | P31b |
| SPL과 GD: 추가 구간까지 합친 범위 밖 음영과 경계 점선, 추가 구간마다 라벨, 계산 메모의 보정 구간 집합(`rc_bands`) | `calibrate.plot.band.extra.<n>`, `calibrate.verdict.notes` | D48 | P148 (ADR 0024) |
| BA 곡선은 `cal.sub_combine(sub_run or run)`, 표와 같은 8색 순환 | `calibrate.ba.table`, `calibrate.ba.subs_only` | 없음 | P31b |

## 9. 내보내기 (screen 8) — P32

| 항목 | id | 편차 | 작업서 |
|---|---|---|---|
| 필터 형식별 출력 레이트: 48 kHz 이외의 고정값 또는 자동 선택 안내 | `export.kind.<kind>.rate` | ADR 0017 | P116, P117 |
| 컨볼루션 IR의 추가 레이트: 접힌 요약, 펼친 목록, 레이트별 선택 | `export.extra_rates`, `export.extra_rates.summary`, `export.extra_rates.list`, `export.extra_rates.<rate>` | ADR 0017 | P116, P117 |
| 자동으로 정한 출력 레이트, 48 kHz로 대신 저장한 이유와 시스템으로 이동 | `export.outcome.<kind>.rate`, `export.outcome.<kind>.rate_fallback`, `export.outcome.<kind>.go_system` | ADR 0017 | P116, P117 |
| 결과 줄: 모드 · 총 지연 · 채널 N개 전후 · 서브 N개 측정 그대로 · MIMO 적용 N | `export.result.summary`, `export.result.mimo` | — | P32 |
| 낡은 결과 알림(업스트림처럼 그래도 내보냄) | `export.result.stale`, `export.status.stale` | — | P32, P38 |
| 내보낼 것이 없는 이유(보정으로 가는 길) | `export.result.empty`, `export.go_calibrate`, `frame.header.blocked` | — | P32 |
| 역할 코드 | `export.result.roles` | — | P32 |
| RC 요약 | `export.result.rc` | — | P32 |
| 내보낼 폴더 표시 | `export.folder.path` | — | P32 |
| [변경…] → `extra.export.folder` | `export.folder.change` | — | P32 |
| [폴더 열기] | `export.folder.open` | — | P32 |
| [REW mdat 저장] → 보정 전후 mdat, `SECS_mdat_<RC_BA\|RC_BA_MIMO>_<모드>_<시각>/` | `export.run.before_after_mdat` | — | P32 |
| [CamillaDSP 내보내기] | `export.run.camilla_dsp`(단계 머리의 주 동작) | — | P32 |
| 필터 내보내기 카드(1.3.0): [텍스트로 내보내기] → `SECS_text_<RC_BA\|RC_BA_MIMO>_<모드>_<시각>/` | `export.group.filters`, `export.run.filter_text` | — | P47 |
| 필터 내보내기 카드(1.3.0): [EqAPO 내보내기] → `SECS_eqapo_<RC_BA\|RC_BA_MIMO>_<모드>_<시각>/` | `export.run.equalizer_apo`, `export.outcome.equalizer_apo.hint` | — | P47 |
| 지원 범위: 텍스트 = RC만, EqAPO = RC · BA, 카밀라 = 전부; 막힌 단추는 끄고 이유를 보임 | `export.kind.filter_text.blocked`, `export.kind.equalizer_apo.blocked` | — | P47 |
| 보정 화면의 숨은 내보내기 카드(단계별 mdat) | `export.advanced`, `export.run.staged_mdat` | U3 | P32 |
| 결과가 없거나 계산·내보내기 중이면 단추 끔 | `export.run.camilla_dsp`, `export.run.before_after_mdat`, `export.run.staged_mdat` | — | P32 |
| 메모: N개 파일 저장 → 경로 / 실패 / 튜토리얼에서는 저장 안 함 | `export.outcome.<kind>`, `export.outcome.<kind>.open` | — | P32 |
| 기본 폴더 `<프로젝트 폴더>/SECS_export/<날짜> <이름>/`, 없으면 `~/Documents/SECS_export/<날짜>` | `export.folder.path` | — | P32 |
| MIMO는 켜고, 계산했고, 위치 3곳 이상일 때만 포함 | `export.result.mimo` | — | P32 |
| 아무 일도 하지 않는 [완료] 단추 | — | U4 | P32 |
| CamillaDSP·Equalizer APO 적용 방식: 파일만 / 적용 | `export.apply.<target>.panel`, `export.apply.<target>.level`, `export.apply.<target>.level.files`, `export.apply.<target>.level.apply` (`<target>`은 `camilla` 또는 `eqapo`) | 업스트림에 없음, ADR 0018 | P121, P122 |
| [CamillaDSP에 적용], 적용 방식이 '적용'이면 설정 저장은 행의 보조 동작 | `export.apply.camilla`, `export.run.camilla_dsp`, `export.status.problem` | 업스트림에 없음, ADR 0018 | P121, P122 |
| CamillaDSP 주소·포트 검사, 다른 컴퓨터의 주소 안내, 실제 입출력 장치 안내 | `export.apply.camilla.address.field`, `export.apply.camilla.address`, `export.apply.camilla.address.problem`, `export.apply.camilla.address.remote`, `export.apply.camilla.devices_note` | 업스트림에 없음, ADR 0018 | P121, P122 |
| CamillaDSP 연결·버전·상태, 이 설계 / 다른 설계 / 알 수 없음, 클리핑, [다시 확인] | `export.apply.camilla.state`, `export.apply.camilla.reason`, `export.apply.camilla.refresh`, `export.apply.camilla.design`, `export.apply.camilla.design.title`, `export.apply.camilla.clipped` | 업스트림에 없음, ADR 0018 | P121, P122 |
| [Equalizer APO에 적용], Windows가 아니거나 설치되지 않았으면 끄고 이유 표시 | `export.apply.eqapo`, `export.apply.eqapo.level`, `export.apply.eqapo.state`, `export.apply.eqapo.reason`, `export.apply.eqapo.refresh` | 업스트림에 없음, ADR 0018 | P121, P122 |
| Equalizer APO 판·설정 폴더·config.txt 줄 상태, 현재 결과와 적용한 설계 비교 | `export.apply.eqapo.state`, `export.apply.eqapo.folder`, `export.apply.eqapo.line`, `export.apply.eqapo.line_note`, `export.apply.eqapo.design` | 업스트림에 없음, ADR 0018 | P121, P122 |
| [config.txt에서 빼기], 한 문장으로 확인한 뒤 SECS 2R 줄만 제거 | `export.apply.eqapo.revert`, `export.apply.eqapo.revert.confirm`, `export.apply.eqapo.revert.confirm.cancel`, `export.apply.eqapo.revert.confirm.confirm` | 업스트림에 없음, ADR 0018 | P121, P122 |
| 적용 완료·실패·이전 설정 복구·줄 제거 완료, 쓰기 권한 없는 폴더 표시 | `export.apply.<target>.outcome`, `export.apply.<target>.outcome.open`, `export.apply.<target>.outcome.path`, `export.apply.camilla.outcome.camilla_error` | 업스트림에 없음, ADR 0018 | P121, P122 |

적용 견본(P121, P122)은 다음 21개이며 [`DESIGN.md`](DESIGN.md) 12절과 같습니다.

- CamillaDSP 파일만·장치 안내·확인 중·연결 실패: `screen.export.apply.camilla.files`, `screen.export.apply.camilla.placeholders`, `screen.export.apply.camilla.checking`, `screen.export.apply.camilla.unreachable`
- CamillaDSP 설계·진행·결과: `screen.export.apply.camilla.this`, `screen.export.apply.camilla.other`, `screen.export.apply.camilla.applying`, `screen.export.apply.camilla.applied`, `screen.export.apply.camilla.rolled-back`
- CamillaDSP 주소 오류·다른 컴퓨터: `screen.export.apply.camilla.address`, `screen.export.apply.camilla.remote`
- Equalizer APO 운영체제·설치·줄 없음: `screen.export.apply.eqapo.not-windows`, `screen.export.apply.eqapo.missing`, `screen.export.apply.eqapo.absent`
- Equalizer APO 설계 비교·줄 손상: `screen.export.apply.eqapo.present`, `screen.export.apply.eqapo.other`, `screen.export.apply.eqapo.official`, `screen.export.apply.eqapo.damaged`
- Equalizer APO 권한·줄 제거·확인: `screen.export.apply.eqapo.denied`, `screen.export.apply.eqapo.reverted`, `screen.export.apply.eqapo.revert-confirm`

## 10. 화면 전체 (inventory §2) — P29, P32

| 항목 | id | 편차 | 작업서 |
|---|---|---|---|
| 창 제목 "<버전> · 측정 위저드", 튜토리얼 표시, 1280×800, 밝은 테마 고정 | `frame`(레일 머리의 판 번호와 튜토리얼 표시) | — | P29 |
| 단계 막대: 단계 누르기, 완료 / 지금 / 아직 | `frame.rail`, `frame.rail.<step>` | — | P29 |
| 사이드바: 구성 요약 | `frame.status.config` | — | P29 |
| 사이드바: 측정 k / N | `frame.status.positions` | — | P29 |
| 사이드바: 보정 상태 | `frame.status.calibration` | — | P29 |
| 사이드바: REW 연결 | `frame.status.path` | — | P29 |
| 사이드바: 프로젝트 파일·저장 안 됨·튜토리얼 | `frame.status.file`, `frame.project` | — | P29 |
| [도움말] | `frame.help` | — | P29 |
| [튜토리얼] / [튜토리얼 끝내기] | `help.intro.tutorial`, `project.tutorial.start`, `tutorial.panel.end`, `tutorial.end.finish` | — | P32 |
| [튜토리얼 다시] | `tutorial.panel.restart`, `tutorial.end.restart` | — | P32 |
| [프로젝트 저장 ▾ 다른 이름으로 저장…] | `frame.project.save`, `frame.project.save_as` | — | P29 |
| [프로젝트 열기] | `frame.project.open`, `project.open`, `project.open.recent.<n>`, `project.open.browse` | — | P29, P32 |
| 숨긴 언어 줄 | `frame.settings.language` | U7 | P29 |
| 내비 [← 이전] / [다음 →] | `frame.rail.<step>`(Alt+← / Alt+→) | — | P29 |
| 가운데 메시지(막힌 이유, 건너뛰기 메모) | `frame.header.blocked` | — | P29 |
| 1, 3, 5단계의 건너뛰기 단추 | `frame.rail.connect`(mdat 경로에서 건너뜀); 각 단추는 §1, §3, §5 | — | P29, P30, P31a |
| 마지막 단계의 [완료] | — | U4 | P32 |
| 단계 이동: 모니터 시작·정지, REW 새로 읽기, "화면 준비 중…" | `frame.rail.<step>`, `jobs.tray` | U8 | P29 |
| 새 프로젝트(업스트림은 단추 없음, 기본 2.0 · CamillaDSP · 20 Hz–20 kHz · 혼합 위상) | `frame.project.new`, `project.new`, `project.new.start`, `project.new.preset`, `project.new.subs`, `project.new.create` | — | P29, P32 |
| 저장(Ctrl+S): 경로 또는 대화상자, `.irs` 복사, SPL 오프셋, 옆 `.mdat` | `frame.project.save`, `project.unsaved.save` | — | P29, P32 |
| 다른 이름으로 저장(Ctrl+Shift+S) | `frame.project.save_as` | — | P29 |
| 열기(Ctrl+O) → `open_project` | `frame.project.open`, `project.open.browse` | — | P29, P32 |
| 닫기·튜토리얼 전 저장 질문: 저장 / 저장 안 함 / 취소 | `project.unsaved`, `project.unsaved.save`, `project.unsaved.discard`, `project.unsaved.cancel` | — | P32 |
| 닫을 때 모니터·측정·계산·MIMO 워커 멈춤 | `project.unsaved`(닫는 길; 잡 정지는 P33의 호스트) | U8 | P32 |
| 언어 전환과 문구(`t()`, 카탈로그), 살아 있는 툴팁 | `frame.settings.language` | U7 | P29 |
| 툴팁(약 150개, 800 ms, 420 px) | `frame.rail.<step>`(막힌 이유 툴팁; 모든 화면의 키트 툴팁) | — | P29 |
| 닫힌 콤보 위의 휠은 페이지를 스크롤 | `frame.content`(기본 select는 휠로 값이 바뀌지 않음) | — | P29 |
| 전문가 모드·정보 창 없음 | — (전역 전문가 표시 없음); 로고 선택은 `frame.settings.logo` | U9 | P29, P58 |
| REW 감시 3초마다(3, 5, 8단계에서 멈춤) | `frame.status.path` | — | P29 |
| 튜토리얼 모드: 데모 프로젝트, 가짜 REW, 분홍 테마, 저장·내보내기 막음 | `project.tutorial`, `frame`(튜토리얼 강조색) | — | P32 |
| 기다림 표시: 로딩 카드, 작업 패널, 시작 화면 | `jobs.tray`, `jobs.job.<kind>`, `jobs.cancel.<kind>` | U8 | P29 |
| 메시지가 여러 곳에 흩어짐, 알림 모음 없음 | `jobs.history` | U8 | P29 |
| 단축키 Alt+← / Alt+→, Ctrl+1…5 | `frame.rail.<step>` | — | P29 |
| 단축키 Ctrl+S, Ctrl+Shift+S, Ctrl+O | `frame.project.save`, `frame.project.save_as`, `frame.project.open` | — | P29 |
| F1 도움말 | `frame.help`, `help` | — | P29, P32 |
| Ctrl+M 떠 있는 마이크 미터 | 연결·점검의 마이크 미터 패널(§3, P30); 단축키는 P33 | — | P30 |
| 도움말 Ctrl+F, Enter 다음, Esc | `help.search`, `help.search.next`, `help.close` | — | P32 |
| 배치도 화살표, 축 메뉴 Enter | `<id>.position.<n>`(§4, §5), `<id>.axes`(§8) | — | P30, P31a, P31b |
| 도움말 창: 비모달, 제목 "도움말 · <단계>", "k / 7" 칩, 검색 | `help`, `help.step`, `help.search` | — | P32 |
| 도움말 탭: 처음이라면, 따라 하기, 확인할 것, 문제 해결, 실제 REW, 측정의 mdat, 내보내기의 CamillaDSP | `help.tab.<tab>` | — | P32 |
| 도움말 그림을 누르면 큰 보기 | —(그림 없이 새 화면에 맞게 다시 씀, M5 §2) | — | P32 |
| 도움말이 지금 단계를 따라감 | `help`, `help.step` | — | P32 |

## 11. 튜토리얼 (inventory §3) — P32, P35

엔진은 P32, 실제 퀘스트는 P35의 [`quests.ts`](../apps/secs2r/ui/src/screens/tutorial/quests.ts)에 있습니다. 현재 업스트림 `tutorial_quests.py`의 목록은 38개가 아니라 43개입니다. 전부 대응시키고 새 레일 순서로 정렬했습니다. 아래 줄 번호는 업스트림 파일의 퀘스트 시작 줄입니다.

- 시스템(구성 2 → 배치 4)을 연결·점검(장치 1 → 레벨 3)보다 먼저 둡니다. 각 단계 안에서는 업스트림 순서를 지킵니다.
- `system.layout_next`와 `connect.level_next`는 같은 단계 안의 영역 소개이므로 `act`에서 `look`으로 바꿉니다. 없는 다음 화면 단추를 만들지 않습니다.
- 업스트림 327(배치 단계의 측정 옵션 카드)은 모의 경로에 없는 타이밍 기준 대신 측정 시작 지연을 소개합니다. 시작 지연은 측정·가져오기에서만 정하므로(P58) 그 화면의 측정 옵션 카드를 가리키는 측정 단계의 첫 퀘스트 `measure.delay`로 둡니다. 단계 안의 업스트림 순서는 그대로입니다.
- `connect.retry`는 실제 REW를 켜는 대신 화면의 모의 측정을 선택합니다. 장치·마이크 소개는 모의 경로에 없는 REW 카드 대신 `connect.method`를 가리킵니다.
- `plant_wrong_out`은 App의 튜토리얼 준비와 재시작에서 수행합니다. SW3 출력이 6이고 16이 비었을 때만 16으로 바꿉니다. 퀘스트는 실제 `setOutput` 액션으로 고칩니다.
- 보정 탭은 호스트의 `selectTab`으로 엽니다. BA 고급은 기존 `CalibrateScreen.initial.editSub`로 SW3을 선택하여 엽니다. 마크업과 스타일은 바꾸지 않습니다.
- 모의 경로에서 없는 REW 연결·오디오 카드는 측정 방식 영역으로, 아직 생기지 않은 결과별 범례·필터 줄은 그래프 카드로 가리킵니다. 별도 id 추가는 필요하지 않습니다.
- `tutorial.live.<step>` 다섯 갤러리 표본은 실제 App을 모의 서비스에서 실행합니다. 내보내기에는 업스트림 행동 퀘스트가 없으므로 첫 보기 퀘스트를 찍습니다.

| 퀘스트 id | 단계 | 종류 | 표적 `data-ui` | 업스트림 줄 |
|---|---|---|---|---|
| system.presets | system | look | system.preset, system.subs | 304 |
| system.patch | system | look | system.patch | 306 |
| system.dsp | system | look | system.dsp | 307 |
| system.output | system | act | system.patch.SW3.out | 308 |
| system.layout_next | system | look | system.layout | 314 |
| system.layout | system | look | system.arrangement.map | 326 |
| system.single | system | look | system.arrangement.1 | 329 |
| system.next | system | act | system.next | 332 |
| connect.sidebar | connect | look | frame.status, frame.help, frame.project | 288 |
| connect.rail | connect | look | frame.rail | 289 |
| connect.connection | connect | look | connect.method | 293 |
| connect.audio | connect | look | connect.method | 296 |
| connect.retry | connect | act | connect.method.mock | 297 |
| connect.level_next | connect | look | connect.level | 302 |
| connect.master | connect | look | connect.level.master, connect.level.play_all | 316 |
| connect.levels | connect | act | connect.level.play_all, connect.level.skip | 317 |
| connect.next | connect | act | frame.rail.measure | 324 |
| measure.delay | measure | look | measure.options | 327 |
| measure.positions | measure | look | measure.positions | 334 |
| measure.sample | measure | act | measure.sample.load | 335 |
| measure.graph | measure | look | measure.plot | 339 |
| measure.mdat | measure | look | measure.mdat.load | 341 |
| measure.next | measure | act | frame.rail.calibrate | 344 |
| calibrate.mode | calibrate | look | calibrate.settings.mode, calibrate.compute | 346 |
| calibrate.graph | calibrate | look | calibrate.graph | 349 |
| calibrate.tabs | calibrate | look | calibrate.tab.spl, calibrate.tab.gd, calibrate.tab.xo, calibrate.settings | 352 |
| calibrate.compute | calibrate | act | calibrate.compute | 356 |
| calibrate.spl | calibrate | look | calibrate.graph | 365 |
| calibrate.metrics | calibrate | look | calibrate.verdict, calibrate.plot.filters | 370 |
| calibrate.gd | calibrate | look | calibrate.graph | 375 |
| calibrate.xo_open | calibrate | act | calibrate.tab.xo | 379 |
| calibrate.xo | calibrate | look | calibrate.graph, calibrate.settings.xo | 384 |
| calibrate.ba_open | calibrate | act | calibrate.tab.ba | 389 |
| calibrate.ba_graph | calibrate | look | calibrate.graph | 395 |
| calibrate.ba_metrics | calibrate | look | calibrate.ba.metrics, calibrate.ba.table | 400 |
| calibrate.ba_settings | calibrate | look | calibrate.ba.subs, calibrate.ba.edit, calibrate.ba.editor | 408 |
| calibrate.infra | calibrate | act | calibrate.ba.infra | 415 |
| calibrate.ba_filters | calibrate | look | calibrate.ba.fixed | 424 |
| calibrate.subs | calibrate | act | calibrate.ba.compute_subs | 430 |
| calibrate.ba_after | calibrate | look | calibrate.graph | 442 |
| calibrate.next | calibrate | act | frame.rail.export | 449 |
| export.mdat | export | look | export.run.before_after_mdat | 451 |
| export.camilla | export | look | export.run.camilla_dsp | 453 |

검증: `screens/tutorial/quests.test.tsx`가 업스트림 전체 줄 목록과 순서, 모든 표적의 실제 App DOM, 모든 행동의 화면 훅·자동 실행·막힘 판정, 두 카탈로그의 키·자리표시자, 계속 단추로 끝 카드까지의 진행을 확인합니다.

| 항목 | id | 편차 | 작업서 |
|---|---|---|---|
| `set_trial(True)`로 시작(데모 프로젝트, 튜토리얼 강조색) | `project.tutorial.start`, `tutorial` | — | P32 |
| `when()`이 거짓인 퀘스트는 뺌 | `tutorial`(`Quest.when`) | — | P32 |
| 끝에서 처음부터 다시 또는 끝내기 | `tutorial.end`, `tutorial.end.restart`, `tutorial.end.finish` | — | P32 |
| 퀘스트 칸: 화면, 종류, 표적, 문구, 왜, 끝 판정(`done`, `base`), `auto`+메모, `busy`+문구, `keep`, `ok`, `blocked`, `animate`, `apart` | `tutorial.coach`(`screens/tutorial/types.ts`의 `Quest`) | — | P32 |
| 용어 25개를 처음 나온 카드 또는 [왜?] 아래에 | `tutorial.coach`, `tutorial.coach.why` | — | P32 |
| 덮개: 표적 말고 모두 어둡게 | `tutorial.overlay` | — | P32 |
| 보기 표적: 점선, 누를 수 없음(휠은 지나감) | `tutorial.overlay.look` | — | P32 |
| 해 보기 표적: 강조색 실선, 퍼지는 고리, 누를 수 있음 | `tutorial.overlay.act` | — | P32 |
| `apart`: 표적마다 따로 테두리 | `tutorial.overlay`(`Quest.apart`) | — | P32 |
| 튜토리얼 단추는 언제나 누를 수 있음 | `tutorial.coach`, `tutorial.panel` | — | P32 |
| 다른 곳을 누르면 카드가 흔들리고 알림 | `tutorial.toast` | — | P32 |
| 표적을 먼저 보이게 스크롤 | `tutorial.overlay` | — | P32 |
| 코치 카드(340 px, 끌기): "<단계> · k / n", 잠시 치우기, 문구, 용어, [왜?], 힌트, [← 이전], [계속 →]; 표적·레일·머리를 피해 놓임 | `tutorial.coach`, `tutorial.coach.pause`, `tutorial.coach.why`, `tutorial.coach.rule`, `tutorial.coach.prev`, `tutorial.coach.next` | — | P32 |
| 해 보기: 첫 [계속]은 힌트를 흔들고, 둘째는 `auto`; 막히면 이유, 진행 중이면 그 문구 | `tutorial.coach.rule`, `tutorial.coach.blocked`, `tutorial.coach.next` | — | P32 |
| 틱: 끝나면 넘어감, 표적이 0.45초 넘게 없으면 건너뜀, 단계 끝 알림 | `tutorial.toast` | — | P32 |
| 퀘스트 목록(오른쪽 위, 알약으로 접힘) | `tutorial.panel`, `tutorial.panel.fold`, `tutorial.panel.pill` | — | P32 |
| 잠시 치우기: 모두 숨기고 이어 하기 단추만; 이어 하면 막힌 단계로 되감기 | `tutorial.coach.pause`, `tutorial.resume` | — | P32 |
