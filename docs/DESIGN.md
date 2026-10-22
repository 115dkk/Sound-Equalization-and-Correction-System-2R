# SECS 2R 화면 설계

이 문서가 화면의 토큰과 규칙의 정본입니다. 값은 [`apps/secs2r/ui/src/styles/tokens.css`](../apps/secs2r/ui/src/styles/tokens.css)에 있고, 이 문서는 그 값이 무엇을 위한 것인지와 지켜야 할 규칙을 적습니다. 근거는 [ADR 0003](adr/0003-stack-and-ipc.md)의 화면 규칙과 make-interfaces-feel-better이고, 화면 지도는 [`M5.md`](M5.md)에 있습니다. 토큰을 바꾸면 이 문서와 `tokens.test.ts`를 함께 고칩니다.

## 1. 원칙

SECS 2R의 사용자는 그래프와 숫자를 읽는 애호가와 설치 기사입니다. 화면은 차분하고 정밀한 계측기처럼 보여야 합니다.

- **데이터가 있는 곳만 촘촘하게.** 그래프, 표, 배치도는 밀도를 높이고, 나머지는 여백과 중립색으로 조용히 둡니다.
- **화면마다 채운 기본 동작 하나.** 단계 머리의 주 동작만 강조색으로 채웁니다. 나머지 단추는 보조(secondary)이거나 조용한(quiet) 단추입니다.
- **강조색 하나.** 강조색은 '누를 수 있음'과 '지금 고른 것'을 뜻합니다. 정적인 글자에는 쓰지 않습니다.
- **글자는 14px 아래로 내리지 않습니다.** 캡션(축 눈금, 단위, 단축키 힌트, 메타데이터)만 13px입니다.
- **바뀌는 숫자는 `tabular-nums`.** 진행, 경과 시간, 레벨, 표의 숫자 칸이 해당합니다.
- **긴 계산은 화면을 막지 않습니다.** 잡은 작업 트레이 한 곳에 보이고, 취소하면 실제로 멈춥니다(U8).
- **문장은 합니다체, 상태와 라벨은 명사구.** 지시는 -세요로 씁니다. 오류는 원인과 다음 행동을 적고, 원문은 '자세히'에 둡니다.
- **설정은 설명 없이 읽힙니다.** 라벨·선택지·단위·현재값으로 선택할 수 있게 씁니다. 정적 보충 설명은 항목이나 묶음의 도움말로 열고, 필요한 경우에만 효과·선택 기준·손익을 1–2문장으로 설명합니다. 현재 상태와 추천값, 오류, 사용 불가 사유와 복구 행동은 화면에 유지합니다.

## 2. 색

### 2.1 역할 토큰

색은 역할 이름으로만 씁니다. 구성 요소의 CSS에는 색 값이 없습니다(`tokens.test.ts`가 확인). 값은 모두 OKLCH이고, 중립색은 강조색과 같은 색상 255–258의 차가운 기운 하나로 맞춥니다. 아래 표는 기본 스킨(Instrument)의 값이고, 스킨(2.5절)은 같은 이름을 덮어씁니다.

| 토큰 | 쓰임 | 밝게 | 어둡게 |
|---|---|---|---|
| `--bg` | 앱 바탕 | `oklch(0.968 0.004 255)` | `oklch(0.185 0.008 258)` |
| `--surface` | 카드, 그래프 바탕, 작업 트레이 | `oklch(0.993 0.002 255)` | `oklch(0.225 0.009 258)` |
| `--surface-sunken` | 표 묶음 줄, 미터 바탕, 분할 단추 바탕 | `oklch(0.952 0.005 255)` | `oklch(0.2 0.008 258)` |
| `--surface-raised` | 팝오버, 메뉴, 대화상자, 토스트 | `oklch(0.995 0.002 255)` | `oklch(0.26 0.01 258)` |
| `--surface-hover`, `--surface-pressed` | 줄과 조용한 단추의 hover, 눌림·비활성 바탕 | 0.935, 0.915 | 0.275, 0.3 |
| `--rail-bg` | 단계 레일 | `oklch(0.948 0.006 255)` | `oklch(0.2 0.008 258)` |
| `--image-outline` | 자기 타일이 있는 그림(로고)의 안쪽 1px 선 | `oklch(0 0 0 / 0.1)` | `oklch(1 0 0 / 0.1)` |
| `--text` | 본문 | `oklch(0.25 0.015 255)` | `oklch(0.93 0.006 258)` |
| `--text-muted` | 설명, 캡션, 축 글자 | `oklch(0.47 0.014 255)` | `oklch(0.75 0.012 258)` |
| `--text-faint` | 비활성, 힌트 | `oklch(0.62 0.01 255)` | `oklch(0.56 0.01 258)` |
| `--border` / `--border-subtle` | 구조 경계, 표의 가는 줄 | 0.88 / 0.92 | 0.33 / 0.29 |
| `--border-control` | 입력칸 테두리(3:1) | `oklch(0.62 0.01 255)` | `oklch(0.54 0.012 258)` |
| `--accent`, `--accent-hover`, `--accent-soft`, `--on-accent` | 주 동작, 고른 것, 초점 고리 | L 0.49 / 0.44 / 0.945 | L 0.76 / 0.81 / 0.31 |
| `--success`, `--warning`, `--danger`, `--info` (+ `-soft`, `--on-success`, `--on-danger`) | 상태 | 2.3절 | 2.3절 |

강조색은 `oklch(var(--accent-l) calc(var(--accent-chroma) * var(--theme-chroma)) var(--accent-hue))`로 조립합니다. 밝기는 테마가, 색상과 채도는 강조색 종류가 정합니다.

