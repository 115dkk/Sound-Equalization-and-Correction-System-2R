# 구조 안내 — 처음 코드를 보는 사람에게 (2026-09-20, 2026-10-10 슬림화 반영)

한 줄 요약: **REW 로 스피커를 재고 → 보정 설계(새 엔진)로 필터를 계산하고 → REW mdat · 카밀라 DSP 설정 · 필터 텍스트 · EqAPO 설정으로 내보내는** 프로그램.
2026-10-10 슬림화로 1.41 옛 화면·엔진, 옛 6·7·8′ 화면과 옛 엔진·DSP 시트·Tesira·검증을 지웠다.
버전별 변화는 `CHANGELOG.md` 에 있다.

## 1. 큰 그림

```
 사람 ─▶ 위저드 (secs/wizard): 1 장치 → 2 구성 → 3 레벨 → 4 배치 → 5 측정 → 보정 설계(화면 9) → 내보내기(화면 8)
            │  읽고 씀                                   │                         │
            ▼                                            ▼                         ▼
      Project (.secs JSON) ◀── 모든 화면 값      calibration.py ──▶ correction_engine.py   REW mdat(보정 전·후) · 카밀라 설정
            │                                            │          mimo*.py(MIMO)
   ┌────────┴──────────┐                                 │
   ▼                   ▼                                 │
 rewapi(측정·Fake)   rewapi/session.py ── IR(wav, .irs/) ┴── load_project_irs
```

- **값은 한곳에**: 화면은 상태를 따로 들지 않고 `secs.model.Project` 를 읽고 쓴다. 저장하면 그대로 `.secs`(JSON)가 된다. 측정한 임펄스 응답은 옆의 `<이름>.irs/` 폴더에 wav 로 남고, 위치별 IR 은 `rewapi.session.load_project_irs` 가 읽는다(창이 든 IR 캐시를 나눠 쓴다).
- **엔진은 Qt 를 모른다**: `calibration.py` · `correction_engine.py` · `mimo*.py` · `scorecard.py` · `export_camilladsp.py` · `rewapi/interfaces.py` 는 순수 파이썬이라 화면 없이 테스트한다. 화면은 워커 스레드(무거운 XO 채점·MIMO 는 spawn 워커 프로세스)에서 이들을 부른다.
- **`.secs` 형식은 그대로**: 옛 엔진용 칸(`mso`·`bc`·`xo`·`DspProfile` 의 경로 지연·올패스 자원·`FOCUS_STEPS` …)은 옛 파일을 그대로 읽게 남겨 두었다(쓰는 엔진은 지웠다).

## 2. 폴더

| 경로 | 무엇 |
|---|---|
| `src/secs/` | 제품 소스. 아래 3절. |
| `tests/` | pytest. `room_model_decay.py` = 합성 감쇠 방(보정 설계 테스트 입력), `synth.py` = 합성 측정, `test_wizard_common.py` = 위저드 테스트 공용 픽스처. (1.41 골든 `golden/` 은 10-10 에 지웠다.) |
| `tools/` | 개발 도구: 실사용 QA 캡처, 번역 누락 찾기, 성능 점검, 실장비 REW 확인(`rew_live_*`). |
| `dist/`, `build/` | PyInstaller 산출물(git 제외). `build_exe.ps1` 이 만든다. |

## 3. `src/secs/` 안

