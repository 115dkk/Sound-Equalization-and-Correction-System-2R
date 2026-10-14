# M5 도달표: 업스트림 기능이 새 화면에서 닿는 자리

[`M5-inventory.md`](M5-inventory.md)의 항목마다 새 화면에서 그것에 닿는 화면 요소의 id(`data-ui` 또는 같은 값의 `id`)를 적습니다. M5는 이 표에 빈칸이 없어야 끝납니다([`M5.md`](M5.md) 6절). 화면 테스트가 표의 id들이 실제로 그려지는지 확인합니다.

- 한 줄은 목록의 한 항목입니다. 항목 이름은 목록의 낱말을 짧게 옮기고, 자리 표시는 `M5-inventory.md`의 소절을 따릅니다.
- **id**는 화면 요소의 안정된 id입니다. 여럿이면 쉼표로 잇습니다. 역할이나 위치처럼 바뀌는 부분은 `<role>`, `<n>`으로 적습니다.
- **편차**는 업스트림과 다르게 닿는 자리입니다([`DEVIATIONS.md`](DEVIATIONS.md)의 번호). 없앤 기능(U4)은 id 칸에 `—`를 적고 편차 번호를 답니다.
- **작업서**는 그 줄을 채운 작업서입니다. 연결(P33) 뒤에 id가 바뀌면 그 작업서가 고칩니다.

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
| 스윕 길이 64k…4M "길이 · 초", 기본 256k | `connect.sweep_length` | 없음 | P30 |
| REW가 떠 있는데 장치 목록이 비면 다시 읽기(2초마다, 15번) | `connect.input_device`, `connect.devices.retry` (다시 읽기는 `status_watch`의 `MeasureStatus`, P33) | 없음 | P30 |
| 루프백 입력 채널(타이밍 기준이 케이블일 때, 자체 측정) | `connect.loopback_channel` | 없음 | P30 |
| 장치 공유: 자동 / 독점 / 공유(전문가 표시, 자체 측정) | `connect.share_mode` | 없음 | P30 |
| 진행 조건: REW 실행과 Pro("REW 를 켜 주세요.") | `connect.rew.hint`, `connect.rew.launch` | 없음 | P30 |
| [장치 확인 건너뛰기] | `connect.method.mdat`, `connect.mdat`, `connect.next` | 없음 | P30 |

## 2. 구성 (Step 2) — P30

| 항목 | id | 편차 | 작업서 |
|---|---|---|---|
| 프리셋 2.0 / 2.1 / 2.2 / 5.1 / 7.1.4 / 9.1.6, 지금 프리셋 강조, 측정 지움 | `system.preset`, `system.preset.<name>`, `system.confirm` | U5 | P30 |
| 빈 구성 / 사용자 정의 | `system.preset.empty`, `system.preset.custom` | U5 | P30 |
| 서브 증감기 0–8, 측정 지움 | `system.subs`, `system.confirm` | U5 | P30 |
| 바이앰프 시작 체크박스(업스트림 `BIAMP_UI=False`) | — | 숨김 유지(`M5.md` 5절) | P30 |
| 측정 출력 장치와 채널 수 알약(미확인 / N개 초과 / 채널 N개) | `system.device` | 없음 | P30 |
| 표 묶음: 프론트, 센터, 서브, 서라운드, 백, 와이드, 탑, 하이트, 기타 | `system.patch` | 없음 | P30 |
| 채널: 역할("코드 · 돌비 이름", 사용자 정의), 서브 역할이면 SUB 대역 | `system.patch.<role>.role` | 없음 | P30 |
| 크기: 소형 · XO 적용 / 대형 · 풀레인지, 서브는 알약 | `system.patch.<role>.size` | 없음 | P30 |
| 앰프 구성과 대역 칩(바이앰프와 함께 숨김) | `system.patch.<role>.band` (전문가 표시, 읽기 전용 대역 열); 앰프 구성 — | 숨김 유지(`M5.md` 5절) | P30 |
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
| 직접 고치면 사용자 지정, "창 ≈ x ms · 해상도 ≈ y Hz" | `system.dsp.note` | U2 | P30 |
| 프리셋 값(탭 / 예산 / IIR / AP / BM) | `system.dsp.device` (`dsp_set { preset }`) | U2 | P30 |
| 진행 조건: 스피커 하나 이상, `validate()` 통과 | `frame.header.blocked`, `system.next` | 없음 | P30 |

