# 0039. conda-forge 패키지 secs2r

상태: 채택, 2026-11-04

## 배경

엔진과 로컬 API 클라이언트는 crates.io(ADR 0016), PyPI(ADR 0019), nuget.org(ADR 0032)에 올라갑니다. 유지보수자는 2026-11-04에 conda-forge와 npm에도 올리기로 했고, conda-forge를 먼저 하기로 했습니다. npm은 ADR 0040이 다룹니다.

conda-forge는 우리 CI가 패키지를 올리는 곳이 아닙니다. 레시피를 `conda-forge/staged-recipes`에 PR로 내고, 검토가 끝나면 conda-forge 조직 아래에 feedstock 저장소가 생기며, 그 저장소의 CI가 플랫폼마다 빌드해 `conda-forge` 채널에 올립니다. 판 갱신은 봇이 PyPI의 새 판을 보고 feedstock에 PR을 엽니다. 이 ADR은 레시피의 내용과 우리 저장소가 지킬 것을 정합니다. 사실 확인은 2026-11-04의 staged-recipes README, conda-forge 문서, cryptography와 orjson의 v1 레시피, rust·maturin·python-abi3 feedstock으로 했습니다.

## 결정

### 레시피

- 형식은 v1 `recipe.yaml`(rattler-build)이고 자리는 `recipes/secs2r/recipe.yaml`입니다. staged-recipes는 새 레시피에 v1을 요구합니다.
- 이름은 PyPI와 같은 `secs2r`, 판은 PyPI 판(`api-v<판>` 태그, 공개 크레이트와 같음)입니다. 소스는 PyPI의 sdist(`https://pypi.org/packages/source/s/secs2r/secs2r-<판>.tar.gz`)와 그 sha256입니다. 봇이 이 URL로 새 판을 알아봅니다.
- 빌드는 `${{ compiler("rust") }}`, `${{ compiler("c") }}`, `${{ stdlib("c") }}`와 호스트의 `python`, `pip`, `maturin >=1.15,<2`로 하고, `python -m pip install . --no-deps --no-build-isolation`으로 sdist 최상위에서 설치합니다. sdist의 `pyproject.toml`이 `bindings/python/Cargo.toml`을 가리키므로 최상위에서 빌드해야 합니다.
- abi3 휠이지만 `noarch`는 아닙니다. `build.python.version_independent: ${{ is_abi3 }}`와 호스트 `python-abi3 ${{ python_min }}.*`로 conda-forge의 abi3 방식을 따르고, 런타임은 `python >=${{ python_min }}`입니다. conda-forge의 `python_min`이 3.11이므로 conda 패키지는 3.11 이상을 대상으로 하며, PyPI 휠의 3.9 하한과는 별개입니다. free-threaded CPython과 PyPy는 건너뜁니다. Windows는 `PYO3_USE_RAW_DYLIB=0`으로 호스트 Python의 import library를 씁니다.
- 라이선스는 AGPL-3.0-only(ADR 0023)이고, `license_file`에 sdist의 `LICENSE`와 빌드 중 `cargo-bundle-licenses`로 모은 `THIRDPARTY.yml`을 넣습니다. Rust 의존은 빌드 중 Cargo가 crates.io에서 받습니다. cryptography와 orjson의 feedstock이 같은 방식입니다. staged-recipes의 CI가 네트워크를 막으면 그때 체크섬이 있는 vendor 소스로 바꿉니다.
- 시험은 `import secs2r`, `secs2r._secs2r`, `pip check`, `Engine().call({"method": "bootstrap"})`의 판이 레시피 판과 같은지, 그리고 abi3 변형에서 `abi3audit`입니다. 엔진 생성과 bootstrap은 네트워크나 실행 중인 앱이 없어도 됩니다.
- `extra.recipe-maintainers`는 `115dkk`입니다.

### feedstock

- 기본 대상은 conda-forge의 기본값(`linux-64`, `osx-64`, `win-64`)입니다. `osx-arm64`는 feedstock이 생긴 뒤 `conda-forge.yml`의 `build_platform`으로 교차 빌드를 추가합니다. 레시피에는 교차 빌드에 드는 `cross-python_${{ target_platform }}`와 빌드 환경의 `python`, `maturin`을 미리 두었습니다.
- feedstock의 `conda-forge.yml`에 `bot: automerge: true`를 둡니다. 봇이 연 판 갱신 PR은 검사가 모두 통과하면 사람 없이 머지됩니다. 검사가 실패한 PR은 머지되지 않으므로, 새 판에서 빌드가 깨지면 유지보수자(와 그 AI)가 feedstock을 고칩니다.
- 레시피의 원본은 feedstock입니다. 이 저장소에는 사본을 두지 않습니다. 사본을 두면 둘이 어긋나기 때문입니다. 이 ADR이 feedstock 주소와 규칙을 적고, 첫 빌드가 채널에 오르면 README에 `conda install -c conda-forge secs2r` 줄을 추가합니다.

### 우리 저장소가 지킬 것

- `release.yml`은 바꾸지 않습니다. PyPI에 sdist가 오르면 봇이 알아서 PR을 엽니다.
- PyPI 패키지의 빌드 요구(`maturin` 하한, abi3 설정, sdist에 드는 크레이트)가 바뀌면 feedstock의 레시피도 고쳐야 합니다. ADR 0019의 변경은 이 ADR도 함께 봅니다.
- 봇의 갱신 PR이 세 개 이상 열린 채 남으면 봇이 새 PR을 멈추므로, 깨진 갱신 PR은 그때그때 닫거나 고칩니다.

### 절차

1. `conda-forge/staged-recipes`를 `115dkk/staged-recipes`로 포크하고, 브랜치 `secs2r`에 `recipes/secs2r/recipe.yaml`을 넣어 PR을 엽니다. PR 본문은 staged-recipes의 점검표를 따릅니다.
2. CI(linux-64, osx-64, win-64)와 검토자의 지적을 고칩니다. 검토 대기는 며칠에서 몇 주입니다.
3. feedstock이 생기면 `conda-forge.yml`에 자동 머지와 `osx-arm64` 교차 빌드를 넣고 다시 렌더링합니다.
4. 첫 패키지가 채널에 오르면 이 저장소의 README에 설치 줄을 추가합니다.

## 결과

- Python 사용자가 PyPI 대신 conda-forge 채널로 같은 패키지를 받습니다. Rust 툴체인 없이 설치됩니다.
- 패키지의 판은 PyPI와 같이 움직이고, 우리 릴리스 절차는 늘지 않습니다.
- 레시피 검토와 feedstock 유지는 conda-forge의 절차를 따르므로 이 저장소의 게이트 밖에 있습니다. 깨진 빌드는 feedstock에서 고칩니다.