- 기본 강조색은 색상 258(파랑), 채도 0.115입니다(밝게 `#3460a1`, 어둡게 `#8cb3ed`). 2026-10-22의 색 감사([`skins/default.md`](skins/default.md))에서 업스트림의 청록(195)을 대신해 골랐습니다. 파랑은 모든 데스크톱에서 '누를 수 있음'의 색이라 배우지 않아도 읽히고, 상태색과 15° 이상 떨어집니다(info는 그래서 232입니다).
- 튜토리얼은 `data-accent="tutorial"`로 색상 345(자홍), 채도 0.165로 바꿉니다. 강조색만 바뀌고 나머지 토큰은 그대로입니다(업스트림 튜토리얼의 분홍 테마에 해당). Rack과 Matrix 스킨은 튜토리얼에서도 자기 강조색을 유지합니다.
- 어두운 테마는 채도를 0.82배로 낮춥니다. 어두운 바탕에서 같은 채도는 번져 보이기 때문입니다.

### 2.2 테마 전환

- 기본값은 운영체제를 따릅니다(`prefers-color-scheme`).
- `data-theme="light"|"dark"`를 `<html>`이나 어느 하위 요소에 걸어도 그 아래가 그 테마로 바뀝니다. 앱은 `<html>`에 걸고(`PrefsScope root`), 갤러리는 견본마다 걸 수 있습니다.
- 어두운 값은 두 벌(시스템 선호, 명시 속성)이 같아야 합니다. CSS에 혼합 문법이 없어 두 번 적고, `tokens.test.ts`가 두 벌을 비교합니다.
- 그래프는 테마·강조색·스킨이 바뀌면 토큰을 다시 읽어 인스턴스를 새로 만듭니다(새로 고침 없음, 6절). 그 감시는 그래프를 만드는 레이아웃 효과 안에서 붙이므로, 앱 틀이 같은 커밋의 효과에서 `<html>`에 속성을 걸어도 놓치지 않습니다.
- 스킨(`data-skin`)은 테마와 독립인 세 번째 축입니다(2.5절).

### 2.3 상태색의 뜻

상태색은 정해진 뜻으로만 씁니다. 색만으로 뜻을 전하지 않고 언제나 글자나 아이콘을 함께 둡니다.

| 색 | 뜻 | 예 |
|---|---|---|
| success | 끝남, 적정 | 완료한 단계, 측정한 위치, 레벨 적정, 연결됨 |
| warning | 확인이 필요함 | 다시 계산 필요, 레벨 높음, 막힌 단계의 이유, 낮은 S/N |
| danger | 실패, 데이터를 잃는 동작 | 잡 실패, 클리핑, 측정 지우기 |
| info | 진행 중 | 진행 막대, 진행 중 알약과 회전 표시 |

### 2.4 측정한 대비

WCAG 대비는 추정하지 않고 계산합니다. `styles/contrast.test.ts`가 토큰 파일을 읽어 OKLCH→sRGB(감마 매핑 포함)로 바꾸고 상대 휘도로 대비를 구하며, 글자 4.5:1, 입력칸 테두리·초점 고리·상태 글리프 3:1 아래면 실패합니다. 아래 표는 그 테스트가 출력한 값입니다(`CONTRAST_TABLE=1 npx vitest run src/styles/contrast.test.ts --reporter=verbose`). 스킨의 표는 같은 명령이 함께 출력합니다.

| 쌍 | default light | default dark | default light tutorial | default dark tutorial |
|---|---|---|---|---|
| `--text` / `--bg` | 14.58 | 15.17 | 14.58 | 15.17 |
| `--text` / `--surface` | 15.67 | 13.92 | 15.67 | 13.92 |
| `--text` / `--surface-sunken` | 13.91 | 14.73 | 13.91 | 14.73 |
| `--text` / `--surface-raised` | 15.77 | 12.64 | 15.77 | 12.64 |
| `--text` / `--rail-bg` | 13.74 | 14.73 | 13.74 | 14.73 |
| `--text-muted` / `--bg` | 6.22 | 8.38 | 6.22 | 8.38 |
| `--text-muted` / `--surface` | 6.68 | 7.69 | 6.68 | 7.69 |
| `--text-muted` / `--surface-sunken` | 5.93 | 8.14 | 5.93 | 8.14 |
| `--text-muted` / `--surface-raised` | 6.72 | 6.98 | 6.72 | 6.98 |
| `--text-muted` / `--rail-bg` | 5.86 | 8.14 | 5.86 | 8.14 |
| `--text-faint` / `--surface` | 3.57 | 3.68 | 3.57 | 3.68 |
| `--border-control` / `--surface` | 3.57 | 3.38 | 3.57 | 3.38 |
| `--border-control` / `--surface-sunken` | 3.16 | 3.58 | 3.16 | 3.58 |
| `--accent` / `--surface` | 6.19 | 7.96 | 6.74 | 7.48 |
| `--accent` / `--bg` | 5.75 | 8.68 | 6.27 | 8.15 |
| `--on-accent` / `--accent` | 6.13 | 8.75 | 6.69 | 8.23 |
| `--on-accent` / `--accent-hover` | 7.62 | 10.41 | 8.29 | 9.81 |
| `--success` / `--surface` | 5.56 | 8.35 | 5.56 | 8.35 |
| `--warning` / `--surface` | 5.57 | 9.06 | 5.57 | 9.06 |
| `--danger` / `--surface` | 5.89 | 6.49 | 5.89 | 6.49 |
| `--info` / `--surface` | 5.75 | 7.83 | 5.75 | 7.83 |
| `--success` / `--success-soft` | 5.04 | 6.77 | 5.04 | 6.77 |
| `--warning` / `--warning-soft` | 4.97 | 7.52 | 4.97 | 7.52 |
| `--danger` / `--danger-soft` | 5.23 | 5.62 | 5.23 | 5.62 |
| `--info` / `--info-soft` | 5.17 | 6.41 | 5.17 | 6.41 |
| `--on-danger` / `--danger` | 5.83 | 7.14 | 5.83 | 7.14 |
| `--on-success` / `--success` | 5.52 | 9.18 | 5.52 | 9.18 |
| `--plot-text` / `--plot-bg` | 6.68 | 7.69 | 6.68 | 7.69 |
| `--map-ink` / `--map-bg` | 10.64 | 10.82 | 10.64 | 10.82 |
| `--meter-fill` / `--meter-track` | 5.93 | 8.14 | 5.93 | 8.14 |