## 3. 레벨 체크 (Step 3) — P30

| 항목 | id | 편차 | 작업서 |
|---|---|---|---|
| 채널 줄(`ordered_speakers` 순서): 재생/멈춤, 역할, 이름, 미터, 값; 저장된 값 미리 채움 | `connect.level`, `connect.level.row.<role>`, `connect.level.meter.<role>` | 없음 | P30 |
| 재생(분홍 잡음, 서브 50 Hz 사인), 다른 재생 단추 잠금, 2.5초 뒤 멈춤 | `connect.level.play.<role>` | 없음 | P30 |
| 레벨 기록(`extra['levels']`, `extra['spl']`), 단계가 보이는 동안 모니터 | `connect.level.meter.<role>`, `connect.mic.state` | 없음 | P30 |
| 마스터 출력 −40…0 dBFS, 0.5 단계, 기본 −20; "재생 중 · 코드" 알약 | `connect.level.master`, `connect.level.playing` | 없음 | P30 |
| [전 채널 자동 순차 재생] | `connect.level.play_all`, `connect.level.stop` | 없음 | P30 |
| 마이크 입력 라벨(채널 · 장치) | `connect.mic.input` | 없음 | P30 |
| 내장 마이크 미터: 미터, dBFS, 피크, 상태, [피크 지우기] | `connect.mic`, `connect.mic.meter`, `connect.mic.peak`, `connect.mic.state`, `connect.mic.clear_peak` | 없음 | P30 |
| 떠 있는 마이크 미터(Ctrl+M; 단축키는 P33) | `frame.mic` (`MicMeterPanel`의 `floating`) | 없음 | P30 |
| 진행 조건: 레벨이 없는 채널("레벨을 아직 못 잰 채널") | `connect.status` | 없음 | P30 |
| [레벨 건너뛰기 (기본값 적용)] | `connect.level.skip` | 없음 | P30 |
| 레벨 체크 결과로 측정 스윕 레벨 정하기(`level_check`, `level_apply`) | `connect.level.advanced`, `connect.level.sweep.check`, `connect.level.sweep.apply` | U1 | P30 |

## 4. 배치 (Step 4) — P30

