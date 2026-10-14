# M5 도달표: 업스트림 기능이 새 화면에서 닿는 자리

[`M5-inventory.md`](M5-inventory.md)의 항목마다 새 화면에서 그것에 닿는 화면 요소의 id(`data-ui` 또는 같은 값의 `id`)를 적습니다. M5는 이 표에 빈칸이 없어야 끝납니다([`M5.md`](M5.md) 6절). 화면 테스트가 표의 id들이 실제로 그려지는지 확인합니다.

- 한 줄은 목록의 한 항목입니다. 항목 이름은 목록의 낱말을 짧게 옮기고, 자리 표시는 `M5-inventory.md`의 소절을 따릅니다.
- **id**는 화면 요소의 안정된 id입니다. 여럿이면 쉼표로 잇습니다. 역할이나 위치처럼 바뀌는 부분은 `<role>`, `<n>`으로 적습니다.
- **편차**는 업스트림과 다르게 닿는 자리입니다([`DEVIATIONS.md`](DEVIATIONS.md)의 번호). 없앤 기능(U4)은 id 칸에 `—`를 적고 편차 번호를 답니다.
- **작업서**는 그 줄을 채운 작업서입니다. 연결(P33) 뒤에 id가 바뀌면 그 작업서가 고칩니다.

## 1. 장치·마이크 (Step 1) — P30

| 항목 | id | 편차 | 작업서 |
|---|---|---|---|

## 2. 구성 (Step 2) — P30

| 항목 | id | 편차 | 작업서 |
|---|---|---|---|

## 3. 레벨 체크 (Step 3) — P30

| 항목 | id | 편차 | 작업서 |
|---|---|---|---|

## 4. 배치 (Step 4) — P30

| 항목 | id | 편차 | 작업서 |
|---|---|---|---|

## 5. 측정 (Step 5) — P31a

| 항목 | id | 편차 | 작업서 |
|---|---|---|---|

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