### 2.5 스킨

스킨은 `data-skin="rack|matrix"`로 고르는 세 번째 축입니다([ADR 0010](adr/0010-skin-sets.md)). 기본 스킨은 속성이 없는 상태(Instrument)이고, 각 스킨은 `styles/skins/<id>.css` 한 파일에서 위 토큰의 일부를 덮어쓴 뒤 구성 요소의 모양(모서리, 깊이, 질감, 상태 표현)을 바꿉니다. 철학과 구성 요소별 답은 [`skins/`](skins/README.md)의 헌법에 있고, 두 스킨이 팔레트만 다르면 실패라는 차별화 게이트가 그 문서에 있습니다. 앱은 고른 스킨을 `AppSettings.skin`에 저장하고(설정 팝오버의 '스킨'), 갤러리는 `skin=` 인자와 툴바로 고릅니다.

## 3. 그래프

### 3.1 채널 색

곡선 색은 채널을 따르고 순위를 따르지 않습니다. 프로젝트의 역할 순서대로 메인에 `--ch-1`…`--ch-16`, 서브에 `--sub-1`…`--sub-8`을 줍니다(`colorTokens`). 칩을 꺼도 남은 곡선의 색은 바뀌지 않습니다.

- **메인 16색.** 인접한 두 색이 색약 모의(Machado 2009)에서도 구별되도록 색상과 밝기를 번갈아 배치했습니다. dataviz 검증기 결과는 밝게: 인접 CVD ΔE 최소 14.1, 정상 시각 최소 26.2, 바탕 대비 모두 3:1 이상입니다. 어둡게: 11.8, 24.8, 모두 3:1 이상입니다.
- **서브 8색.** 메인보다 한 단계 진한(밝은 테마) 또는 밝은(어두운 테마) 별도 층이고, 선도 더 굵습니다. 그래서 검증기의 밝기 범위 밖에 일부러 둡니다. 서브끼리의 인접 CVD ΔE는 밝게 6.1, 어둡게 7.6으로 '보조 표시가 있어야 하는' 구간입니다. 범례 칩의 이름과 BA 표의 이름으로 보조합니다. 서브가 여덟 대인 구성은 드뭅니다.

### 3.2 선 규칙

| 곡선 | 굵기 | 불투명도 | 토큰 |
|---|---|---|---|
| 위치 1(청취 위치), 메인 | 2.25px | 1 | `--plot-w-pos1` |
| 다른 위치, 메인 | 1px | 0.38 | `--plot-w-other`, `--plot-alpha-other` |
| 서브 | 3px(다른 위치 1.5px) | 1(0.38) | `--plot-w-sub` |
| 칩에 커서를 댄 채널·위치 | +0.5px | 1 | |
| 그 밖의 곡선(강조 중) | 1px | 0.12 | `--plot-alpha-dim` |
| 타겟 | 2px, 점선 6 4 | 1 | `--plot-w-target`, `--plot-dash-target`, `--plot-target` |
| 보정 전 | 점선 2 3 | | `--plot-dash-before` |
| 낡은 결과 | | ×0.45 | |

### 3.3 축과 상호작용

- **주파수 축**은 로그이고 눈금은 5, 10, 20, 50, 100, 200, 500, 1k, 2k, 5k, 10k, 20k로 고정입니다(업스트림 `plot.py`). 기본 범위는 5–22000 Hz입니다.
- **축 범위 팝오버**는 주파수와 레벨의 하한·상한, 자동, 적용을 둡니다. 하한이 상한보다 낮지 않으면 "주파수 하한은 상한보다 낮아야 합니다."를 보이고 닫히지 않습니다. Enter는 적용입니다. 직접 정한 범위가 있으면 단추에 강조색 점이 붙습니다. `Graph`는 고른 범위를 스스로 쥐고, 화면이 실행 동안 남겨야 하면 `range`와 `onRangeChange`로 쥡니다(측정 그래프).
- **타겟 점**은 10–24000 Hz, ±30 dB 안에 있고 언제나 두 개 이상입니다. 끌기로 옮기고(이웃 점을 넘지 않음), 빈 곳을 두 번 누르면 더하고, 점에서 오른쪽 클릭하거나 Delete를 누르면 뺍니다. 점을 두 번 누르면 그 점에 머물고 점을 더하지 않습니다. 초점이 있는 점은 ←/→로 1/24옥타브(Shift 1/3옥타브), ↑/↓로 0.5 dB(Shift 3 dB) 움직입니다.
- **XO 선**은 40–200 Hz 안에서 5 Hz 단위로 끕니다. 손잡이는 `role="slider"`이고 화살표, PageUp/PageDown, Home/End가 움직입니다.
- **한 동작에 한 번 저장.** 타겟 점과 XO 선은 움직이는 동안 `onChange`로 값을 알리고(화면이 그 값을 쥐고 다시 넘김), 동작이 끝날 때 `onCommit`을 한 번 부릅니다: 끌고 놓았을 때, 키를 누르다 놓았을 때(누르고 있으면 반복), 초점이 떠날 때. 점 빼기는 그 자체로 한 동작이라 바로 저장합니다. 화면은 `onCommit`에서 저장합니다.
- **RC 대역**은 범위 밖을 `--plot-shade`로 칠하고 경계에 점선과 "RC 하한 20 Hz" 같은 라벨을 둡니다. 자동 상한을 아직 모르면(계산 전) 하한 아래만 칠하고 하한 선만 둡니다(`band.hi` 없음).
- **커서 값**은 그래프 위 왼쪽에 "1.25k Hz · −3.2 dB"처럼 보입니다.
- **그래프 위 줄**(단위, 커서 값, 축 범위 단추)은 작은 컨트롤 높이에 8px을 더한 띠(40px)이고, 단추는 캔버스에서 8px 떨어집니다. 스킨이 단추를 돌출 캡으로 그려도 유리의 베젤에 닿지 않게 하기 위한 것입니다.
- **축 숫자**는 캡션 크기(13px)이고 굵기는 `--plot-weight`(기본 400)를 따릅니다. 스킨이 어두운 유리 안에 숫자를 두면 이 토큰으로 굵게 합니다(Rack은 700).
- 끌 수 있는 것(타겟 점, XO 선)만 강조색입니다.

