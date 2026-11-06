# 0041. macOS 앱의 자체 서명 인증서

상태: 채택, 2026-11-06

## 배경

0.27.2부터 0.53.0까지 macOS 앱은 임시 서명(ad-hoc)만 되어 있었습니다. 서명이 아예 없으면 Apple 실리콘 Mac이 받은 앱을 손상된 것으로 보고 열지 않기 때문에 넣은 것이고, Apple이 발급하는 Developer ID 인증서는 유료 개발자 계정이 있어야 받을 수 있어 쓰지 않았습니다.

임시 서명에는 서명 인증서가 없으므로 macOS는 앱을 코드 디렉터리 해시(cdhash)로 알아봅니다. 이 해시는 빌드마다 다르기 때문에 macOS에게 SECS 2R의 판은 저마다 다른 프로그램입니다. 마이크 허락은 앱의 지정 요구 사항(designated requirement)과 함께 저장되는데, 그것이 cdhash이면 판이 바뀔 때마다 허락 창이 다시 열립니다(ADR 0020의 결과에 적어 둔 문제). 자동 업데이트가 머지마다 새 판을 내보내므로 사용자는 새 판으로 처음 측정할 때마다 허락 창을 다시 봅니다.

유지보수자는 임시 서명 대신 자체 서명으로 바꾸라고 했습니다(2026-11-06).

## 결정

### 인증서

- 프로젝트가 스스로 발급한 X.509 인증서로 앱 번들에 서명합니다. 주체는 `CN=SECS 2R (self-signed), O=SECS 2R contributors`, 키는 RSA 3072, 서명은 SHA-256, 용도는 코드 서명(`keyUsage digitalSignature`, `extendedKeyUsage codeSigning`)입니다. Keychain Access가 만드는 '자체 서명 루트, 코드 서명' 인증서와 같은 모양이라 codesign이 자기 자신을 앵커로 받아들입니다.
- 2026-11-06에 만들었고 2046-11-06에 만료됩니다. codesign은 서명할 때 인증서가 유효해야 하므로, 만료가 가까워지면 아래 절차로 바꿉니다.
- 공개 절반은 `apps/secs2r/macos/codesign.pem`에 둡니다. 릴리스 검사가 앱의 서명을 이 파일과 맞춰 보고, 사용자도 `codesign -dvv`의 `Authority=`로 같은 인증서인지 봅니다. 지문은 SHA-1 `c202de54a24d45771d563d048eb14372157c817c`, SHA-256 `77f874ab2fbc029a3c0445de1ae28b7251a3a88f308eb4d6ad82869364d984e3`입니다. SHA-1은 macOS가 지정 요구 사항의 `certificate leaf = H"…"`에 쓰는 값입니다.
- 비밀 절반은 PKCS#12로 묶어 저장소 환경 `updater-signing`의 시크릿 `MACOS_CODESIGN_P12`(base64)와 `MACOS_CODESIGN_P12_PASSWORD`에 둡니다. 이 환경은 main에서만 배포하므로 PR 브랜치의 워크플로는 읽지 못합니다(업데이터 키와 같은 자리, ADR 0020). 유지보수자의 PC에 백업이 있습니다.

### 서명