### 3.1 새로 만든 것(2.x) — 주로 여기를 고친다
| 모듈 | 하는 일 | 주로 부르는 쪽 |
|---|---|---|
| `version.py` | 제품 이름·버전(한 곳). | `__main__`, 위저드 창 |
| `model.py` | 데이터 모델: `ROLES`(L R C SW1~8 … 탑 Tfl~Tbr · 하이트 Hfl Hfr Hbl Hbr — 10-10 에 그룹 '탑' · '하이트' 로 나눔, `.secs` 판 2 · 판 1 의 '하이트' XO 는 불러올 때 '탑' 으로 `migrate_group_keys`; `.secs` 판 3 — 10-10 기능 이름 정리, 판 2 이하의 저역 감쇠 설정 키를 `mimo_*` 로 `migrate_calibration_keys`), `ordered_speakers`(화면 목록 · 측정 순서를 역할 순서로 — 저장 순서는 그대로, 측정은 서브를 맨 뒤로), `SpeakerConfig`(스피커·대역·출력), `DspProfile`(09-24 개정 6: `bm_path_delays` = 베이스 관리 경로 지연 수), `TargetCurve`, `Project`(.secs 읽기/쓰기·검증). `DspProfile.allocate_taps()`(개정 7, 09-24)는 채널 FIR(Mixed Phase)·서포트 FIR(하이브리드/FIR) 을 **저역 통합까지 포함해 예산을 고르게** 나눈 권장값(순수 함수, `fir_taps_max` = 하드웨어 상한과 따로). `FOCUS_STEPS`(0/25/50/75/100)·`snap_focus()` 는 옛 7단계 최적화 목표 5단계(`.secs` 의 `focus` 반올림에만). 옛 엔진용 칸은 `.secs` 형식이라 남겼다. | 전부 |
| `rewapi/interfaces.py` | REW 와의 약속(측정 계획 `MeasurementPlan`, 결과, 예외). Qt 없음. | 위저드 1·3·5단계 |
| `rewapi/rew.py` · `callback.py` | 실제 REW API(`RealREW`, 포트 4735) + REW 가 진행 상황을 알려 오는 로컬 콜백 서버. | `wizard.run_wizard` |
| `rewapi/fake.py` | REW 없이 도는 가짜(`FakeREW`). 테스트·샘플 모드·QA 도구. `use_sample_dolby=True`(샘플 모드 전용, `secs.wizard.build_backend` 가 켠다)면 원측정 14개(7.1.4 의 L·R·C·Ls·Rs·Lb·Rb·Tfl·Tfr·Tbl·Tbr + 서브 SW1 DBA L · SW2 DBA R · SW3 INFRA — 10-11, 샘플 구성 `new_project(fake=True)` = 7.1.4 + 서브 3)에 `rewapi/sample_dolby_714.npz`(2026-03-10 돌비룸 실측, `tools\make_sample_dolby.py [mdat 경로]` 로 재생성 — 원본 돌비룸 실측 mdat(167 MB)은 이 저장소에 없다)의 실측 IR·시간 원점을 준다. 그 밖의 role·기존 테스트(플래그 기본 꺼짐)는 그대로 합성. | 테스트, `--fake-rew` |
| `rewapi/session.py` | 측정 결과를 `.secs` 와 `.irs/` 에 기록(`MeasurementSession`·`IrSaver`). `load_project_irs(project, path, cache=)` = `.secs` 기록 → 위치별 IR(10-10 옛 `design` 에서 옮김). | 5단계, 창, 보정 설계 |
| `calibration.py` | **보정 설계 계산 묶음**(2026-09-26, Qt 없음): `CalibrationSettings`(= `project.extra['calibration']`) → `compute(project, irs, settings)` → `CalibrationRun`(`CorrectionEngine`·결과·채널 지표·표시 곡선·풀린 RC 상한·전체 지연·측정 지문) · `compute_mimo(run, fsiso, f_low)` → `MimoRun`(`mimo.design_mimo` → `mimo_iir.design_mimo_iir`, 위치 3곳 이상) · `export_mdat`(REW 단계별: 보정 전 탑·서브, XO 타깃 탑·서브, 합 후 보정, MIMO 적용) · `export_camilladsp` · `export_before_after_mdat`(내보내기 단계: 채널마다 보정 전 = 원 측정 IR, 보정 후 = `stage_responses(...)['final']`, MIMO 적용, 서브 원 측정) · `rc_pieces`(RC 상한이 정해진 까닭 = 엔진 노트 + 방 부피). 설정의 방 부피 `room_volume_m3`·치수 `room_dims_m`(09-26, 치수 셋이면 부피 = 곱). 서브 결합 수동 설정(10-05 R27): `subs_manual`(역할 → `sub_entry` — 자동/고정/묶음 · 값 · MIMO 서포트 · 고정 출력 필터 HP·LP·PEQ — 엔진이 그 서브 스펙트럼에 곱한다) · `sub_main_delay_ms`(고정이 있을 때 메인 추가 지연) → 엔진 `CorrectionEngine(sub_manual=…)`(모두 자동이면 지금 경로 `_align_subs`, 아니면 `_align_subs_manual`), `take_sub_values`(결과 값 → 칸), `compute_mimo(exclude=)`, mdat 노트 `sub_values_text`. 바이앰프 대역 측정(`'L:LF'`)은 아직 뺀다. | 보정 설계 화면, 내보내기 화면 |
| `correction_engine.py` · `mimo.py` · `mimo_iir.py` | **보정 엔진**(2026-09-25~, Qt 없음, 10-10 에 이름 정리): `CorrectionEngine` 이 측정 → 레벨 · 딜레이 → **서브 정렬(BA, Bass Alignment)** → 채널 보정 필터(RC, 입력 쪽 PEQ + FIR)를 풀어 `CorrectionResult` 를 낸다(`MODES` = 보정 모드). **MIMO**(다입력 다출력 저역 감쇠): 입력마다 다른 스피커(서브 · 다른 메인)에 저역 서포트 신호를 더해 늦은 에너지를 줄인다 — `mimo.design_mimo`(서포트 고르기 · 대역 · 이상 저속 FIR → `MimoResult`), `mimo_iir.design_mimo_iir`(DSP 에 올릴 IIR 서포트 `IirSupport`, 입력마다 워커 프로세스 · `abort_workers`). 서포트 필터는 IIR(PEQ 9 + 올패스 3, 밴드 한도 `mimo_iir._knobs` — 10-12 기본 ±15 dB · Q≤10 · 대역 안 · 벌점 0.05) 또는 FIR(10-12 'FIR 사용' = `CalibrationSettings.mimo_fir` → `compute_mimo(fir=True)`: `design_mimo(taps_low=FIR_TAPS_LOW)` 의 저속 FIR 을 `mimo.FirSupport` 로, IIR 맞춤 없음, 48 kHz 8,000탭 Conv 로 내보냄). `MimoRun.iir` 에 둘 중 하나가 들고 `MimoRun.fir` · `calibration.mimo_kind` 로 가른다. 위치 3곳 이상. | `calibration` |
| `scorecard.py` · `export_camilladsp.py` | 성적표 지표 `metrics()`(±1 dB 비율·위상 바퀴·GD 폭·1/24 GD 봉우리) · 카밀라 DSP v4 설정(`make_config`/`write_config`, ASCII 만 — GUI 서버가 CP949 로 읽는다). 내보낸 폴더 = CamillaGUI 폴더 모양(10-10): `configs/secs_<모드>.yml` + `coeffs/secs_<모드>_<역할>_fir.wav`, FIR 경로는 설정 기준 `../coeffs/…`(카밀라가 설정 파일 폴더 기준으로 찾는다 — 4.1.3 실측), 파이프라인 맨 앞 · 끝 1:1 `route_in` · `route_out`(GUI Mixers 탭에서 번호만 바꿔 라우팅), 머리 주석 끝에 사용 안내(`howto_lines`: 순서표 · 장치 · 지연). | 보정 설계, 도구 |
| `export_filters.py` | **필터 내보내기**(10-15): 엔진 결과 → `FilterPlan`(채널마다 입력 보정 · 메인 경로 · 서브 센드 · LFE · 서브 버스 · MIMO 갈래의 필터 순서 — 카밀라와 같은 신호 흐름) → 텍스트(`write_text`, 한국어 UTF-8 BOM + FIR wav, 아무 DSP 에나 손으로) · Equalizer APO 설정째로(`write_eqapo`: `config.txt` = 머리 여유 Preamp · 입력 Copy · `Include` · 출력 Copy — 사람이 고치는 건 번호 두 줄, `secs_<모드>/` = 필터 본체 + FIR wav, 한국어 안내. 채널마다 한 신호라 메인 · 센드 · 서브 버스 · MIMO 를 가상 채널 `vm*` · `vs*` · `vb*` · `vp*` 로 나눠 처리, 장치 번호는 윈도우 채널 순서로 기본값). `plan_responses` = 계획의 전기 응답(엔진 응답 함수) — 테스트가 엔진 최종 응답과 대조, 머리 여유 계산. | `calibration.export_filter_text` · `export_eqapo` → 내보내기 화면 |
| `rew_mdat/serialization/writer.py` · `rew_mdat/builder.py` | 자바 직렬화 원시 그래프를 핸들·클래스 서술자·주석 블록까지 바이트 동일하게 되쓰고, `template_1.mdat` 의 측정 하나를 복제해 제목·노트·샘플레이트·IR·시간 원점과 IR 기반 응답 캐시를 바꾼 새 REW 프로젝트를 만든다. | 보정 설계·내보내기 mdat, 저장 때 곁 mdat, mdat 쓰기 테스트 |
| `rew_mdat/source_info.py` | 원 측정의 REW 측정 환경(마이크 캘 곡선 · 입출력 장치 · 볼륨 · OS)을 JSON 사전으로 꺼내고(`extract` · REW 측정은 `from_rew_measurement`) 템플릿 측정에 넣는다(`apply`, 없으면 `scrub` 으로 비움). mdat 불러오기 · REW 측정이 만든 사전은 `<프로젝트>.irs/<role>_P<pos>.src.json` 에 남고(`IrSaver.save_source` · `load_project_sources`), 내보내기 '보정 전' 은 그대로, '보정 후' 는 같은 환경 + `computed` 표시. 템플릿 제작자 환경이 남의 mdat 에 들어가던 버그(10-17) 때문에 생겼다. 템플릿 정리는 `tools/scrub_mdat_template.py`. | builder, mdat_loader, rewapi.rew · session, calibration.before_after_items, step8 · 곁 mdat |
| `i18n.py` · `translations.py` | 영/한 표시 계층: 번역 위젯, `translate()`(정확 일치 → `{0}` 패턴 → 줄·조각), 언어 관리자. | 위저드 |

