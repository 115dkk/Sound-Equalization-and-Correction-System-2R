# 제3자 소프트웨어 고지 (Third-party notices)

SECS 2 자체는 **GNU General Public License v3(GPL-3.0)** 로 공개합니다(`LICENSE` 참고).
Releases 의 실행 파일(exe, PyInstaller 로 묶음)과 이 저장소의 소스는 아래 패키지를 사용합니다.
버전과 라이선스는 Beta 1.0.0 을 빌드한 가상환경(Python 3.13)에서 `importlib.metadata` 로 읽은 값입니다.
각 패키지의 전체 라이선스 문구는 그 패키지의 배포물(`*.dist-info/licenses/`)과 아래 누리집에 있습니다.

SECS 2 itself is released under the **GNU General Public License v3 (GPL-3.0)**; see `LICENSE`.
The executable in Releases (bundled with PyInstaller) and this repository use the packages listed below.
Versions and licenses were read with `importlib.metadata` from the virtual environment (Python 3.13) used for the Beta 1.0.0 build.

## 1. 실행 파일에 들어가는 패키지 (bundled in the exe)

| 패키지 | 버전 | 라이선스 | 용도 | 누리집 |
|---|---|---|---|---|
| **PyQt6** | 6.11.0 | **GPL-3.0-only** (상용 이중 라이선스 중 GPL 쪽을 선택) | 화면(Qt 파이썬 바인딩) | https://www.riverbankcomputing.com/software/pyqt/ |
| **PyQt6-Qt6** (Qt 6 라이브러리) | 6.11.2 | **LGPL-3.0** | Qt 본체(QtCore · QtGui · QtWidgets) | https://www.qt.io/ |
| PyQt6-sip | 13.12.0 | BSD-2-Clause | PyQt6 의 파이썬↔C++ 연결 | https://github.com/Python-SIP/sip |
| numpy | 2.5.3 | BSD-3-Clause AND 0BSD AND MIT AND Zlib AND CC0-1.0 | 수치 계산 | https://numpy.org/ |
| scipy | 1.18.1 | BSD-3-Clause (부속 라이브러리 라이선스는 scipy 배포물의 `LICENSE.txt` 에 있음) | 신호 처리 · 최적화 | https://scipy.org/ |
| pyqtgraph | 0.14.0 | MIT | 그래프 | https://www.pyqtgraph.org/ |
| requests | 2.34.2 | Apache-2.0 | REW API(HTTP) 통신 | https://requests.readthedocs.io/ |
| urllib3 | 2.7.0 | MIT | requests 의 의존성 | https://github.com/urllib3/urllib3 |
| idna | 3.19 | BSD-3-Clause | requests 의 의존성 | https://github.com/kjd/idna |
| charset-normalizer | 3.5.1 | MIT | requests 의 의존성 | https://github.com/jawah/charset_normalizer |
| certifi | 2026.7.22 | MPL-2.0 | requests 의 의존성(인증서 묶음) | https://github.com/certifi/python-certifi |
| soundfile | 0.14.0 | BSD-3-Clause | 임펄스 응답 · FIR 의 wav 읽고 쓰기 | https://github.com/bastibe/python-soundfile |
| cffi | 2.1.1 | MIT-0 | soundfile 의 의존성 | https://cffi.readthedocs.io/ |
| pycparser | 3.0 | BSD-3-Clause | cffi 의 의존성 | https://github.com/eliben/pycparser |
| setuptools | 84.0.0 | MIT (부속 패키지는 각자의 라이선스, 그 배포물 안에 있음) | PyInstaller 분석 결과 번들에 포함됨 | https://github.com/pypa/setuptools |
| packaging | 26.3 | Apache-2.0 OR BSD-2-Clause | PyInstaller 분석 결과 번들에 포함됨 | https://github.com/pypa/packaging |
| colorama | 0.4.6 | BSD-3-Clause | pyqtgraph 의 의존성 | https://github.com/tartley/colorama |

위 표에서 **PyQt6 가 GPL v3** 이기 때문에, PyQt6 를 묶은 SECS 2 실행 파일도 GPL v3 로 배포하고 소스(이 저장소)를 함께 공개합니다.

### 같이 들어가는 네이티브 라이브러리