- `release.yml`의 macOS 잡은 `tauri build` 전에 `.github/scripts/macos_codesign.py prepare-keychain`을 돌립니다. 이 스크립트는 시크릿의 p12를 러너의 임시 키체인에 넣고, 인증서를 코드 서명 용도로 신뢰하게 만들고, p12에 든 인증서가 저장소의 공개 인증서와 같은지 확인한 뒤, `APPLE_SIGNING_IDENTITY`에 인증서 이름을 내보냅니다. Tauri는 이 변수를 `tauri.conf.json`의 `signingIdentity`보다 먼저 보고 `codesign -s "SECS 2R (self-signed)" --options runtime`으로 번들 안쪽부터 서명합니다.
- Tauri 자체의 `APPLE_CERTIFICATE` 가져오기는 쓰지 않습니다. Apple이 발급한 이름(`Developer ID Application:` 등)의 인증서만 알아보기 때문입니다.
- 시크릿이 없으면 릴리스가 실패합니다. 임시 서명으로 조용히 내보내면 사용자가 허락 창을 다시 보게 되기 때문입니다.
- `tauri.conf.json`의 `signingIdentity`는 `-`(임시 서명) 그대로 둡니다. 로컬 빌드와 macOS 밖의 잡은 인증서가 필요 없습니다.
- 하드닝 런타임은 그대로 켜 둡니다. 마이크는 0.27.2부터와 같이 `com.apple.security.device.audio-input` 권한으로 씁니다.

### 검사

- PR의 CI(`ci.yml`)는 `updater-signing` 환경을 읽지 못하므로, 실행마다 `macos_codesign.py make-certificate`로 이틀짜리 인증서를 만들어 같은 절차로 키체인에 넣고 서명합니다. 업데이터 키를 실행마다 만드는 것과 같은 방식입니다(ADR 0020).
- `macos_app_check.py --signing-certificate <PEM>`은 모든 아키텍처 슬라이스가 임시 서명이 아니라 그 인증서로, 하드닝 런타임 아래 서명되었는지, 그리고 지정 요구 사항이 번들 식별자와 인증서의 해시를 담고 cdhash가 아닌지 확인한 뒤, 앱을 띄워 측정과 보정까지 돌립니다. 릴리스에서는 저장소의 공개 인증서와, PR에서는 그 실행의 인증서와 맞춰 봅니다.

### 인증서 교체

만료가 가까워지거나 비밀 키가 샜을 때 바꿉니다. `macos_codesign.py make-certificate`로 새 인증서를 만들고, 공개 절반으로 `apps/secs2r/macos/codesign.pem`을 바꾸고, 두 시크릿을 새 값으로 바꾼 뒤, 이 ADR의 지문과 날짜를 고칩니다. 새 인증서로 서명한 첫 판은 macOS에게 다른 프로그램이므로 사용자는 마이크 허락을 한 번 다시 합니다. 업데이터의 키(ADR 0020)와는 별개여서 자동 업데이트는 그대로 됩니다.

## 결과

- 판을 올려도(업데이터로든 손으로든) 마이크 허락이 유지됩니다. 0.53.0 이하에서 0.54.0으로 올릴 때는 서명 주체가 바뀌므로 허락 창이 한 번 더 열립니다.
- 사용자가 서명한 주체를 확인할 수 있습니다. `codesign -dvv`의 `Authority=SECS 2R (self-signed)`와 `codesign -d -r -`의 `certificate leaf = H"c202de54a24d45771d563d048eb14372157c817c"`이 저장소의 공개 인증서와 맞습니다.
- 달라지지 않는 것이 있습니다. Apple이 공증(notarization)하지 않은 앱이므로 처음 열 때 Apple이 악성 코드 여부를 확인할 수 없다는 경고와 '그래도 열기'는 그대로입니다. 공증은 Apple 개발자 계정과 Developer ID 인증서가 있어야 합니다. 그렇게 되면 이 ADR을 대신하는 ADR을 씁니다.
- macOS CLI(`secs2r-cli` tar.gz)는 이 인증서로 서명하지 않습니다. 링커가 넣는 임시 서명만 있고, 터미널에서 쓰는 도구라 마이크 허락과 무관합니다.
- `updater-signing` 환경은 이제 업데이터 키와 코드 서명 인증서를 함께 가집니다. 이름은 그대로 둡니다.
- 비밀 키를 잃으면 인증서를 교체합니다. 업데이터 키와 달리, 잃어도 이미 깔린 판은 그대로 쓸 수 있고 다음 판에서 허락 창이 한 번 더 열리는 것이 전부입니다.
