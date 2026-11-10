# ART 실측 비교 게이트의 자료

[ADR 0026](../../docs/adr/0026-art-reference-gate.md)의 게이트가 읽는 자료입니다. `*.art` 파일은 방 측정을 암호화한 묶음이고, `cases.toml`은 사례 목록입니다. 측정과 거기서 나온 값은 공개하지 않습니다. 평문, 복호화한 파일, 지표의 값은 커밋하지 않고 PR, 이슈, 로그에도 적지 않습니다.

## 사례

| 이름 | 기준 | 내용 |
| --- | --- | --- |
| `art-a-*` | ART 실측 | 보정 전 9위치 측정, 주 청취 위치의 출력별 측정, 같은 위치에서 ART를 켜고 잰 측정 |
| `input-a-*` | 다른 방의 ART 기준 | `art-a`와 같은 방에서 그보다 먼저 잰 Dirac Live 프로젝트의 보정 전 측정. 2.2 구성 여섯 벌과 4.2 구성 세 벌이고, 4.2의 마지막 것은 위치 3곳만 잰 것입니다. |
| `input-b-*` | 다른 방의 ART 기준 | Dirac Live 프로젝트에서 가져온 보정 전 9위치 측정. 프로젝트마다 스피커 구성이 다르고, 이름 끝의 숫자가 그 구성입니다. |

`input-a`는 `art-a`와 같은 방이지만 스피커 자리와 측정 날짜가 달라, 그 방의 ART 측정과 직접 비교하지 않고 다른 방처럼 다룹니다. `input-b`의 6.x 구성에는 뒤 서라운드가 한 대 있는데, SECS 2R에는 그 자리의 역할이 없어 그 스피커를 빼고 넣었습니다. 내용이 같은 프로젝트(자동 저장본)는 하나만 넣었습니다.

## 실행

키는 저장소 시크릿에 있습니다(이름은 ADR 0026). 로컬에서는 같은 값을 환경 변수에 넣고 실행합니다.

```sh
cargo build --release --locked -p secs2r-cli
python .github/scripts/art_gate.py verify --manifest reference/art/cases.toml
python .github/scripts/art_gate.py run --manifest reference/art/cases.toml --cli target/release/secs2r-cli
```

`verify`는 키 없이 암호문과 매니페스트의 해시만 대조합니다. `run`은 사례마다 설정 묶음을 계산하고 지표별 판정과 등급을 출력합니다. `--values-out <파일>`을 주면 지표의 값을 그 파일에 남깁니다. 이 파일은 저장소 밖에 두십시오.

## 사례 추가

1. 사례 폴더를 만듭니다. `case.json`, 프로젝트(`project/<이름>.secs`와 그 IR 폴더), 평가 위치의 출력별 보정 전 측정을 넣습니다. ART 측정이 있으면 입력별 WAV를 `art/`에 넣고 `case.json`의 `art`에 적습니다. 형식은 `.github/scripts/art_gate.py`의 머리말에 있습니다.
2. `python .github/scripts/art_gate.py pack --case-dir <폴더> --name <이름> --out reference/art/<이름>.art`로 암호화합니다.
3. 출력된 `sha256`을 `cases.toml`의 새 항목에 적습니다. ART 측정이 있으면 `kind = "art"`, 없으면 `kind = "input"`입니다.
4. `.liveproject` 원본, 계정 ID, 파일 경로, 측정 UUID가 폴더에 남아 있지 않은지 확인합니다.

키를 바꾸면 모든 묶음을 다시 암호화해야 합니다.