| 항목 | id | 편차 | 작업서 |
|---|---|---|---|
| 배치 카드 1 / 9 / 13 / 17(작은 배치도, "위치 N곳"), 다른 카드를 누르면 측정 지움 | `system.layout`, `system.arrangement`, `system.arrangement.<n>`, `system.confirm` | U5 | P30 |
| 측정 배치도: 스크린, 1 m 원, 의자·소파, 머리, 30 cm 축척, 번호 표식(위치 1, ±20 cm, 완료·지금) | `system.arrangement.map`, `system.arrangement.map.position.<n>` | 없음 | P30 |
| 측정 시작 지연: 끔 / 5 / 10 / 15초 | `system.start_delay` | 없음 | P30 |
| 위치당 스윕 | `system.sweeps` | 없음 | P30 |
| 예상 시간: 위치당과 전체(`position_seconds`) | `system.estimate` | 없음 | P30 |
| 타이밍 기준: 음향(기준 스피커) / 루프백(케이블) / 없음 | `system.timing` | 없음 | P30 |
| 기준 출력: 스피커(`timing_ref_role`, 기본 R) 또는 채널(`timing_ref_output`) | `system.timing.role`, `system.timing.output` | 없음 | P30 |
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
| 옵션: 시작 지연, 스윕 길이, 반복 1/2/4/8(음향 타이밍 기준이면 1) | `measure.options.delay`, `measure.options.sweep`, `measure.options.repeats`, `measure.options.repeats.acoustic` | — | P31a |
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
| 채널 고르기(역할 코드, 계산 전에는 위치 1의 역할, BA에서 끔, XO에서 그 채널의 그룹으로 바꿈), 성적표 줄로도 고름 | `calibrate.channel`, `calibrate.scorecard`, `calibrate.scorecard.sort` | | P31b |
| 탭 SPL / GD / XO / BA(서브 2대 이상) | `calibrate.tab.spl`, `calibrate.tab.gd`, `calibrate.tab.xo`, `calibrate.tab.ba` | | P31b |
| SPL 타겟 편집: 점 끌기, 빈 곳 두 번 눌러 더하기, 점 오른쪽 클릭(또는 Delete)으로 빼기, 빈 곳 오른쪽 클릭으로 타겟 메뉴(그래프 위 '타겟' 단추로도 열림) | `calibrate.target.point.<n>`, `calibrate.plot`, `calibrate.target.menu` | | P31b |
| XO 탭의 XO 선 끌기(40–200 Hz, 끄는 동안 곡선이 따라오고 놓으면 저장) | `calibrate.xo.line.handle` | | P31b |
| 탭별 범례(보정 전, 보정 후, 타겟, MIMO 적용, 메인, 서브, 합, 정렬 전, 정렬 후, 서브 합) | `calibrate.plot.legend.<key>` | | P31b |
| 지표 줄 "±1 dB (20–200 Hz) a % → b %" · "GD 산포 (20–100 Hz) a → b ms"(판정 카드는 전체, 성적표는 채널별) | `calibrate.verdict.within`, `calibrate.verdict.gd`, `calibrate.verdict.spread`, `calibrate.scorecard` | | P31b |
| 필터 줄 "PEQ n · FIR n탭 \| FIR 없음 · XO n Hz · MIMO 서포트 n"(고른 채널) | `calibrate.plot.filters` | | P31b |
| 전체 지연과 엔진 메모(메모는 전문가 표시) | `calibrate.verdict.delay`, `calibrate.verdict.notes` | | P31b |
| BA 탭: 서브 합 평탄도(lo–hi Hz), 좌석 편차 또는 한 위치 | `calibrate.ba.metrics` | | P31b |
| BA 탭: 표(서브 \| 방식 \| 게인 \| 딜레이 \| 극성 \| 올패스, 서브 이름은 곡선 색) | `calibrate.ba.table` | | P31b |
| BA 탭: 고정 필터와 레이아웃 줄 | `calibrate.ba.filters` | | P31b |
| BA 탭: 빨간 경고(`sub_filter_warnings`, `sub_gain_warnings`) | `calibrate.ba.warnings` | | P31b |
| 첫 계산 전: 위치 1 미리보기와 안내(계산이 할 일, XO 추천을 먼저 함, 모드) | `calibrate.verdict.plan`, `calibrate.plot.preview` | | P31b |
| 모드: 최소 지연(`zero`), 낮은 지연(`mixed`, 기본), 표준(`linear`) | `calibrate.mode.zero`, `calibrate.mode.mixed`, `calibrate.mode.linear` | | P31b |
| 초고지연(`ultra`, 사양 줄 다섯 번 누르기로 열림)과 지연 한도 30–200 ms, 10 단위 | `calibrate.mode.ultra`, `calibrate.mode.ultra_ms` | U9 | P31b |
| 사양 줄(`cal.mode_spec`) | `calibrate.mode.spec` | | P31b |
| [계산](첫 계산에서 XO 추천이 없으면 추천부터, 서비스가 정함) | `calibrate.compute` | | P31b |
| [취소](결과를 버림)와 XO·MIMO 카드의 취소 | `jobs.cancel.calibrate`, `jobs.cancel.recommend_xo`, `jobs.cancel.mimo` | U8 | P31b |
| 상태 줄: 계산 중, XO 추천 중, 실패, 취소, 대기, 측정 없음, 측정 바뀜, 설정 바뀜, 완료 | `calibrate.running`, `calibrate.verdict.failed`, `calibrate.verdict.stale`, `calibrate.stale`, `calibrate.verdict.plan`, `jobs.history` | U8 | P31b |
| 타겟 메뉴의 프리셋 플랫, 하만, 시네마 | `calibrate.target.preset.flat`, `calibrate.target.preset.harman`, `calibrate.target.preset.cinema` | | P31b |
| '사용자 지정' 표시 | `calibrate.target.custom` | | P31b |
| 타겟 JSON 저장·불러오기 | `calibrate.target.save`, `calibrate.target.load` | | P31b |
| 타겟 점 한계(10–24000 Hz, ±30 dB, 두 점 이상), 고치면 프리셋이 '사용자 지정'으로 바뀜 | `calibrate.target.point.<n>`, `calibrate.target.custom` | | P31b |
| 결과의 수명: 캐시에서 복원, 설정을 바꾸면 낡음만 표시, 새 계산이 MIMO와 서브만 결과를 비움 | `calibrate.verdict.stale`, `calibrate.stale`, `calibrate.verdict.mimo`, `calibrate.ba.subs_only` | | P31b |
| 이 단계의 숨은 내보내기 카드 | — | U3 | P31b |
| 진행 조건: 언제나(새 레일은 측정이 없거나 측정 중이면 막고, 지금 결과가 있으면 완료) | `calibrate.compute` | | P31b |

