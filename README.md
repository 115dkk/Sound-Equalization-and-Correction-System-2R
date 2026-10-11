
# Sound Equalization and Correction System 2

**한국어** | [English](#english)

SECS(Sound Equalization & Correction System) 2 는 룸 보정 설계 프로그램입니다.
REW 로 측정한 스피커 응답으로 룸 보정(RC) · 서브 정렬(BA) · 저역 늦은 울림 보정(MIMO)을 설계하고, CamillaDSP 설정이나 REW mdat 로 내보냅니다.
Windows 용이고 설치 없이 폴더째 실행합니다.

> 독립 프로젝트입니다. REW, CamillaDSP 등 다른 제품 이름은 각 소유자의 것이며, 이 프로젝트는 그들과 관계가 없습니다.

## 무엇을 하나

장치·마이크 → 구성 → 레벨 체크 → 배치 → 측정 → 보정 설계 → 내보내기 순서로 진행합니다.

- **측정**: 스피커 구성(2.0 ~ 9.1.6, 직접 추가, 서브 0~8개)을 정하고 마이크를 여러 위치에 두고 잽니다. REW Pro 가 있으면 SECS 가 REW 를 조종해 자동으로 재고, 없으면 REW 에서 잰 `.mdat` 를 불러옵니다.
- **보정 설계**:
  - 룸 보정(RC): 타깃 커브에 맞추는 PEQ/FIR 필터와 GD(군지연) 보정
  - 크로스오버(XO) 추천과 서브 정렬(BA): 서브의 게인 · 딜레이 · 극성 · 올패스. DBA · 멀티서브 · 인프라 서브 지원
  - MIMO: 여러 스피커를 함께 써서 저역의 늦은 울림을 줄이는 보정(마이크 위치 3곳 이상)
- **내보내기**: REW `.mdat`(채널마다 보정 전 · 후), CamillaDSP v4 설정(`.yml`)과 FIR(`.wav`)
- **튜토리얼 · 도움말**: 샘플 데이터로 따라 하는 튜토리얼, F1 도움말, 버튼마다 풍선 도움말

## 책임의 한계 (면책)

SECS 2 는 **있는 그대로(AS IS)** 제공되며, 어떤 보증도 하지 않습니다.
**SECS 2 를 쓰거나 쓰지 못해 생긴 모든 문제에 대해 개발자와 기여자는 책임지지 않습니다.** 스피커 · 앰프 등 장비 손상, 청력 손상, 데이터 손실, 측정 · 보정 결과의 오류, 그 밖의 직접 · 간접 손해가 모두 포함됩니다. 사용에 따른 위험은 전적으로 사용하는 분에게 있습니다. 자세한 조건은 `LICENSE`(GPL v3) 15 · 16조를 보세요.

- 측정 전에 앰프 볼륨을 낮게 두고 시작하세요. 스윕 신호가 트위터와 귀를 상하게 할 수 있습니다.
- 만든 필터를 DSP 에 올린 뒤에는 작은 소리로 먼저 들어 보세요.

## 받는 법

1. 이 저장소의 **Releases** 에서 `SECS_2.0.0-beta.1.1.0_….zip` 을 받아 원하는 곳에 풉니다.
2. `SECS` 폴더의 `SECS 2 Beta 1.1.0.exe` 를 실행합니다. 설치는 필요 없습니다.
3. Windows 가 "알 수 없는 게시자" 경고를 띄우면 **추가 정보 → 실행** 을 누릅니다(코드 서명이 없습니다).

## 요구 사항

- Windows 10 / 11 64-bit (Windows 11 에서 확인)
- **REW(Room EQ Wizard) 5.40 이상**
  - 자동 측정은 REW **Pro** 의 API 기능입니다. REW 를 `-api` 옵션으로 실행하세요: `"C:\Program Files\REW\roomeqwizard.exe" -api`
  - Pro 가 없으면 REW 에서 재서 `.mdat` 로 저장한 뒤 측정 화면의 **[mdat 불러오기]** 로 가져옵니다.
- 측정용 마이크 · 오디오 인터페이스
- CamillaDSP 로 적용하려면 CamillaDSP 4.1.x 와 CamillaGUI 4.1.x

## 처음 쓰는 법

1. 왼쪽 사이드바의 **[튜토리얼]** 을 누르면 샘플 데이터로 측정부터 내보내기까지 따라 할 수 있습니다. REW · 마이크가 필요 없고 소리도 나지 않습니다.
2. 막히면 **F1**(도움말)을 누르세요.
3. 실제 측정은 REW 를 `-api` 로 먼저 켠 뒤 1단계부터 순서대로 진행합니다. 프로젝트는 `.secs` 파일로 저장됩니다.

## CamillaDSP 로 적용하기

내보내기 화면에서 CamillaDSP 를 고르면 CamillaGUI 폴더와 같은 모양(`configs\*.yml`, `coeffs\*.wav`)으로 파일을 만듭니다.
CamillaDSP · CamillaGUI 설치부터 장치 · 라우팅 설정, 파일 올리기까지는 **F1 도움말 → 내보내기 화면의 「CamillaDSP」 탭**에 그림으로 있습니다. CamillaDSP 5.x 는 지원하지 않습니다.

## 알려진 제한

- 화면은 한국어만 지원합니다.
- 바이앰프(스피커 하나를 대역별로 따로 재기)는 지원하지 않습니다.
- MIMO 는 계산이 오래 걸리고(입력 · 위치가 많으면 10분 넘게), 실제 효과가 예측보다 작을 수 있습니다.
- 내보내기 형식은 REW mdat 와 CamillaDSP 뿐입니다.
- Windows 에서 CamillaDSP 로 12채널 이상을 입력받는 쉬운 방법은 확인하지 못했습니다(맥은 BlackHole 16ch 로 가능).
- Windows 용 exe 만 있습니다.

## 소스에서 실행 · 빌드

Python 3.13 기준입니다.

```powershell
python -m venv .venv-secs
.venv-secs\Scripts\python.exe -m pip install -r requirements.txt
.venv-secs\Scripts\python.exe run_secs.py --fake-rew     # REW 없이 샘플 데이터로 실행
.venv-secs\Scripts\python.exe run_secs.py                # REW 연동으로 실행

.venv-secs\Scripts\python.exe -m pip install -r requirements-dev.txt
powershell -NoProfile -ExecutionPolicy Bypass -File build_exe.ps1   # exe 빌드(dist\SECS\)
```

## 라이선스

SECS 2 는 **GNU General Public License v3(GPL-3.0)** 자유 소프트웨어입니다. 전문은 [`LICENSE`](LICENSE), 제3자 패키지 고지는 [`THIRD_PARTY_NOTICES.md`](THIRD_PARTY_NOTICES.md) 에 있습니다.
**Pull Request 는 받지 않습니다.** 고치고 싶은 곳이 있으면 자유롭게 포크해서 쓰세요.

## 도움 주신 분들

이 프로젝트는 한국의 오디오 커뮤니티 스피커 갤러리의 Sound Equalization and Correction System 프로젝트에서 시작한 시스템입니다. SECS 2의 모든 소스는 공개되어 있으며 자유로운 수정이 가능합니다(GPL v3). SECS 2가 골방의 구원이 되길 바랍니다.

— 스피커 갤러리 일동

## 문의

**문의 · 버그 신고는 받지 않습니다.** 취미로 만든 프로그램이라 답변이나 수정을 약속할 수 없습니다.

---

<a id="english"></a>

# Sound Equalization and Correction System 2 (English)

SECS (Sound Equalization & Correction System) 2 is a room correction design program. From speaker measurements taken with REW, it designs room correction (RC), subwoofer alignment (BA) and low-frequency late-decay correction (MIMO), and exports the result as a CamillaDSP configuration or REW `.mdat` files. Windows only; no installer.

> An independent project. REW, CamillaDSP and other product names belong to their owners; this project is not affiliated with them.

**The program's interface is Korean only.**

## What it does

Devices/Mic → Setup → Level Check → Positions → Measure → Calibration → Export.

- **Measure**: speaker layouts from 2.0 to 9.1.6 or custom, 0-8 subwoofers, several microphone positions. With REW Pro, SECS drives REW and measures automatically; without it, load `.mdat` files measured in REW.
- **Calibration**:
  - Room correction (RC): PEQ/FIR filters toward a target curve, plus group-delay correction
  - Crossover (XO) suggestion and subwoofer alignment (BA): gain, delay, polarity, all-pass; DBA, multi-sub and infra-sub layouts
  - MIMO: uses several speakers together to reduce low-frequency late decay (3 or more mic positions)
- **Export**: REW `.mdat` (before/after per channel), CamillaDSP v4 configuration (`.yml`) and FIR `.wav` files
- **Tutorial and help**: a tutorial with sample data, F1 help, tooltips

## Disclaimer

SECS 2 is provided **"as is", without warranty of any kind**.
**The developers and contributors accept no liability for any problem arising from the use of, or inability to use, SECS 2**, including damage to loudspeakers, amplifiers or other equipment, hearing damage, data loss, errors in measurement or correction results, and any other direct or indirect damages. You use it entirely at your own risk. See sections 15 and 16 of `LICENSE` (GPL v3).

- Turn the amplifier volume down before you start measuring; sweep signals can damage tweeters and hearing.
- After loading new filters into your DSP, listen at a low level first.

## Download and run

1. Download `SECS_2.0.0-beta.1.1.0_….zip` from **Releases** and unzip it anywhere.
2. Run `SECS 2 Beta 1.1.0.exe` in the `SECS` folder. No installation needed.
3. If Windows warns about an unknown publisher, choose **More info → Run anyway** (the exe is not code-signed).

## Requirements

- Windows 10 / 11, 64-bit (tested on Windows 11)
- **REW (Room EQ Wizard) 5.40 or later**. Automatic measurement uses the REW **Pro** API (start REW with `-api`); without Pro, load `.mdat` files measured in REW.
- A measurement microphone and audio interface
- For CamillaDSP: CamillaDSP 4.1.x and CamillaGUI 4.1.x (5.x is not supported)

## Known limitations

- Korean interface only.
- Bi-amping (measuring one speaker per band) is not supported.
- MIMO is slow (over 10 minutes with many inputs and positions) and its real-world effect can be smaller than predicted.
- Export formats are REW mdat and CamillaDSP only.
- No easy way was found to feed 12 or more channels into CamillaDSP on Windows (on macOS, BlackHole 16ch works).
- Windows exe only.

## Running and building from source

Python 3.13. See the commands in the Korean section above (`requirements.txt`, `run_secs.py --fake-rew`, `build_exe.ps1`).

## License

SECS 2 is free software under the **GNU GPL v3**. See [`LICENSE`](LICENSE) and [`THIRD_PARTY_NOTICES.md`](THIRD_PARTY_NOTICES.md).
**Pull requests are not accepted.** Feel free to fork it.

## Credits

This project grew out of the Sound Equalization and Correction System project of Speaker Gallery, a Korean audio community. All SECS 2 source code is open and free to modify (GPL v3). May SECS 2 be the salvation of every small listening room.

— The Speaker Gallery team

## Contact

**Questions and bug reports are not accepted.** This is a hobby project, so no replies or fixes can be promised.