### 3.4 구현 메모

- uPlot 1.6.32(MIT)를 씁니다. 곡선 구조나 테마·강조색·스킨이 바뀔 때 인스턴스를 새로 만들고(축 글꼴까지 토큰에서 다시 읽음), 값은 `setData`, 강조·낡음은 선 굵기와 색 함수를 바꾼 뒤 `redraw`로 다시 칠합니다.
- 캔버스 색은 토큰을 읽어 `rgba()`로 바꿉니다. 빌드가 `oklch()`를 `lab()`으로 바꿔 쓰므로, hex·rgb·oklch는 직접 읽고 그 밖의 값은 1픽셀 캔버스에 칠해 sRGB로 읽습니다(`color.ts`).
- 곡선 57개 × 3000점(갤러리 `plot.dense`)에서 첫 그림 약 150 ms, 칩 강조와 켜고 끄기의 다시 그리기는 한 프레임 안이었습니다(헤드리스 Chromium, 소프트웨어 렌더링).
- uPlot의 로그 축 필터는 고정 눈금 일부를 지우므로 끕니다. `setScale`에 값을 직접 주면 스케일의 range 함수를 건너뛰므로, 레벨 범위는 계산한 값을 넘깁니다.

## 4. 글자

- **글꼴**: `--font-ui` = "Pretendard", "Apple SD Gothic Neo", "Malgun Gothic", "Noto Sans KR", system-ui, sans-serif. 웹 글꼴은 받지 않습니다(오프라인, CSP). 코드와 원문 오류에는 `--font-mono`를 쓰고, 여기에도 한글 대체 글꼴을 둡니다.
- **크기**

| 토큰 | 크기 | 쓰임 |
|---|---|---|
| `--text-caption` | 13px | 축 눈금, 단위, 단축키 힌트, 시각, 레일의 막힌 이유, 상태 요약의 항목 이름 |
| `--text-ui` | 14px | 라벨, 단추, 메뉴, 표, 설명, 오류, 툴팁(기본 글자) |
| `--text-body` | 16px | 도움말 같은 긴 글 |
| `--text-title-s` | 16px | 카드·대화상자 제목 |
| `--text-title` | 20px | 단계 제목 |
| `--text-display` | 24px | 카드마다 큰 숫자 하나까지. 자릿수만 크고 단위는 `--text-title-s`로 옆에 둡니다(보정 결과의 ±1 dB 안 비율) |

- 모든 `font-size`는 이 토큰을 거칩니다. 14px(캡션 13px) 아래는 `tokens.test.ts`가 막습니다. 글자가 넘치면 줄이지 않고 문장을 줄이거나 자리를 넓힙니다.
- 굵기는 400, 500, 600, 700만 씁니다. 18px 아래에서 400보다 가늘게 쓰지 않습니다.
- 줄바꿈은 `word-break: keep-all`(한국어 어절 단위)과 `overflow-wrap: anywhere`(긴 경로)입니다. 제목은 `text-wrap: balance`, 문단은 `pretty`입니다.
- macOS 글자 번짐을 줄이려고 `<html>`에 `-webkit-font-smoothing: antialiased`를 겁니다.

## 5. 간격, 모양, 깊이

- **간격**은 4px 격자입니다: `--space-1`(4) … `--space-12`(48). 사이 값 `--space-0-5`(2), `--space-1-5`(6), `--space-2-5`(10)를 둡니다.
- **크기**: 입력칸과 단추 36px(`--control-h`), 작은 단추와 칩 32px(`--control-h-sm`). 누르는 영역은 40px 이상이고, 32px 아이콘 단추는 의사 요소로 40px까지 넓힙니다. 레일 폭 252px, 툴팁 최대 폭 420px.
- **반지름**: 4(칩, 미터), 6(단추, 입력칸), 10(카드, 팝오버), 14(대화상자). 24px보다 가깝게 겹친 면은 바깥 = 안쪽 + 간격입니다. 메뉴(10)와 항목(6)의 간격은 4px이고, 분할 단추(8)와 조각(6)의 간격은 2px입니다.
- **깊이**는 그림자, **구조**는 선입니다. 카드와 보조 단추는 투명 그림자 고리(`--shadow-card`, `--shadow-control`), 팝오버·대화상자는 `--shadow-raised`, `--shadow-dialog`입니다. 표의 줄, 레일 경계, 입력칸 테두리는 선입니다. 어두운 테마의 그림자는 흰 1px 고리 하나로 줄입니다.
- **초점**은 키보드 초점에서 늘 보입니다: 강조색 2px 외곽선, 2px 띄움. 입력칸은 칸 전체에 고리를 겁니다.

## 6. 움직임

- 시간은 `--dur-fast` 100 ms(hover 색), `--dur-base` 150 ms(토글, 펼침), `--dur-slow` 220 ms(대화상자, 토스트 등장)이고 곡선은 `--ease-out`(cubic-bezier(0.2, 0, 0, 1))입니다.
- 상호작용 상태는 CSS 전환으로 바꿔 중간에 끊을 수 있습니다. 키프레임은 회전 표시, 불확정 진행 막대, 한 번의 등장에만 씁니다.
- 누르면 `scale: 0.96`(`--press-scale`)입니다. 빠르게 거듭 누르는 단추는 `static`으로 끕니다.
- `transition: all`은 쓰지 않습니다(테스트가 확인).
- `prefers-reduced-motion`이면 시간이 0이 되고 누름 크기 변화도 없습니다. 회전 표시는 멈추고 불확정 막대는 정지한 띠가 되며, 옆의 글자("진행 중")가 상태를 전합니다. 움직임만으로 상태를 전하지 않습니다.