## 7. 보정 설정 (inventory §5) — P31b

| 항목 | id | 편차 | 작업서 |
|---|---|---|---|
| RC 하한 10–200 Hz, 5 단위, 기본 20 | `calibrate.range.lower` | | P31b |
| RC 상한 자동(슈뢰더, 최대 500 Hz), 기본 켬 | `calibrate.range.upper_auto` | | P31b |
| RC 상한 100–20000 Hz, 50 단위(자동이 꺼졌을 때) | `calibrate.range.upper` | | P31b |
| 룸 볼륨 0–100000 m³(0 = 없음) | `calibrate.range.volume` | | P31b |
| 방 치수 가로 × 세로 × 높이 0–100 m, 0.1 단위(셋 다 넣으면 볼륨) | `calibrate.range.dims.w`, `calibrate.range.dims.d`, `calibrate.range.dims.h` | | P31b |
| 부스트 상한 0–10 dB, 기본 10 | `calibrate.range.boost` | | P31b |
| RC 상한이 정해진 방법(엔진 메모와 룸 볼륨) | `calibrate.range.notes` | | P31b |
| XO 그룹 | `calibrate.xo.group` | | P31b |
| XO 40–200 Hz, 5 단위, 기본 80 | `calibrate.xo.hz` | | P31b |
| XO PEQ, 기본 켬 | `calibrate.xo.peq` | | P31b |
| XO PEQ 하한·상한 0–1000 Hz(0 = 자동: XO ½, XO × 4) | `calibrate.xo.peq_lo`, `calibrate.xo.peq_hi` | | P31b |
| "추천 N Hz" | `calibrate.xo.recommended` | | P31b |
| [XO 추천 계산](60/80/100/120 Hz), 자체 취소와 상태 | `calibrate.xo.recommend`, `calibrate.xo.state`, `jobs.cancel.recommend_xo` | U8 | P31b |
| 서브가 없으면 "서브가 없어 XO가 없습니다" | `calibrate.xo.none` | | P31b |
| 인프라 서브(없음 또는 서브), 인프라 XO 10–300 Hz(처음 25), 종류 LR24/LR12/LR48/BW12/BW24, 인프라 최대 부스트 0–24 dB | `calibrate.ba.infra`, `calibrate.ba.infra_hz`, `calibrate.ba.infra_type`, `calibrate.ba.infra_boost`, `calibrate.ba.infra_note` | | P31b |
| 서브마다 방식: 자동, 고정, 묶음 A, 묶음 B | `calibrate.ba.mode.<role>` | | P31b |
| 고칠 서브 고르기(업스트림 서브 콤보) | `calibrate.ba.edit` | | P31b |
| 묶음 레이아웃(상대값 고정, DBA, 멀티서브), DBA 줄 앞·뒤, 뒤 지연 보정 −20…20 ms | `calibrate.ba.layout`, `calibrate.ba.dba_row`, `calibrate.ba.rear_trim` | | P31b |
| 값(고정, 상대값 고정 묶음): 게인 −60…+20 dB 0.5 단위, 딜레이 0–500 ms 0.1 단위, 극성, 올패스 1–2(f 0–500 Hz, Q 0.1–20) | `calibrate.ba.gain`, `calibrate.ba.delay`, `calibrate.ba.polarity`, `calibrate.ba.allpass.<n>.hz`, `calibrate.ba.allpass.<n>.q` | | P31b |
| 고정 필터(모든 방식): HP·LP 0–300 Hz와 종류(종류를 고르면 HP 20, LP 80), PEQ 1–3(f, Q, 게인 −30…+12 dB) | `calibrate.ba.hp_type`, `calibrate.ba.hp`, `calibrate.ba.lp_type`, `calibrate.ba.lp`, `calibrate.ba.peq.<n>.hz`, `calibrate.ba.peq.<n>.q`, `calibrate.ba.peq.<n>.gain` | | P31b |
| 메인 추가 지연 0–100 ms(고정 서브가 있을 때) | `calibrate.ba.main_delay` | | P31b |
| MIMO 서포트로 쓰기, 기본 켬 | `calibrate.ba.mimo_support` | | P31b |
| [서브만 계산] | `calibrate.ba.compute_subs` | | P31b |
| [결과 값 가져오기] | `calibrate.ba.take_values` | | P31b |
| 서브별 요약과 빨간 경고 | `calibrate.ba.subs`, `calibrate.ba.settings_warnings` | | P31b |
| 서브가 2대 미만이면 BA 없음 | `calibrate.ba.none` | | P31b |
| [MIMO 사용], 기본 끔, 위치 3곳 미만이면 끄고 이유 | `calibrate.mimo.on` | | P31b |
| 서포트 주파수 상한 50–150 Hz, 5 단위, 기본 150 | `calibrate.mimo.fsiso` | | P31b |
| 하한 자동(기본) 또는 15–150 Hz(기본 20) | `calibrate.mimo.low_auto`, `calibrate.mimo.low` | | P31b |
| 입력당 최대 서포트 0–12(0 = 제한 없음) | `calibrate.mimo.max` | | P31b |
| FIR 서포트(끄면 IIR: PEQ 9 + AP 3, 켜면 8,000탭 FIR) | `calibrate.mimo.fir` | | P31b |
| [MIMO 계산](보정 결과 필요), 취소 | `calibrate.mimo.compute`, `calibrate.mimo.reason`, `jobs.cancel.mimo` | U8 | P31b |
| MIMO 상태: 계산 중, 먼저 보정 계산, 계산 전, 설정 바뀜, 완료 "FIR\|IIR 서포트 N · 입력당 ≤ cap · FIR 예산" | `calibrate.mimo.state`, `calibrate.running` | U2 | P31b |
| 그 밖의 입력: 타겟 곡선, DSP 프로필(MIMO FIR 예산), 새 프로젝트의 기본 위상 | `calibrate.target.menu`, `calibrate.mimo.state`, `calibrate.mode.mixed` | U2 | P31b |

