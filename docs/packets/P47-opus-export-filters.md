# P47 (Opus 5.5): 내보내기 화면의 필터 텍스트와 Equalizer APO

업스트림 SECS 2 Beta 1.3.0(이슈 #35)은 내보내기 화면에 '필터 내보내기' 카드를 더했습니다. CamillaDSP · 텍스트 · EqAPO 세 단추, 막힌 단추의 이유 한 줄, 결과 한 줄입니다(`src/secs/wizard/step8_mdat.py`의 `_filter_card`·`blocks`·`_sync_buttons`, 문구는 `src/secs/wizard/en_g8.py`·`tooltips.py`의 `cal.export_text`·`cal.export_eqapo`). 이 작업서는 그것을 우리 내보내기 화면에 들입니다. 서비스 연결(P46)과 모의 서비스·문장 다듬기(P48)는 따로 갑니다.

## 작업 위치

- 워크트리 `/home/user/wt/p47`(브랜치 `p47`)에서만 일합니다. 다른 경로는 읽기만 합니다. 같은 시각에 Daybreak가 `/home/user/wt/p45`에서 크레이트를 만듭니다.
- 커밋하지 않습니다. git으로 인덱스나 작업 트리를 바꾸지 않습니다. 화면 의존성은 이미 설치되어 있습니다(Node 24: `export PATH=/opt/nvm/versions/node/v24.22.0/bin:$PATH`).
- 먼저 `CLAUDE.md`, `docs/DESIGN.md`, `docs/UI-TERMS.md`, `src/screens/export/` 전체, `src/ui/`의 부품(`Card`, `Button`, `Advanced`, `StatusPill`, `ErrorNotice`, `Icon`)을 읽으세요. 토큰은 `src/styles/tokens.css`가 정본이고 구성 요소 CSS에 색 값·글자 크기 값을 직접 쓰지 않습니다(`styles/tokens.test.ts`가 검사).

## 이미 있는 것 (기반 커밋 `d11b5f0`)

- `ExportKind`에 `filter_text`·`equalizer_apo`가 있고(`src/ipc/bindings/ExportKind.ts`), 막힌 이유 `ExportBlock`(`"subs" | "mimo"`)과 오류 코드 `export_scope`가 있습니다.
- 범위 규칙은 서비스 쪽 `crates/secs-types/src/ipc.rs`의 `filter_export_block`이 정본입니다: 텍스트는 서브가 있으면 `subs`, 아니면 MIMO가 있으면 `mimo`. EqAPO는 MIMO가 있으면 `mimo`. CamillaDSP와 mdat은 막히지 않습니다.
- `ExportScreen.tsx`의 `KIND_TEXT`에 두 종류의 제목·본문·실행·진행 문구 키가 있고, `whereText`가 두 종류의 폴더를 말합니다. 화면에는 아직 그리지 않습니다. 카탈로그의 그 문장들은 초안입니다.

## 할 일

### 1. 범위 규칙 (`screens/export/types.ts`)

`exportBlock(view, kind): ExportBlock | null`을 더합니다. 서브가 있음 = `view.results.calibration.chain.subs.length > 0`, MIMO가 있음 = `mimoInclusion(view).included`(서비스가 내보내기에 MIMO를 넣는 조건을 이미 따르는 함수). 주석에 서비스의 `filter_export_block`을 따른다고 적습니다. 결과가 없으면 null. `EXPORT_KINDS`를 화면에 나오는 순서로 고칩니다: `camilla_dsp`, `filter_text`, `equalizer_apo`, `before_after_mdat`, `staged_mdat`. 테스트(`types.test.ts` 새로): 네 조합(서브 유무 × MIMO 유무)마다 다섯 종류의 판정.

### 2. 내보낼 파일 카드 (`ExportScreen.tsx`, `export.css`)

이 단계가 무엇을 위한 것인지: 계산한 보정을 DSP와 REW가 읽을 파일로 씁니다. 이제 DSP 형식이 셋이라, 카드 안을 두 묶음으로 나눕니다.

- **DSP에 넣을 필터** (`export.group.filters`): CamillaDSP 설정(지금처럼 머리글의 주 동작이 실행하고, 줄에는 진행 표시와 결과), 필터 값 텍스트(`export.run.filter_text`), Equalizer APO 설정(`export.run.equalizer_apo`).
- **REW에서 볼 응답** (`export.group.rew`): 보정 전후 mdat, 그 아래 고급의 단계별 mdat(지금 그대로).

묶음 이름은 카드 제목보다 한 단계 아래의 제목으로, 줄의 제목보다 한 단계 위로 읽히게 합니다(제목 단계와 `aria-labelledby`를 맞추고, 시각적으로는 작은 묶음 머리글). 묶음 사이 간격은 기존 `export-items`의 리듬을 따릅니다.

**막힌 줄**: `exportBlock`이 null이 아니면 실행 단추를 끄고, 줄 안에 이유 한 줄(`data-ui="export.kind.<kind>.blocked"`)을 둡니다. 오류가 아니라 형식의 범위이므로 경고 색이 아니라 정보 아이콘과 보조 글자 색으로 그립니다. 문장은 원인과 다음 행동을 함께 말합니다(합니다체). 키:

| 키 | 초안(ko) |
| --- | --- |
| `export.blocked.filter_text.subs` | 서브(BA)가 있는 결과는 텍스트로 낼 수 없습니다. 텍스트는 RC만 적용한 결과에서 냅니다. |
| `export.blocked.filter_text.mimo` | MIMO를 넣은 결과는 텍스트로 낼 수 없습니다. 보정에서 MIMO를 끄면 낼 수 있습니다. |
| `export.blocked.equalizer_apo.mimo` | MIMO를 넣은 결과는 Equalizer APO로 낼 수 없습니다. 보정에서 MIMO를 끄면 낼 수 있습니다. |

**EqAPO 결과 줄**: 저장이 끝나면 지금의 결과 줄(파일 수 → 폴더, 폴더 열기) 아래에 적용 안내 한 줄을 더합니다(`export.outcome.equalizer_apo.hint`): "config.txt와 secs_{mode} 폴더를 Equalizer APO의 config 폴더에 복사합니다. 순서와 채널 번호는 같은 폴더의 'EqAPO 적용 방법.txt'에 있습니다." `{mode}`는 필터 파일 이름의 모드입니다: `view.settings.mode`, 초고지연이면 업스트림 `_filter_plan`처럼 `ultra` 뒤에 ms를 붙인 이름(`ultra200`, ms는 파이썬 `:g` 꼴). 이 이름을 만드는 함수를 `types.ts`에 하나 둡니다. 폴더 이름(`whereText`)은 지금처럼 초고지연에도 `ultra`만 씁니다(업스트림 `SECS_{kind}_{what}_{settings.mode}_…`).

**CamillaDSP 본문**: 업스트림이 툴팁에 'RC · BA · MIMO 모두'를 더했습니다. `export.kind.camilla_dsp.body`의 문장 끝에 그 뜻을 더합니다(초안: "RC, BA, MIMO를 모두 담습니다.").

진행 중 규칙은 지금과 같습니다: 내보내기 잡은 하나씩 돌고, 도는 동안 다른 실행 단추는 꺼지며(`canExport`), 도는 줄은 바쁨 표시. 튜토리얼에서는 지금처럼 기록만 남습니다.

### 3. 견본과 고정값 (`screens/export/specimens.tsx`, `fixtures.ts`)

- 서브가 있는 결과(지금 `exportReady`): 텍스트가 막힌 줄.
- RC만 있는 결과(서브 없음): 세 형식 모두 켜짐.
- MIMO를 넣은 결과: 텍스트와 EqAPO가 막힌 줄.
- EqAPO 저장 완료: 결과 줄과 적용 안내. 파일 목록은 서비스가 쓰는 꼴(`<폴더>/config.txt`, `<폴더>/secs_mixed/secs_mixed.txt`, `<폴더>/EqAPO 적용 방법.txt`, `<폴더>/secs_mixed/secs_mixed_L_fir.wav` …)로 둡니다.

### 4. 화면 투어 (`src/gallery/tour/scenes.ts`, 내보내기 장면만)

내보낼 파일 카드가 창 아래로 내려가면 `reveal`로 보이게 합니다. 장면을 더합니다(id는 `export.` 접두):

- `export.filters`: 샘플(서브 있음) 결과에서 필터 묶음 — 텍스트가 막힌 이유가 보임.
- `export.eqapo-done`: EqAPO 저장을 누르고 끝낸 뒤의 결과 줄과 안내. 지금 모의 서비스는 새 종류에 자리표시 파일 이름을 돌려줍니다(P48이 서비스의 꼴로 고칩니다). 그대로 찍으세요.
- MIMO를 넣은 결과를 모의 서비스로 만들 수 있으면(`mimo` 잡, 위치 3곳 이상) `export.mimo-blocked`를 더하고, 못 만들면 보고서에 적습니다.

장면이 막히거나 기존 장면의 조작이 새 배치와 맞지 않으면 그 장면을 고칩니다.

### 5. 테스트 (`ExportScreen.test.tsx`)

새 줄과 단추가 그려짐, 단추가 `exportFiles("filter_text")`·`exportFiles("equalizer_apo")`를 부름, 막힌 줄에서 단추가 꺼지고 이유가 보임(세 키), MIMO 결과에서 두 줄이 막힘, EqAPO 결과 줄의 안내, 내보내는 동안 다른 단추가 꺼짐. 테스트의 `data-ui` 목록을 갱신합니다.

## 건드리면 안 되는 인터페이스 id

`export.run.camilla_dsp`(머리글 주 동작, 튜토리얼 퀘스트 `export.camilla`가 가리킴), `export.kind.<kind>`, `export.outcome.<kind>`, `export.outcome.<kind>.open`, `export.folder*`, `export.result*`, `export.advanced`, `export.go_calibrate`. 새 id는 위에 적은 것만 씁니다.

## 문구

새 키는 `src/i18n/ko.json`·`en.json`에 함께 넣습니다(합니다체, 상태와 라벨은 명사구). 영어 초안도 씁니다. 문장 다듬기는 Claude가 ASTRA에게 넘깁니다. 보고서에 새 키와 초안을 따로 적습니다.

## 고쳐도 되는 파일

`src/screens/export/`의 컴포넌트·CSS·`types.ts`·`fixtures.ts`·`specimens.tsx`와 그 테스트(새 `types.test.ts` 포함), `src/gallery/tour/scenes.ts`(내보내기 장면), `src/i18n/ko.json`·`en.json`(새 키와 `export.kind.camilla_dsp.body`).

건드리지 않음: `src/state/`, `src/screens/*/use*.ts`, `src/ipc/`, `src/app/`, `src/ui/`, `scripts/`, `crates/`, `.github/`, `docs/`.

## 검증 (모두 포그라운드로, 보고를 마친 뒤에만 턴을 끝냅니다)

```sh
export PATH=/opt/nvm/versions/node/v24.22.0/bin:$PATH
cd /home/user/wt/p47/apps/secs2r/ui
npm run typecheck && npm test && npm run build
npm run build:gallery
CHROMIUM_PATH=/opt/pw-browsers/chromium-1194/chrome-linux/chrome node scripts/gallery-shots.mjs --out /home/user/wt/p47-shots/specimens
CHROMIUM_PATH=/opt/pw-browsers/chromium-1194/chrome-linux/chrome node scripts/screen-tour.mjs --out /home/user/wt/p47-shots/screens --only export.
```

바뀐 견본과 내보내기 장면의 PNG를 Read 도구로 직접 열어 봅니다(밝게·어둡게, 한국어·영어 각 하나 이상).

## 보고 형식

1. 바꾼 파일과 각 한 줄 설명
2. 1–5번 각각: 무엇을 했는지, 정한 배치(묶음 제목 단계, 막힌 줄의 모양)
3. 새 카탈로그 키와 ko·en 초안
4. 검증 명령마다 마지막 몇 줄의 출력(테스트 수, 찍은 수)
5. 본 것: 작업 중 눈에 띈 다른 화면 결함(고치지 않음)