## 7. 앱 틀

```text
┌ 레일 252px ───────────┬ 단계 ─────────────────────────────────────────┐
│ [L] SECS 2R 사전 출시판│ 단계 제목  [상태 알약]          [주 동작]     │
│ [프로젝트 ▾ 파일 이름] │ 막힌 이유와 다음 행동                         │
│ ① 시스템          ✓   ├───────────────────────────────────────────────┤
│ ② 연결·점검       ✓   │                                               │
│ ③ 측정·가져오기   ●   │ 단계 화면(P30–P32)                             │
│ ⓛ 보정  측정 필요      │                                               │
│ ⓛ 내보내기 결과 필요   │                                               │
│ 프로젝트 상태           ├───────────────────────────────────────────────┤
│  구성 · 측정 위치 ·    │ 작업 트레이: 진행 중인 잡, 기록                 │
│  보정 · 측정 경로 · 파일│                                               │
│ 설정 · 도움말 F1       │                                               │
└───────────────────────┴───────────────────────────────────────────────┘
```

- **로고**(그림의 [L]): 레일 맨 위, 이름과 판 줄 왼쪽의 36px 마크입니다. 업스트림 사이드바처럼 로고 자신의 짙은 타일이 두 테마 모두에서 색을 지니고, `--image-outline` 1px 선이 어두운 레일에서 타일 가장자리를 남깁니다. 설정에서 고른 로고를 따릅니다(편차 U12). '전문가 모드'는 업스트림 트리의 SVG를 그대로 읽고, '킹반인 모드'는 `apps/secs2r/icons/logo/civilian.svg`입니다(`app/logo.ts`). 킹반인 판은 같은 짙은 청록 타일에 측정 응답(분홍)과 목표선 위의 보정 응답(청록), PEQ 손잡이를 그리고, 위쪽 광원·아래쪽 비네트·선 그림자로 깊이를 줍니다. Qt SVG로도 똑같이 그려지도록 필터 없이 그라디언트와 투명도만 씁니다. 32px 이하로 그릴 때는 두 로고 모두 작은 크기용 단순판을 씁니다.
- **레일**: 다섯 단계와 상태(완료 ✓, 지금 단계, 막힘(자물쇠 아이콘, 그림의 ⓛ)과 짧은 이유, 건너뜀). 막힌 단계도 열어서 무엇이 필요한지 볼 수 있습니다. 건너뛴 단계(mdat 가져오기 경로의 연결·점검)는 열리지 않습니다. 막힌 이유의 전체 문장은 툴팁과 단계 머리에 있습니다.
- **단계 머리**: 제목, 해당 단계의 도움말, 주 동작 하나. 목적 설명은 제목 옆 도움말로 엽니다. 막혔으면 이유와 갈 곳을 화면에 적고 주 동작을 끕니다(`aria-describedby`로 이유를 잇습니다). 화면이 주 동작을 끌 수도 있습니다(`PrimaryAction.disabled`, 같은 화면의 다른 잡이 돌 때). 이때는 제목 옆 상태 알약이 이유이고 `aria-describedby`가 알약을 가리킵니다. 진행 중(`busy`)은 주 동작 자신의 잡에만 씁니다.
- **곁 패널 자리**(`AppFrame aside`): 단계 머리와 작업 트레이 사이의 상자(`.app-body`)가 패널의 기준 상자입니다. 도움말 패널은 그 오른쪽에 놓여 주 동작과 트레이의 취소를 가리지 않고, 트레이 기록을 펴면 함께 줄어듭니다.
- **프로젝트 상태**: 구성("7.1.4 · 서브 2 · 출력 13"), 측정 위치(k/N), 보정(결과 없음, 계산 중, 완료, 다시 계산 필요, 실패), 측정 경로(REW·자체 측정·모의 측정과 연결 상태), 파일(이름 또는 저장 전, 미저장 변경 사항).
- **프로젝트 메뉴**: 새로 만들기, 열기…, 저장, 다른 이름으로 저장…. **설정**: 언어, 테마(시스템·밝게·어둡게), 스킨(기본·하드웨어 랙·시그널 매트릭스), 로고. 세부 항목은 해당 기능에서 펼치며 전역 숙련도 설정은 두지 않습니다(P58).
- **단축키**는 `app/shortcuts.ts` 한 곳에 있고 메뉴의 힌트도 여기서 읽습니다. macOS에서는 Ctrl 대신 ⌘입니다. 한글 자판에서도 물리 키(`code`)로 맞춥니다.

| 키 | 동작 |
|---|---|
| Alt+← / Alt+→ | 이전·다음 단계(건너뛴 단계는 지나침) |
| Ctrl+1…5 | 해당 단계 |
| Ctrl+S / Ctrl+Shift+S | 저장 / 다른 이름으로 저장 |
| Ctrl+O | 열기 |
| F1 | 도움말 |

- **작업 트레이**(U8, `app/jobs/`): 업스트림의 작업 패널, 로딩 카드, 물방울을 하나로 모읍니다. 잡마다 이름, 단계, 진행(완료/전체 또는 불확정), 경과, 속도를 알게 된 뒤의 남은 시간, 실제로 멈추는 취소가 있습니다. 기록에는 `Log` 알림, 완료, 취소, 오류(원인과 다음 행동, '자세히'의 원문)가 쌓입니다. 서비스가 밀어 보내는 `JobEvent`(Started, Progress, Log, Finished)와 처음 한 번의 `jobs` 응답만으로 그리고 폴링하지 않습니다. 남은 시간은 지금 단계의 진행 속도로만 셉니다.

## 8. 구성 요소

`src/ui/`에 있고 `src/ui/index.ts`로 가져옵니다. 모든 대화형 요소에는 튜토리얼과 도달표가 가리킬 `id` 또는 `data-ui`가 있습니다.