## 8. 그래프 (inventory §4) — P31a, P31b

| 항목 | id | 편차 | 작업서 |
|---|---|---|---|
| 측정 그래프: 로그 x 5–22000 Hz와 고정 눈금, y −35…+20 dB, 이동·확대 없음, (역할, 위치)마다 곡선, 280점 1/12 옥타브, 200–2000 Hz 평균 정규화 | `measure.plot` | 평활과 정규화는 서비스(`measurement_curves`)가 하고 화면은 그리기만 | P31a |
| 측정 곡선의 선 규칙: 채널 색, 서브 굵게, 위치 1 굵게, 다른 위치 가늘고 옅게, 칩에 커서를 대면 강조 | `measure.plot.legend.role.<role>`, `measure.plot.legend.position.<n>` | — | P31a |
| 측정 그래프의 축 범위 팝오버: 주파수·레벨 하한과 상한, [자동], [적용], "하한은 상한보다 낮아야", 실행 동안 기억 | `measure.plot.axes` | 범위는 화면이 열려 있는 동안 기억(그래프를 숨겨도 유지). 단계를 옮겨도 남기려면 P33이 범위를 보관해야 함 | P31a |
| 배치도 ArrayLayoutView (5단계) | `measure.map` | — | P31a |
| 보정 그래프(300–380 px 높이, 로그 x, 팬·줌 없음) | `calibrate.plot` | | P31b |
| SPL: 5–20000 Hz, 타겟 기준 dB(타겟 중앙 −30/+15), 보정 전, 보정 후, 타겟(점선), MIMO 적용 | `calibrate.tab.spl`, `calibrate.plot.legend.before`, `calibrate.plot.legend.after`, `calibrate.plot.legend.target`, `calibrate.plot.legend.mimo` | | P31b |
| GD: 5–5000 Hz, ms(1–99 백분위), 보정 전·후 | `calibrate.tab.gd` | | P31b |
| XO: 5–500 Hz, 타겟 기준 dB(−30/+12), 타겟·메인·서브·합(`cal.xo_preview`) | `calibrate.tab.xo`, `calibrate.plot.legend.main`, `calibrate.plot.legend.sub`, `calibrate.plot.legend.sum` | | P31b |
| BA: 5–300 Hz, 타겟 기준 dB(−40/+6), 서브별 정렬 전(점선)·후, 서브 합 전·후, 타겟 | `calibrate.tab.ba`, `calibrate.plot.legend.align_before`, `calibrate.plot.legend.align_after`, `calibrate.plot.legend.sub_sum` | | P31b |
| SPL: 타겟이 0 dB에 오도록 레벨 이동, 타겟 손잡이, RC 하한·상한 선과 라벨과 바깥 음영, 계산 전 위치 1 측정과 타겟, 표시 평활 1/24 옥타브 | `calibrate.target.point.<n>`, `calibrate.plot.band.lower`, `calibrate.plot.band.upper`, `calibrate.plot.preview`, `calibrate.plot.legend.measured` | | P31b |
| GD도 RC 선을 같이 씀, XO 평활 1/12 옥타브 | `calibrate.plot.band.lower`, `calibrate.plot.band.upper`, `calibrate.xo.line.handle` | | P31b |
| BA 곡선은 `cal.sub_combine(sub_run or run)`, 표와 같은 8색 순환 | `calibrate.ba.table`, `calibrate.ba.subs_only` | | P31b |

## 9. 내보내기 (screen 8) — P32

| 항목 | id | 편차 | 작업서 |
|---|---|---|---|

## 10. 화면 전체 (inventory §2) — P29, P32

| 항목 | id | 편차 | 작업서 |
|---|---|---|---|

## 11. 튜토리얼 (inventory §3) — P32, P35

| 항목 | id | 편차 | 작업서 |
|---|---|---|---|