### 3.2 위저드 `wizard/`
| 파일 | 하는 일 |
|---|---|
| `__init__.py` | `run_wizard(argv)` — REW(실제/가짜)를 골라 창을 띄운다. `BIAMP_UI`(10-11 부터 False — 2단계 바이앰프 칸 · 앰프 구성 · 대역 열을 만들지 않음, 모델 · 엔진의 바이앰프는 그대로) · `open_project`(`.secs` 열기 — 실행 인자 · [불러오기] · 튜토리얼 끝내기, 바이앰프가 닫혀 있으면 `single_amp` 로 단일 앰프로, 파일은 그대로). |
| `window.py` | 창 뼈대: 사이드바·단계 바·내비, 단계 이동 `go()`, 엔진 주입, 저장/불러오기, 단축키, 언어 전환. |
| `step_calibration.py` | **보정 설계**(2026-09-26, 옛 6·7 통합 — 화면 번호 `strings.CAL_STEP` = 9, 진행바 6번째). 오른쪽 칸 '서브 결합 설정' 카드(10-05 R27, 서브 2개 이상: 서브 하나씩 골라 방식·값·MIMO 서포트, `_refresh_sub_controls`). 모드(`correction_engine.MODES` zero/mixed/linear + 숨은 초고지연 `ultra` — 최고사양에서 사양 줄 5번, 지연 30–200 ms → `calibration.ultra_params`) · RC 범위 · MIMO · 결과(SPL/군지연 — SPL · XO · 서브 정렬 탭 곡선은 모두 타깃 기준으로 옮겨 그린다, `_level_offset`, 10-10) · 내보내기(REW mdat · 카밀라). 계산은 `secs.calibration` 을 daemon 워커 스레드(보통 이하 우선순위)에서 부르고 신호로 받는다(`_Bridge`) — 엔진은 끊을 수 없어 [취소] 는 결과를 버린다. 설정은 `project.extra['calibration']`. 창 흐름은 `strings.FLOW` = (1, 2, 3, 4, 5, 9, 8)(옛 6·7 화면은 10-10 에 지웠다). 번역 `en_g8`, 풍선 도움말 `cal.*`. |
| `step8_mdat.py` | **내보내기**(2026-09-26 새로 — 화면 번호 8, 진행바 7번째, 제목 '7 · 내보내기'). 보정 설계 화면의 `run`·`mimo_run`(켜고 계산했을 때만)을 읽어 [REW mdat 저장] → `<내보낼 폴더>/SECS_export_<모드>_<날짜시간>/SECS 보정 전후 <모드>.mdat` (`calibration.export_before_after_mdat`). 결과가 없거나 계산 중이면 버튼이 꺼지고 이유가 보인다. 창이 보정 설계의 `changed`·`computed` 에 `refresh` 를 잇는다. 옛 8단계(`step8_export`, 시트·Tesira·검증, 화면 번호 10)는 10-10 슬림화로 지웠다. 번역 `en_g8`, 풍선 도움말 `s8.export_before_after`·`s8.folder_*`. |
| `step1_devices.py` … `step5_measure.py` | 단계 화면 하나씩(파일 번호 = 화면 번호): 1 장치·마이크(REW 연결) → 2 구성 → 3 레벨 → 4 배치 → 5 측정, 이어서 보정 설계(9) · 내보내기(8). 창은 화면을 뜻 이름으로 든다 — `window.PAGE_NAMES` = 화면 번호 → 이름 dict(`devices_page` `config_page` `level_page` `layout_page` `measure_page` `export_page` `calibration_page`). 3·4·5·8 은 번호 이름 `step3`…`step5`·`step8` 도 같은 화면. 번호 6·7·10(옛 필터 설계·저역 통합·내보내기·검증)은 10-10 에 지운 화면의 빈 번호다(8·9 를 바꾸지 않으려고 — `go()` 는 ValueError). **`window.step1`·`step2` 는 없다** — 예전 `step1` 이 구성이었어서 헷갈린다. 타이밍 레퍼런스는 4단계 '측정 옵션' 카드(`timing.py`, 값은 `project.extra['devices']`). |
| `timing.py` | 타이밍 레퍼런스 선택지·기본 기준 스피커·`timing_settings(project)`(5단계 계획이 읽는다). 4단계가 고른다. |
| (단계 화면 공통) | 공통 모양: `__init__(project, …)` → `_build()`(위젯 만들기) → `refresh()`(project 값으로 다시 채우기) → `can_advance()`(다음으로 가도 되는지와 이유) + `changed` 시그널. 오래 걸리는 단계(5·보정 설계·8)는 `cancel()`/`wait()` 가 있다. |
| `axis_range.py` | 그래프 축 범위 조절 `AxisControl`(09-24): 그래프 오른쪽 위 [축] → 네 칸 + [적용]·[자동]. 그래프는 자기 기본 범위를 잡은 뒤 `apply()` 를 부르고, 값은 종류(`kind`)별로 실행 동안만(`_SESSION`) 기억한다. 5단계 측정 그래프. |
| `legend.py` | 다점 위치 표시: `position_pen`/`position_z`(위치 1 굵고 진하게) · `PositionChips`(위치 켜고 끄기·커서 강조) — 5단계. 상태는 화면 위젯에만 있고 `.secs` 에는 없다. (옛 6·7단계 범례 단추 `LegendToggle` 은 10-10 에 지웠다.) |
| `plot.py` | 그래프 공용: 주파수 고정 눈금(`freq_ticks`)·로그 주파수 범위(`set_freq_range` — 끝 눈금 글자 '5'·'20k' 가 숨지 않게 양 끝 여유)·축 이름(`axis_label` — ink2 색을 직접 준다, pyqtgraph `setPen` 이 이름 색을 옅은 격자색으로 덮는다)·곡선 색(`C_MEASURED`·`C_CORRECTED`·`C_TARGET`)·표시 스무딩 폭(`octave_smoothing_sigma`). 5단계 채널 곡선 색은 `style.channel_colors`(메인 16색 · 서브 진한 색 굵은 선, 10-10). 옛 6단계 그래프(`DesignPlot`·`XoStrip`)는 10-10 에 지웠다. |
| `layout_map.py` | 스피커 배치도·마이크 위치 그림(QGraphicsScene). |
| `style.py` | 색·QSS·공용 부품(`card`, `label`, `pill`, `help_box`, `seg` …). 색은 목업이 기준. **단계 공통 2열 배치 `two_columns(outer)`**(09-23 사용자: 단계마다 좌우 크기가 달라진다): 본문 칸(남는 폭 전부) + 오른쪽 칸 `SideColumn`(폭 = 본문 영역 폭 × 0.4 를 300~400 px 로 자른 값, 모든 단계 같은 값 — 창이 `set_side_width` 로 맞춘다). 두 칸의 위쪽은 제목 바로 아래 같은 y, 칸 사이 16 px, 카드 사이 12 px. 오른쪽 칸: 1 마이크·오디오, 2 배치도·DSP(좁은 창에선 표 아래로 쌓임), 3 마스터 출력, 4 측정 옵션, 5 위치 목록, 보정 설계는 탭별 카드. 8단계는 오른쪽 칸 없음. 단계 스크롤 영역은 세로 막대 자리를 늘 둔다(막대 유무로 폭이 흔들리지 않게). 점검: `tools\qa_capture.py --geometry`, 회귀 `tests/test_layout_alignment.py`. |
| `branding.py`, `assets/` | 로고: 창 아이콘(`app.ico`)·사이드바 마크(`app_256.png`). 원본은 `assets/logo_b6.svg`(+ 작은 크기용 `logo_b6_small.svg`), 다시 뽑기는 `tools\render_logo.py --ico … --small …`. exe 아이콘은 `SECS.spec` 의 `icon=`. |
| `qt.py` | `QtWidgets` 대역 — 글자를 가진 위젯을 번역 위젯으로 바꿔 준다. 단계 파일은 `from secs.wizard import qt as QtWidgets`. |
| `strings.py`, `en_g1.py`, `en_g2.py`, `en_g4.py`, `en_g8.py` | `t()` 와 영어 번역표. **화면 문구는 한국어 원문으로 쓰고** 여기에 (한국어, 영어) 쌍을 더한다. `en_g4` 는 도움말·로딩 창, `en_g8` 은 보정 설계·내보내기. 옛 `en_g3`(8′)·`en_g7`(7단계)은 10-10 에 지우고 지금 화면도 쓰는 쌍만 `en_g2` 끝으로 옮겼다. 채널 이름(돌비 표기)처럼 번역하면 안 되는 글자는 `style.plain_label`. |
| `tooltips.py`, `en_g5.py` | **풍선 도움말**(09-24): 버튼·칸·표 머리·그래프의 기능 설명 정본 `TIPS`(키 `<구역>.<이름>` — `win`·`s1`~`s5`·`s8`·`cal`·`mdat`·`load`). 단계 화면이 `attach(위젯, 키)`·`attach_header(표, 키들)`·`attach_cells(표, 키)` 로 달고, 창이 `install(app)` 한 `TipFilter` 가 ToolTip 이벤트에서 지금 언어로 옮겨 띄운다(0.8 s, 폭 420 px, 원래 툴팁은 첫 줄). 영어는 `en_g5.EN`(같은 키). 새 버튼·칸은 여기 한 줄 + `attach`(`tests/test_tooltips.py` 가 전수 검사). |
| `mic_meter.py` | 마이크 입력 미터 창(10-07, 사이드바 [마이크 미터] · Ctrl+M). REW `/input-levels/last-levels` 를 워커 스레드에서 0.25 s 폴링(3단계 구독과 안 겹침), 값이 굳으면 Start 를 다시 보낸다. |
| `help_dialog.py`, `tutorial_text.py`, `en_g6.py`, `assets/tutorial/` | [도움말](F1) = **따라 하기 튜토리얼**(09-24). `tutorial_text.py` 가 문안(`INTRO` 처음이라면 · `TUTORIAL[1..8]` 따라 하기/확인할 것/문제 해결/실제 REW · 그림 번호 설명 `CAPTIONS`), `en_g6.py` 가 같은 모양의 영어 표에서 (한국어, 영어) 쌍을 만든다. `help_dialog.py` 는 비모달 창 — 탭마다 문서를 새로 그리고, 그림은 `assets/tutorial/<key>_<ko|en>.png`(+ `MANIFEST.json`)를 문서 자원 `tut:<key>` 로 걸어 창 폭에 맞추며(120 ms 디바운스), 누르면 `ImageViewer` 로 크게 본다. 그림은 `tools/make_tutorial_shots.py` 로 다시 뽑는다(화면·문구가 바뀌면 문안과 같이). 5단계 덧붙임 탭 「mdat 불러오기」(`EXTRA_TABS` · `NAMES` · `NAMES_CAPTIONS`, 10-10)는 그림 위주 — 그림 4장은 `tools/make_mdat_help_shots.py` 가 만든다(진짜 `MdatAssignDialog` 캡처 + QPainter). 내보내기 덧붙임 탭 「CamillaDSP」(`CAMILLA` · `CAMILLA_CAPTIONS`, 10-10: 아무것도 없는 윈도우 · 맥에서 CamillaGUI 로 소리 날 때까지)도 같은 모양 — 그림 12장 `cam_*` 은 `tools/make_camilla_help_shots.py`(QPainter, 순서표 그림은 진짜 `export_camilladsp.howto_lines` 출력). 두 탭은 `tut.PICTURE_TABS` 로 묶여 `help_dialog._picture_page` 가 그린다. 버튼·칸의 뜻은 풍선 도움말(`tooltips.py`)이 정본이다(옛 기능 설명표 `help_text.py` 는 10-10 에 지웠다). 화면에는 설명 문구를 깔지 않는다(09-22 사용자 지시). |
| `../proc.py` | 외부 프로그램(REW) 실행 도우미 — exe 의 DLL 폴더·PATH·작업 폴더를 물려주지 않는다(09-23). |
| `splash.py` | 로딩 창. `__main__` 이 무거운 임포트 전에 띄우고 `run_wizard` 가 창이 뜬 뒤 닫는다. |
| `loading.py` | **로딩 오버레이**(09-23): 긴 일(5단계 mdat·샘플 불러오기) 동안 단계 화면을 덮는 막 + 가운데 카드(문구·경과·예상·[취소]). 300 ms 넘게 돌 때만 보인다(`SHOW_DELAY_MS`). 창의 `_poll_busy`(250 ms)가 `_overlay_jobs` 를 보고 `want()`/`clear()` 하고, 지난번 300 ms 넘게 걸린 단계로 이동할 때는 `show_now()` 로 먼저 그린다. 로고 방울 애니메이션(스프라이트 24장, `tools/make_logo_b8.py --frames` 로 다시 뽑음) — `LoadingIndicator(size=)` 는 크기 인자(기본 160)이고 작은 크기는 물리 px 로 미리 줄인 프레임을 캐시해 선명하게 그린다. **`BusyDrop`**(09-24, 기본 48 px, `set_busy()` 도 300 ms 규칙)은 화면을 덮지 않는 대기용: 구석 `busy.BusyPanel`(측정·보정 설계 계산·MIMO·XO 추천·파일 쓰기) 왼쪽, 1단계 상태 배지 옆(32 px), 5단계 경과 줄 옆 — 단계 화면의 `working()`/`poll_busy()` 를 창의 `_poll_busy` 가 부른다(`window._drop_pages`). 무거운 일은 워커 스레드여야 오버레이·방울이 움직인다 — 동기 호출 모양 그대로 워커에서 돌리는 `run_off_thread()`(5단계 mdat·샘플 채우기). 회귀 `tests/test_busy_drop.py`. |
| `rew_cache.py` | **화면 스레드는 REW 에 묻지 않는다**(09-23): `RewCache` 가 REW 상태·장치/채널 목록·캘리브레이션을 `RewSnapshot` 으로 들고 단계 화면은 REW 객체 자리에 이것을 받아 캐시 값만 읽는다. 새로 읽기는 워커 스레드(창의 백그라운드 점검 `watch_tick`, 유효 시간 15 s, [다시 확인]·[설정 읽기])만, 끝나면 `updated` 신호. REW 로 보내는 명령(레벨 모니터·제너레이터)은 `SerialWorker`. FakeREW 처럼 `probe()` 가 없는 흉내는 그대로 통과. 느린 REW 흉내 측정은 `tools\slow_rew_profile.py`, 회귀 테스트 `tests/test_rew_gui_thread.py`. |
| `wheel_guard.py` | 앱 전체 이벤트 필터. 닫힌 드롭다운 위 마우스 휠은 선택을 바꾸지 않고 감싼 스크롤 영역으로 넘긴다(09-23). `run_wizard` 가 설치. |