| 구성 요소 | 상태 | 접근성 |
|---|---|---|
| `Button`, `IconButton` | primary, secondary, quiet, danger; md, sm; 아이콘; 진행 중(회전 표시, 라벨 유지); 비활성 | 기본 `type="button"`, 진행 중 `aria-busy`, 아이콘 단추는 `aria-label` |
| `Card`, `Section`, `Advanced` | 제목·도움말·현재 상태·동작; 낡음(왼쪽 경고색 띠); 촘촘함; '고급' 접힘·펼침(접힌 항목 수) | `section` + 제목, 펼침 단추 `aria-expanded`, 접힌 내용은 없앰 |
| `StatusPill` | neutral, success, warning, danger, info, busy | 채운 점 + 명사구 |
| `NumberField` | 단위, 범위, 단계, 잘못된 값(범위를 문장으로), 비활성, 범위 표시 | `role="spinbutton"`, `aria-valuemin/max/now`, `aria-invalid`, 오류를 `aria-describedby`로 연결. ↑/↓ 단계(Shift ×10), Enter 확정, Esc 되돌림 |
| `Select` | 보통, 오류, 비활성; 라벨 왼쪽(`inline`, 그래프 막대 같은 좁은 줄) | 기본 `select`, 라벨 연결 |
| `CellSelect` | 보통, 오류, 비활성; 닫힌 상태에 굵은 코드와 흐린 이름 같은 풍부한 표시 | 표 칸용, 열 머리가 보이는 라벨이고 `aria-label`로 이름을 줌, 오류 설명을 `aria-describedby`로 연결. 상자는 32px, 누름 영역은 40px |
| `Stepper` | 하한·상한에서 단추 끔 | `role="spinbutton"` 탭 하나, ↑/↓/Home/End |
| `Toggle` | 켬, 끔, 비활성, 설명 | `role="switch"`, 라벨은 켠 상태를 말함 |
| `Segmented` | 고름, 비활성, 작음 | `radiogroup`, 탭 하나, 화살표로 이동하며 고름 |
| `DataTable` | 묶음 머리줄, 숫자 칸, 고른 줄, 빈 표 | `caption`, `scope`, 고를 수 있는 줄은 Enter·Space, `aria-current` |
| `LevelMeter` | 측정 전, 낮음, 적정(−30…−15 띠), 높음, 클리핑; 최고값 표시 | `role="meter"`, 값과 상태를 `aria-valuetext`로 |
| `ProgressBar` | 확정, 불확정 | `role="progressbar"` |
| `ToastProvider`, `useToast` | neutral, success, warning, danger; 동작 하나(되돌리기) | `aria-live="polite"`, 커서·초점이 있는 동안 멈춤 |
| `Dialog`, `ConfirmDialog`, `useConfirmAndUndo` | 확인, 위험 동작 | `role="dialog"`, 초점 가둠, Esc와 닫기 단추, 연 단추로 초점 복귀 |
| `Tooltip`, `HelpTip` | hover 400 ms 뒤 열림, 키보드 초점은 즉시, 도움말 클릭·터치로 고정/해제, 폭 420px 및 창 안으로 제한 | `role="tooltip"`, `aria-describedby`, 도움말 단추는 항목 이름과 연결, 설명 위 hover 유지, Esc는 도움말부터 닫음 |
| `Popover`, `Menu` | 열림, 닫힘; 메뉴 항목의 고름(`checked`, 강조색 확인 표시), 상태만 알리는 고름(`checked` + `disabled`, 글자색 유지), 구분선(`separator`) | `dialog` / `menu`, 고름은 `menuitemradio` + `aria-checked`, 화살표·Home·End(끈 항목은 건너뜀), Esc와 바깥 클릭으로 닫힘 |
| `EmptyState`, `ErrorNotice` | 빈 곳(이름, 채우는 법, 동작 하나); 오류·경고(원인, 다음 행동, 자세히) | 오류는 `role="alert"` |
| `LogPlot`, `Graph`, `Legend`, `AxisRangeButton`, `TargetHandles`, `XoLine` | 3절 | 그래프는 `figure` + 요약 문장, 칩은 `aria-pressed`, 타겟 점과 XO 손잡이는 키보드로 움직임 |
| `RoomMap`, `ArrangementMap`, `ArrangementThumb` | 9절 | `listbox` 탭 하나 + `aria-activedescendant` |

**지우기 확인과 되돌리기(U5).** 측정을 지우는 동작은 `useConfirmAndUndo`를 씁니다. 제목은 동작의 이름("위치 3 측정 지우기"), 본문은 결과 한 줄, 단추는 동작을 되풀이합니다("측정 지우기" / "취소"). 지운 뒤 토스트가 "되돌리기"를 주고, 서비스의 `position_clear`가 돌려준 되돌리기 토큰을 `undo`에 넘깁니다.

## 9. 배치도

- 손으로 그린 SVG이고 라이브러리를 쓰지 않습니다. 좌표와 모양은 업스트림 `layout_map.py`와 `model.py`에서 옮긴 고정값(`ui/maps/geometry.ts`)이고, 서비스가 배치를 보내게 되면(P26 `layout_get`) props로 받습니다.
- **방 배치도**(420px): 스크린, 청취 위치, 스피커 표식(메인 채운 원, 탑·하이트 점선 원, 서브 사각형)과 코드·출력. 방 안 자리가 없는 역할은 아래 '역할 미지정' 줄에 단추로 둡니다.
- **측정 배치도**(360 × 300px): 스크린, 의자 또는 소파(13곳부터), 머리, 지름 1 m 원, 30 cm 축척, 번호 표식. 위치 1은 크고, ±20 cm 높이는 반만 채우며, 완료는 success, 지금 위치는 강조색입니다. ←/↑는 이전, →/↓는 다음, 고른 위치가 없으면 → 는 1, ←는 마지막입니다(업스트림과 같음). Home/End도 됩니다.
- **스크린**은 지도 위쪽의 둥근 띠(`map__screen`)와 캡션(`map__caption--screen`)입니다. 같은 뜻의 글 없는 기호 `map__front`(스크린을 가리키는 화살표: `map__front-shaft`와 `map__front-head`)도 함께 그려 두고 기본 스킨은 숨깁니다. Rack·Matrix는 캡션과 좌석 라벨(`map__caption--seat`)을 숨기고 기호를 보입니다. 축척 라벨(`map__caption--scale`)은 어느 스킨에서도 남습니다.
- **작은 배치도**는 배치 카드의 장식이고 `aria-hidden`입니다.
- 배치도는 제 크기로 그립니다. 좁은 곳에서는 줄이지 않고 스크롤합니다. 줄이면 SVG 글자가 13px 아래로 내려가기 때문입니다. 범례는 SVG 밖의 HTML 글자입니다.

