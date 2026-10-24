# 묶여 나가는 글꼴

앱은 글꼴을 직접 싣습니다(`docs/DESIGN.md` §4). 네트워크에서 받는 것은 없고, `fonts.css`의 `@font-face`가 이 폴더의 woff2를 가리킵니다. `manifest.json`은 파일마다 출처·버전·SHA-256·굵기·커버리지(코드 포인트 범위)를 적고, `fonts.test.ts`가 파일과 스타일시트와 카탈로그를 그 매니페스트에 맞춰 검사합니다.

| 파일 | 글꼴 | 쓰임 | 출처 | 라이선스 |
|---|---|---|---|---|
| `PretendardVariable.woff2` | Pretendard Variable 1.3.9 | 모든 스킨의 라벨·본문(`--font-ui`) | npm `pretendard@1.3.9`의 `dist/web/variable/woff2/PretendardVariable.woff2`를 그대로 | OFL-1.1, `LICENSE-Pretendard.txt` |
| `SarasaMonoK-{Regular,SemiBold,Bold}.woff2` | Sarasa Mono K 1.0.42 | Matrix 스킨의 판독값·좌표·코드(`--font-mono`) | Sarasa Gothic 1.0.42(Arch Linux `ttf-sarasa-gothic 1.0.42-2`의 `Sarasa-{Regular,SemiBold,Bold}.ttc`에서 'Sarasa Mono K' 면)를 서브셋 | OFL-1.1, `OFL-Sarasa.txt` |
| `Orbit-Regular.woff2` | Orbit 1.000 | Matrix 스킨의 띠 활자(`--skin-band-font`) | [JAMO-TYPEFACE/Orbit](https://github.com/JAMO-TYPEFACE/Orbit) `Fonts/ttf/Orbit-Regular.ttf`를 서브셋 | OFL-1.1, `OFL-Orbit.txt` |

## 왜 이렇게 싣는가

- **Pretendard는 손대지 않습니다.** 라이선스가 글꼴 이름을 예약(Reserved Font Name)하고 있어 서브셋을 떠서 같은 이름으로 내보내면 조건을 어깁니다. 공식 variable woff2(2.0 MB)가 이미 웹용 빌드이고 한글 11,172자를 다 담고 있습니다.
- **Sarasa Mono K와 Orbit은 서브셋입니다.** 둘 다 이름을 예약하지 않습니다. Sarasa는 원본이 굵기당 45 MB(한중일 전부)라 UI가 보일 수 있는 범위(`manifest.json`의 `unicodes`: 라틴과 그 기호, 화살표·연산자·도형, 호환 자모, 한글 음절 전부)로 줄여 1.0 MB 안팎이 됩니다. 합자(`calt`, `liga`, `dlig`)는 뺍니다. 판독값은 글자를 글자대로 보여야 하기 때문입니다.
- **한글 음절은 전부 싣습니다.** 사용자가 적는 프로젝트·장치 이름은 KS X 1001의 2,350자에 갇히지 않습니다. Orbit만 설계상 2,780자인데, 띠의 라벨은 카탈로그 문장이라 `fonts.test.ts`가 카탈로그의 모든 음절이 Orbit에 있는지 봅니다. 없는 기호는 `fonts.css`의 다음 글꼴(Pretendard)로 떨어집니다.
- **같은 이름의 시스템 글꼴에 밀리지 않습니다.** 페이지의 `@font-face`는 같은 이름으로 설치된 글꼴보다 먼저 잡히므로, 사용자 PC의 Pretendard가 어떤 판이든 앱의 글자는 같습니다.

## 다시 만들기

```sh
pip install "fonttools[woff]==4.60.1" "brotli==1.1.0"
python3 apps/secs2r/ui/scripts/build-fonts.py \
  --pretendard <PretendardVariable.woff2> \
  --sarasa-dir <SarasaMonoK-{Regular,SemiBold,Bold}.ttf가 있는 폴더> \
  --orbit <Orbit-Regular.ttf>
```

### 바이트 재현 확인 (2026-10-24)

최초 생성 환경의 도구 판은 남아 있지 않습니다. 위의 fontTools 4.60.1과
Brotli 1.1.0을 Python 3.13에서 설치해 별도 임시 폴더에 다시 만들었으며,
출하 파일은 덮어쓰지 않았습니다. Orbit과 Sarasa 세 굵기의 SHA-256이
`manifest.json`과 일치했습니다. Pretendard는 원본 복사이므로 서브셋 도구를 거치지 않습니다.

- Orbit 원본 리비전: [`95122395c6d1fda07aebdf36f566a75678bc3bdd`](https://github.com/JAMO-TYPEFACE/Orbit/tree/95122395c6d1fda07aebdf36f566a75678bc3bdd)
- 원본 경로: `Fonts/ttf/Orbit-Regular.ttf` (784,480바이트)
- 원본 SHA-256: `5d0206fb0a9e3eeac51aff8d4a6dbb7613d63fc435f1a51f96dc35cefb5f9f87`
- Orbit 서브셋 SHA-256: `4d25ddd3161f43e05fe1445541bdd65e06dc5a898ad58f6ef89d476ad243bd46`

Sarasa 원본은 [Arch 보관소의 1.0.42-2 패키지](https://archive.archlinux.org/packages/t/ttf-sarasa-gothic/ttf-sarasa-gothic-1.0.42-2-any.pkg.tar.zst)입니다.
TTC에서 `Sarasa Mono K` 면을 TTF로 저장하면 `head.modified`가 추출 시각으로
바뀝니다. 이 값까지 달라지면 글리프와 다른 테이블이 같아도 woff2 바이트가
달라집니다. 재현할 때는 추출한 TTF를 `TTFont(path, recalcTimestamp=False)`로
열고 아래의 원래 `head.modified` 값(OpenType epoch 초)을 넣어 저장한 뒤
`build-fonts.py`에 넘깁니다. 이 값을 맞추기 전에는 세 굵기 모두 불일치했고,
맞춘 뒤에는 세 파일 모두 바이트 단위로 일치했습니다.

| 굵기 | `head.modified` | 서브셋 SHA-256 |
|---|---:|---|
| Regular | 3875516346 | `fbfa7352f127e4aacb7544d7bc20e999c225760e3328e263cf34f6aa7b361abe` |
| SemiBold | 3875516388 | `0243d618f44f617e5ebf584a08364220918417490ecc8e23b7b06af6149bf159` |
| Bold | 3875516340 | `2d9416b31583401e5b38f4e56033186eb69c5480aae1c839020ed8c951d4068f` |

원본을 올릴 때는 이 표와 `manifest.json`의 `upstream`, `docs/THIRD_PARTY.md`를 함께 고칩니다. 크기 상한(6 MiB)은 `fonts.test.ts`에 있습니다.