## 4. 데이터가 흐르는 순서 (단계 ↔ 코드)
1. **장치**(1) `rewapi` 로 REW 연결·Pro 확인·입출력 장치·스윕 길이 → `project.extra['devices']`. 화면은 `RewCache` 만 읽는다.
2. **구성**(2) `Project.config` ← `step2_config`. 출력 번호·대역·극성(출력 이름은 1단계에서 받은 REW 풀 네임).
3. **레벨**(3) `LevelMonitor` 로 채널별 입력 레벨.
4. **배치**(4) 마이크 위치 묶음(9/13/17) → `project.extra['array']`, 타이밍 레퍼런스 → `project.extra['devices']` 의 타이밍 키.
5. **측정**(5) `Measurer.run(plan)` → `session` 이 `project.measurements` 와 `.irs/*.wav` 기록. 또는 mdat 불러오기.
6. **보정 설계**(진행바 6번째, 화면 번호 9 — 2026-09-26 흐름) `secs.calibration.compute(project, irs, settings)` → `CalibrationRun`(새 엔진 `CorrectionEngine`), [MIMO 계산] → `MimoRun`. 설정만 `project.extra['calibration']` 에, 결과는 세션에만. 이 화면에서도 내보낸다(REW 단계별 mdat · 카밀라 설정).
7. **내보내기**(화면 번호 8, 2026-09-26 새로) 보정 설계 결과 → `calibration.export_before_after_mdat` → 보정 전·후 REW mdat 하나(`step8_mdat`).

## 5. 고칠 때 지킬 것
- **게이트**: `QT_QPA_PLATFORM=windows` 로 `.venv-secs\Scripts\python.exe -m pytest tests -q`(1.41 골든 대조는 10-10 에 폐지 — 새 엔진은 보정 설계 테스트가 지킨다).
- **화면을 건드렸으면** 실사용 QA: `tools\qa_capture.py --preset 2.1`, `--preset 9.1.6 --biamp`, 각각 `--lang en` 도. 가로 스크롤·잘림·남은 한글 0.
- **새 문구** = 한국어 원문 + 영어 쌍 등록.
- **주석은 '왜'를 적는다**: 무엇을 하는지는 코드가 말한다. 실측·QA 에서 배운 이유(날짜 포함)를 그 줄 옆에 남긴다. 단계가 끝나면 "P4 에서 구현" 같은 과거 시점의 말은 지운다. 모듈 독스트링 첫 문단에는 **무엇을 하는 모듈인지 · 누가 부르는지 · 입력과 출력**을 적는다.