## 10. 문구

- 화면의 모든 글자는 `src/i18n/ko.json`·`en.json`의 키를 거칩니다. 두 파일의 키 집합과 자리표시자가 같아야 하고(`i18n.test.ts`), 한국어가 원문입니다. P34에서 ASTRA가 다듬습니다.
- 키는 도달표(`docs/M5-reach.md`)가 인용할 안정된 id입니다: `frame.*`(틀), `jobs.*`(작업 트레이), `error.<code>`·`error.<code>.next`(오류 코드의 원인과 다음 행동), `ui.*`(구성 요소), `plot.*`, `map.*`, `shortcut.*`.
- 서비스의 코드는 타입 있는 표로 키에 잇습니다: `JOB_NAME`, `JOB_STATUS`, `ERROR_TEXT`(`ErrorCode` 17개 모두), 단계 `jobs.stage.<stage>`, 알림 `jobs.log.<code>`. 모르는 알림 코드는 "알 수 없는 알림"과 '자세히'의 원문으로 보입니다.
- `gallery.sample.*`는 갤러리 견본에만 쓰는 문구입니다. 앱은 쓰지 않지만 갤러리가 쓰므로 '쓰이지 않는 키 없음' 검사(`i18n.test.ts`)는 이 키들을 쓰인 것으로 셉니다. 화면 용어는 [`UI-TERMS.md`](UI-TERMS.md)를 따릅니다.
- 문장은 합니다체이고 지시는 -세요입니다. 해요체 문장과 느낌표가 없음을 `i18n.test.ts`가 확인합니다. 상태와 라벨은 명사구("다시 계산 필요", "미저장 변경 사항")입니다.

## 11. 안정 id

튜토리얼(P32)과 도달표는 화면 요소를 `data-ui`(또는 같은 값의 `id`)로 가리킵니다.

| 자리 | id |
|---|---|
| 틀 | `frame`, `frame.logo`, `frame.rail`, `frame.rail.<step>`, `frame.header`, `frame.header.blocked`, `frame.content`, `frame.status`, `frame.status.<config\|positions\|calibration\|path\|file>` |
| 메뉴·설정 | `frame.project`, `frame.project.<new\|open\|save\|save_as>`, `frame.settings`, `frame.settings.<language\|theme\|skin\|logo>`, `frame.help` |
| 주 동작 | 화면이 정합니다(예: `measure.position.start`). 갤러리는 `<step>.primary` |
| 작업 트레이 | `jobs.tray`, `jobs.job.<kind>`, `jobs.cancel.<kind>`, `jobs.history`, `jobs.history.toggle`, `jobs.history.clear` |
| 그래프 | `<id>`, `<id>.axes`, `<id>.legend.role.<role>`, `<id>.legend.position.<n>`, `<id>.point.<n>` |
| 배치도 | `<id>.speaker.<role>`, `<id>.unassigned.<n>`, `<id>.position.<n>` |

단계 id는 `system`, `connect`, `measure`, `calibrate`, `export`입니다.

## 12. 갤러리와 모의 서비스