| 구성 요소 | 라이선스 | 어디서 오나 |
|---|---|---|
| Python 3.13 (CPython) | PSF License | PyInstaller 가 실행기로 묶음. https://www.python.org/ |
| OpenSSL 3 (`libcrypto-3.dll`, `libssl-3.dll`) | Apache-2.0 | CPython 배포물 |
| libffi (`libffi-8.dll`) · expat(`pyexpat`) · bzip2 · liblzma(xz) | MIT · MIT · BSD 계열 · 퍼블릭 도메인 | CPython 배포물 |
| scipy-openblas (OpenBLAS) (`numpy.libs\`, `scipy.libs\`) | BSD-3-Clause | numpy · scipy 휠 |
| libsndfile (`_soundfile_data\libsndfile_x64.dll`) | **LGPL-2.1** (안에 든 libFLAC · libogg · libvorbis · libopus 는 BSD-3-Clause, libmp3lame 은 LGPL-2 이상, libmpg123 은 LGPL-2.1) | soundfile 휠. 소스: https://github.com/libsndfile/libsndfile |
| Qt 가 쓰는 제3자 라이브러리(FreeType · HarfBuzz · libpng · zlib · PCRE2 등) | 각자의 허용형 라이선스 | PyQt6-Qt6 휠. 목록: https://doc.qt.io/qt-6/licenses-used-in-qt.html |
| Microsoft Visual C++ 런타임(`VCRUNTIME140*.dll`, `msvcp140*.dll`, `ucrtbase.dll`, `api-ms-win-*.dll`) | Microsoft 재배포 조건 | CPython · numpy · scipy 배포물 |

## 2. Qt (LGPL v3) 고지

이 프로그램은 **Qt 6** 라이브러리를 **GNU Lesser General Public License v3** 에 따라 사용합니다(PyQt6-Qt6 휠, Qt 6.11.2).

- Qt 는 The Qt Company Ltd. 와 Qt 프로젝트 기여자의 저작물이며, Qt 는 The Qt Company Ltd. 의 상표입니다.
- Qt 라이브러리는 실행 파일 옆 `_internal\PyQt6\Qt6\` 폴더에 **별도 DLL 로 동적 연결**되어 있어, 같은 버전의 다른 Qt 빌드로 바꿔 끼울 수 있습니다.
- 사용한 Qt 의 소스 코드: https://download.qt.io/official_releases/qt/ (6.11 계열) · https://code.qt.io/
- LGPL v3 전문: PyQt6-Qt6 휠의 `PyQt6_Qt6-6.11.2.dist-info/LICENSE`, 또는 https://www.gnu.org/licenses/lgpl-3.0.html
- 결합한 실행 파일 전체가 GPL v3 로 배포되고, 소스와 빌드 방법(`build_exe.ps1`)이 이 저장소에 있어 Qt 를 바꿔 다시 빌드할 수도 있습니다.

This program uses the **Qt 6** libraries under the **GNU LGPL v3** (PyQt6-Qt6 wheel, Qt 6.11.2). Qt is a trademark of The Qt Company Ltd.
The Qt libraries are dynamically linked as separate DLLs (`_internal\PyQt6\Qt6\`) and can be replaced with another build of the same Qt version.
Qt source: https://download.qt.io/official_releases/qt/ and https://code.qt.io/. The full LGPL v3 text is in the PyQt6-Qt6 wheel (`PyQt6_Qt6-6.11.2.dist-info/LICENSE`) and at https://www.gnu.org/licenses/lgpl-3.0.html.

## 3. 빌드 도구: PyInstaller 와 부트로더 예외

exe 는 **PyInstaller 6.22.3**(GPL-2.0-or-later, **Bootloader Exception** 포함)으로 만듭니다.
PyInstaller 의 부트로더와 관련 파일은 exe 에 그대로 들어가는데, PyInstaller 의 라이선스는 "컴파일된 부트로더와 관련 파일을 다른 프로그램에 링크하거나 넣어 배포하는 데 제한이 없다"는 예외를 줍니다(런타임 훅은 Apache-2.0). 그래서 부트로더가 SECS 2 의 라이선스(GPL v3)에 영향을 주지 않습니다.
PyInstaller 와 그 의존성(pyinstaller-hooks-contrib · altgraph · pefile · pywin32-ctypes)은 **빌드할 때만** 쓰고 exe 에 들어가지 않습니다(부트로더 제외).

The exe is built with **PyInstaller 6.22.3** (GPL-2.0-or-later with the **Bootloader Exception**: unlimited permission to link or embed the compiled bootloader and related files into combinations with other programs and to distribute them without restriction from those files; run-time hooks are Apache-2.0). PyInstaller and its dependencies are build-time tools only.

## 4. 개발 · 시험용 (exe 에 들어가지 않음)

pytest(MIT) · pluggy(MIT) · iniconfig(MIT) — `tests/` 를 돌릴 때만 필요합니다. 설치 목록은 `requirements.txt`.

## 5. 이 고지에 대하여

- 이 목록은 실행 파일(`dist\SECS\_internal`)과 PyInstaller 분석 결과를 기준으로 사람이 정리했습니다. 빠진 것이나 틀린 것이 있을 수 있습니다.
- 의존성을 올리면(`requirements.txt`) 이 파일의 버전도 함께 고칩니다.
