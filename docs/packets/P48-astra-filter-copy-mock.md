# P48 (ASTRA): 필터 내보내기의 문장과 모의 서비스

업스트림 SECS 2 Beta 1.3.0(이슈 #35)의 필터 내보내기(텍스트 · Equalizer APO)가 화면에 들어왔습니다(P47, 커밋 `8ca2922`). 이 작업서는 그 문장을 다듬고, 모의 서비스가 실제 서비스처럼 답하게 합니다. 서비스 연결은 Daybreak가 따로 합니다(`docs/packets/P46-daybreak-filter-service.md` — 아래 규칙의 정본).

## 작업 위치

- 워크트리 `/home/user/wt/p48`(브랜치 `p48`)에서만 일합니다. 다른 경로는 읽기만 합니다.
- 커밋하지 않습니다. git으로 인덱스나 작업 트리를 바꾸지 않습니다. 화면 의존성은 설치되어 있습니다(Node 24: `export PATH=/opt/nvm/versions/node/v24.22.0/bin:$PATH`).
- 먼저 `CLAUDE.md`(화면 문구 규칙), `docs/UI-TERMS.md`, `src/screens/export/ExportScreen.tsx`·`types.ts`, `src/ipc/mock.ts`의 `export` 처리, `src/app/jobs/jobText.ts`를 읽습니다. 업스트림 뜻의 정본은 `src/secs/wizard/tooltips.py`의 `cal.export_camilla`·`cal.export_text`·`cal.export_eqapo`, `src/secs/wizard/en_g8.py`의 '필터 내보내기' 쌍, `docs/upstream/README.md`의 'Equalizer APO 로 적용하기'입니다.

## 할 일

### 1. 문장 (`src/i18n/ko.json`·`en.json`)

아래 키의 ko·en 문장을 다듬습니다. 합니다체, 상태와 라벨은 명사구, 효과를 말하는 라벨, 꾸민 말 없이. 뜻은 업스트림 툴팁과 같게 하되 화면에서 이미 다른 줄이 하는 말을 되풀이하지 않습니다.

- `frame.step.export.purpose`: 단계 머리의 설명. 지금 "CamillaDSP 설정과 REW mdat 파일로"만 말합니다. Equalizer APO와 필터 값 텍스트를 넣되 한 문장으로.
- `export.group.filters`, `export.group.rew`: 묶음 이름(명사구).
- `export.kind.camilla_dsp.body`: 끝의 "RC, BA, MIMO를 모두 담습니다."와 앞 문장 "MIMO를 포함하면 서포트 경로도…"가 겹칩니다. 한 번만 말하게 고칩니다.
- `export.kind.filter_text`, `.body`, `.where`, `.run`, `export.status.exporting.filter_text`: 본문 끝 "RC만 적용한 결과에서 낼 수 있습니다."가 막힌 이유(`export.blocked.filter_text.subs`)의 둘째 문장과 겹칩니다. 본문은 범위를 짧게 말하고, 막힌 이유는 원인과 다음 행동만 말하게 나눕니다. 영어도 같습니다("Only for results with RC alone." / "Text is for results with RC only.").
- `export.kind.equalizer_apo`, `.body`, `.where`, `.run`, `export.status.exporting.equalizer_apo`.
- `export.blocked.filter_text.subs`, `export.blocked.filter_text.mimo`, `export.blocked.equalizer_apo.mimo`: 원인 + 다음 행동. MIMO의 다음 행동은 '보정에서 MIMO를 끄면 낼 수 있음', 서브의 다음 행동은 'CamillaDSP나 Equalizer APO로 내보냄'.
- `export.outcome.equalizer_apo.hint`: 저장 뒤 할 일. 업스트림 README처럼 기존 `config.txt`를 먼저 다른 이름으로 보관하라는 말이 필요한지 판단해 넣습니다(한 줄이 길어지면 안내 파일을 가리키는 쪽을 남깁니다). `{mode}` 자리표시를 지킵니다.
- `error.export_scope`, `error.export_scope.next`: 서비스가 범위 밖 내보내기를 거절했을 때(화면은 보통 단추를 꺼서 막으므로 드물게 보임). `params.reason`(`subs`/`mimo`)은 문장에 쓰지 않습니다.

자리표시(`{mode}` 등)는 ko·en이 같아야 하고, 키를 더하거나 지우지 않습니다. 파일 이름(`config.txt`, `EqAPO 적용 방법.txt`, `secs_{mode}`)은 서비스가 쓰는 그대로 둡니다(영어 문장에서도 안내 파일 이름은 한국어 그대로입니다).

### 2. 모의 서비스 (`src/ipc/mock.ts`, `src/ipc/fixtures.ts`)

`export`의 새 두 종류를 서비스 규칙대로 답하게 합니다.

- **범위**: 서브가 있음 = 지금 결과의 `chain.subs`가 비어 있지 않음, MIMO가 있음 = 모의 서비스가 카밀라 내보내기에 MIMO를 넣는 조건(MIMO 켜짐, 위치 3곳 이상, MIMO 결과 있음, 낡지 않음)과 같게. 텍스트는 서브 → `subs`, 아니면 MIMO → `mimo`; EqAPO는 MIMO → `mimo`. 막히면 잡을 시작하지 않고 `ipcError("export_scope", <detail>, { reason })`를 던집니다. 튜토리얼과 결과 없음의 기존 오류가 먼저입니다.
- **폴더**: `folder`가 비면 `<내보낼 폴더>/SECS_text_<RC_BA|RC_BA_MIMO>_<모드>_<YYYYmmdd-HHMMSS>`·`SECS_eqapo_…`(모드는 설정의 모드, 초고지연도 `ultra`; 시각은 모의 시계). 주면 그 폴더.
- **파일 목록**(서비스와 같은 순서): 텍스트 = `<폴더>/SECS 필터 <라벨>.txt`, 그다음 메인마다 `<폴더>/secs_<파일 모드>_<역할>_fir.wav`(FIR이 있는 모드만; `zero`는 wav 없음). EqAPO = `<폴더>/config.txt`, `<폴더>/secs_<파일 모드>/secs_<파일 모드>.txt`, `<폴더>/EqAPO 적용 방법.txt`, 그다음 메인마다 `<폴더>/secs_<파일 모드>/secs_<파일 모드>_<역할>_fir.wav`. 파일 모드 = 화면의 `filterModeName`(초고지연이면 `ultra200`), 라벨 = 서비스의 `mode_label`(`crates/secs-types/src/calibration.rs`; 모의 서비스에 이미 같은 이름이 있으면 그것을 씀).
- `fixtures.ts`의 모의 `bootstrap` 업스트림 판을 `UPSTREAM.toml`(Beta 1.3.0, `5acb5a0c786469f41bdeb51e5e5c9d06ce12b4e2`, 2026-10-15)과 맞춥니다.
- 테스트(`src/ipc/mock.test.ts` 또는 `mockWiring.test.ts`): 서브가 있는 결과에서 텍스트가 `export_scope`/`subs`로 거절되고 잡이 없음, MIMO가 든 결과에서 둘 다 `mimo`, 서브 없고 MIMO 없는 결과에서 텍스트 파일 목록, EqAPO 파일 목록과 폴더 이름.

### 3. 확인

- 화면 투어의 `export.eqapo-done` 장면이 이제 서비스 꼴의 파일 수와 폴더를 보여야 합니다. 투어를 내보내기 장면만 다시 찍어 PNG를 Read로 확인합니다.

## 고쳐도 되는 파일

`src/i18n/ko.json`·`en.json`(위 키의 문장만), `src/ipc/mock.ts`, `src/ipc/fixtures.ts`, `src/ipc/*.test.ts`, 필요하면 `src/state/*.test.ts`.

건드리지 않음: 화면 컴포넌트와 CSS(`src/screens/**/*.tsx`, `*.css`), `src/screens/*/types.ts`, `src/gallery/`, `scripts/`, `crates/`, `.github/`, `docs/`.

## 검증 (모두 포그라운드로, 보고를 마친 뒤에만 턴을 끝냅니다)

```sh
export PATH=/opt/nvm/versions/node/v24.22.0/bin:$PATH
cd /home/user/wt/p48/apps/secs2r/ui
npm run typecheck && npm test && npm run build
npm run build:gallery
CHROMIUM_PATH=/opt/pw-browsers/chromium-1194/chrome-linux/chrome node scripts/screen-tour.mjs --out /home/user/wt/p48-shots --only export.
```

## 보고 형식

1. 바꾼 파일과 각 한 줄 설명
2. 문장: 키마다 고치기 전 → 고친 뒤(ko·en), 겹침을 어떻게 나눴는지
3. 모의 서비스: 범위·폴더·파일 목록 규칙과 근거(서비스 작업서의 어느 줄)
4. 검증 명령마다 마지막 몇 줄의 출력, 본 PNG