- `gallery.html`은 두 번째 Vite 입구입니다. `vite build`는 앱과 갤러리를 함께 짓고, `npm run build`(Tauri가 부름)는 `--mode app`으로 앱만 지어 갤러리와 고정값이 설치본에 들어가지 않습니다. 갤러리만 지으려면 `npm run build:gallery`입니다.
- 주소 인자: `gallery.html?theme=light|dark&lang=ko|en&accent=default|tutorial&skin=default|rack|matrix&only=<id 접두어,…>`. 견본마다 `data-gallery-id`가 있어 CI가 하나씩 찍을 수 있습니다. 상자 크기와 시각·진행 값이 고정이라 찍을 때마다 같습니다. 예외는 불확정 진행 막대의 움직이는 띠(CI는 `prefers-reduced-motion: reduce`로 찍으면 멈춥니다), `frame.live`, `plot.dense`의 시간 표시입니다.
- 견본: `frame.empty`, `frame.normal`, `frame.busy`, `frame.stale`, `frame.error`, `frame.blocked`, `frame.tutorial`, `frame.skipped`, `frame.aside`(곁 패널 자리의 도움말, 트레이 기록 펼침), `frame.live`(모의 서비스에서 실제로 돌고 취소됨), `jobs.idle`, `jobs.running`, `jobs.history`, `frame.project-menu`, `frame.settings`, `ui.button`, `ui.card`, `ui.pill`, `ui.number`, `ui.choice`, `ui.table`, `ui.table.empty`, `ui.meter`, `ui.toast`, `ui.dialog`, `ui.tooltip`, `ui.states`, `plot.measure`, `plot.highlight`, `plot.target`, `plot.xo`, `plot.axes`, `plot.stale`, `plot.dense`, `map.room`, `map.room.unassigned`, `map.arrangement.9`, `map.arrangement.17`, `map.arrangement.13`, `map.thumbs`.
- 찍기: `npm run build:gallery && npm run gallery:shots`가 `gallery-shots/`에 견본마다 PNG를 남깁니다(밝게·어둡게 × 한국어·영어, 튜토리얼 강조색, 그리고 Rack·Matrix 스킨의 밝게·어둡게 한국어: 9개 변형). 이 컨테이너처럼 Chromium이 이미 있으면 `CHROMIUM_PATH`로 가리키고, 없으면 `npx playwright install chromium`으로 받습니다(`playwright`는 개발 의존성으로 고정).
- **화면 투어**: `gallery.html?tour=<장면 id>&theme=light|dark&lang=ko|en`은 견본 대신 실제 `App`을 모의 서비스 위에 창 전체로 띄우고, 장면 표(`gallery/tour/`)의 준비(`setup`, 마운트 전 모의 서비스 조작)와 조작(`ops`, 마운트 뒤 키·누르기·페이지 안 함수)으로 정한 상태에서 멈춥니다. 모의 서비스는 자동 틱 없이(`tickMs: 0`) 돌아 찍을 때마다 같은 그림이 납니다. `node scripts/screen-tour.mjs`가 장면마다 1280×800 창(`tauri.conf.json`의 기본 창)을 밝게·어둡게, 한국어·영어로, 그리고 Rack·Matrix 스킨의 어두운 한국어로 찍어(6개 변형) `screen-shots/`에 PNG와 `manifest.json`을 남깁니다. 실장비 없이는 닿지 않는 상태(측정·계산 중, 실패, 결과, 내보내기 완료)도 실제 화면 코드로 그립니다. 화면을 바꿔 장면의 조작이 맞지 않게 되면 그 PR에서 장면 표를 고칩니다.
- **리뷰 이슈**: `ui-gallery.yml`이 화면 PR(`apps/secs2r/ui/**`)과 수동 실행에서 투어와 견본을 찍어 `ui-gallery` 브랜치에 올리고, PR마다 리뷰 이슈 하나(`화면 갤러리 리뷰: PR #N`, 라벨 `ui-gallery`)를 열거나 댓글로 갱신합니다. 이슈 글은 `.github/scripts/gallery_issue.py`가 짓습니다(본문은 투어, 댓글은 한국어·밝게 견본). 브랜치에는 장면 전부와 한국어·밝게 견본만 올리고(나머지 견본 변형은 실행 아티팩트), 최근 4번의 실행만 남기도록 매번 다시 쓰므로 저장소를 복제해도 스크린숏이 쌓이지 않고, 더 오래된 이슈는 이미지를 잃습니다. 이슈는 머지를 막지 않습니다.
- Playwright에는 `locale`을 주어야 합니다. 브라우저 언어 태그가 잘못되면(`en-US@posix`처럼) uPlot이 `Intl.NumberFormat`에서 멈춥니다.
- **모의 서비스**(`ipc/mock.ts`): `createMockService({ tickMs, project, results })`는 `makeClient`에 넘길 `invoke`와 그 클라이언트, 잡 이벤트 버스를 줍니다. 생성된 바인딩으로 타입을 맞춘 고정값(`ipc/fixtures.ts`)으로 답하고, 잡은 타이머(또는 `tick()`, `drain()`)로 진행·로그·완료 이벤트를 보냅니다. `failNext`, `failAlways`, `failJobs`로 실패시킬 수 있고, 받은 요청은 `calls`에 남습니다. 같은 종류의 잡이 돌면 `job_busy`, 없는 잡을 취소하면 `job_not_found`입니다.

## 13. 검사

| 검사 | 파일 |
|---|---|
| 14px 하한(캡션 13px), 모든 `font-size`가 토큰, `transition: all` 없음, 어두운 테마 두 벌 일치, 밝게·어둡게 같은 토큰 집합, 줄인 움직임에서 시간 0, 구성 요소 CSS에 색 값 없음(토큰 파일과 스킨 파일만 예외) | `styles/tokens.test.ts` |
| 측정한 대비(글자 4.5:1, 테두리·초점·글리프 3:1), 강조색이 상태색과 15° 이상, 중립색의 한 기운, 순흑·순백 없음, 스킨 파일의 어두운 두 벌 일치와 밝게·어둡게 색 토큰 집합 일치, 모든 스킨 × 테마 | `styles/contrast.test.ts` |
| 카탈로그 키와 자리표시자 일치, 해요체 문장과 느낌표 없음 | `i18n/i18n.test.ts` |
| 레일 상태와 막힌 이유, 주 동작 끄기(막힘, 화면이 끔), 곁 패널 자리, 단축키, 상태 요약, 영어 | `app/AppFrame.test.tsx` |
| 이벤트로 그리는 잡, `job_cancel`, 기록의 원인·다음 행동·자세히 | `app/jobs/JobTray.test.tsx`, `jobStore.test.ts` |
| 숫자 칸 범위·단계·확정 | `ui/NumberField.test.tsx` |
| 타겟 점 한계(10–24000 Hz, ±30 dB, 두 점 이상) | `ui/plot/targetEdit.test.ts` |
| 타겟 점과 XO 선의 한 동작 한 저장(`onCommit`), 점 두 번 누르기, 하한만 있는 RC 대역, `Graph`의 쥔 범위 | `ui/plot/plot.test.tsx` |
| 메뉴의 고름과 상태만 알리는 고름, 라벨 왼쪽 `Select` | `ui/Popover.test.tsx`, `ui/controls.test.tsx` |
| 선 규칙, 색은 역할을 따름, 고정 눈금 | `ui/plot/series.test.ts` |
| 배치도 키보드 선택, 역할 미지정 | `ui/maps/ArrangementMap.test.tsx` |
| 모의 서비스 계약 | `ipc/mock.test.ts` |
| 갤러리의 모든 견본이 Chromium에서 오류 없이 그려지고 13px 미만 글자가 없음, 견본마다 PNG(밝게·어둡게, 한국어·영어, 튜토리얼 강조색, Rack·Matrix 스킨) | `scripts/gallery-shots.mjs`, `ci.yml`의 `gallery` 잡 |
| 화면 투어의 모든 장면이 오류 없이 그려지고 13px 미만 글자가 없음, 장면마다 PNG 여섯 장(밝게·어둡게, 한국어·영어, Rack·Matrix 스킨의 어두운 한국어), 장면 표의 형식 | `scripts/screen-tour.mjs`, `ui-gallery.yml`의 `shots` 잡, `gallery/tour/scenes.test.ts` |
