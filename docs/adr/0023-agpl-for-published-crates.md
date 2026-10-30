# 0023. 공개 크레이트와 PyPI 패키지는 AGPL-3.0-only

상태: 채택, 2026-10-30

## 배경

SECS 2R은 업스트림 SECS 2를 따라 GPL-3.0-only입니다(ADR 0001). ADR 0016에 따라 공개 API 크레이트 `sound-equalization-and-correction-system-rust-api`와 그것이 의존하는 내부 크레이트 15개를 crates.io에 올렸고, 같은 엔진을 PyPI 패키지 `secs2r`로도 낼 계획입니다.

GPL은 프로그램을 배포할 때만 소스 공개를 요구합니다. 엔진을 서버에 넣어 네트워크 서비스로 제공하면 배포가 아니므로 수정한 소스를 공개하지 않아도 됩니다. 공개 API는 바로 그렇게 쓰기 좋은 모양입니다.

유지보수자는 2026-10-30에 업스트림 저작자 KIM-O-O(스피커 갤러리 닉네임 김ㅇㅇ)에게서 API 패키지에 포함된 SECS 2를 AGPLv3로 다시 라이선스해도 된다는 허가를 받았습니다. 허가 글과 그 스크린샷은 [`docs/licensing/SECS2-AGPL-PERMISSION.md`](../licensing/SECS2-AGPL-PERMISSION.md)에 있습니다. SECS 2가 GPL-3.0-only이므로 이 허가가 없었다면 재라이선스는 할 수 없었습니다.

## 결정

- **crates.io에 올리는 크레이트 16개는 AGPL-3.0-only입니다.** 공개 API 크레이트와 내부 크레이트 15개(`secs-types`, `secs-dsp`, `secs-compute`, `secs-engine`, `secs-jos`, `secs-mdat`, `secs-project`, `secs-dirac`, `secs-export`, `secs-graph`, `secs-camilla`, `secs-filters`, `secs-rew`, `secs-audio`, `secs-service`)가 대상입니다. 공개 API 크레이트 하나만 바꾸면 같이 공개된 `secs-service`를 직접 써서 AGPL을 피할 수 있으므로 16개를 함께 바꿉니다(유지보수자 결정 2026-10-30).
- **PyPI 패키지 `secs2r`는 처음부터 AGPL-3.0-only로 냅니다.**
- **앱과 게시하지 않는 크레이트는 GPL-3.0-only 그대로입니다.** `apps/` 아래 앱, `secs-sys`, `secs-gpu`, `secs-cuda-sys`, `secs-local-api`, `secs-policy`, `secs-oracle`, `secs-engine-testkit`이 그렇습니다. 앱은 GPLv3 13조에 따라 AGPL 크레이트와 하나로 결합해 배포할 수 있고, 결합한 작품 가운데 AGPL 크레이트 부분에는 AGPL의 조건이 그대로 적용됩니다. 앱의 로컬 API는 127.0.0.1에서만 열리므로 다른 사람에게 네트워크 서비스를 제공하는 경우에 해당하지 않습니다.
- **원문과 사본.** 저장소 루트에 AGPL-3.0 원문 `LICENSE-AGPL`(https://www.gnu.org/licenses/agpl-3.0.txt, SHA-256 `0d96a4ff68ad6d4b6f1f30f713b18d5184912ba8dd389f86aa7710db079abcb0`)을 둡니다. 루트 `LICENSE`(GPL-3.0)는 업스트림 파일이므로 그대로 둡니다. 게시 크레이트마다의 `LICENSE`는 `LICENSE-AGPL`의 사본이고, `secs-policy`의 게시 검사가 사본과 원문이 같은지, 게시 크레이트의 `license`가 `AGPL-3.0-only`인지, 게시하지 않는 크레이트가 작업 공간의 GPL을 따르는지 확인합니다.
- **의존성 검사.** `cargo deny`의 라이선스 허용 목록에는 AGPL을 넣지 않고, 우리 게시 크레이트 16개에만 예외로 허용합니다. 서드파티 AGPL 크레이트는 지금처럼 들이지 않습니다.
- **이미 올린 판은 그대로입니다.** crates.io에 이미 올린 판(api-v0.5.0까지)은 GPL-3.0-only로 남고, 다음 라이브러리 판부터 AGPL-3.0-only입니다.

## 결과

- 공개 크레이트나 PyPI 패키지를 고쳐 네트워크 서비스로 제공하는 사람은 그 서비스의 사용자에게 수정한 소스를 제공해야 합니다(AGPL 13조).
- 공개 크레이트에 기여한 코드는 AGPL-3.0-only로 배포됩니다. `CONTRIBUTING.md`에 적습니다.
- 공개 크레이트를 쓰는 프로그램은 AGPL과 호환되는 조건으로 배포해야 합니다. 공개 API 크레이트의 문서와 README에 적습니다.
