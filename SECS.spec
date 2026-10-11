# -*- mode: python ; coding: utf-8 -*-
# 진입점은 루트의 run_secs.py, 코드 본체는 src/secs 패키지다.
# 빌드 전 dist\SECS\SECS.exe 가 실행 중이면 파일이 잠겨 조용히 실패한다 → 먼저 종료할 것.
import os
import sys

from PyInstaller.utils.hooks import collect_submodules

SRC = os.path.abspath('src')
if SRC not in sys.path:
    sys.path.insert(0, SRC)   # 아래 collect_submodules('secs') 가 패키지를 찾게 한다

hiddenimports = ['numpy', 'scipy', 'scipy.fft', 'scipy.ndimage', 'scipy.optimize', 'scipy.signal', 'soundfile', 'requests', 'PyQt6.QtCore', 'PyQt6.QtGui', 'PyQt6.QtWidgets']
# pyqtgraph 의 예제·OpenGL·주피터는 뺀다(2026-10-10 슬림화): examples.test_examples 가 pytest → pygments →
# pygments.formatters.img → PIL(12.8 MB)을 끌고 왔다. 위저드는 셋 다 안 쓴다(그래프는 래스터).
hiddenimports += collect_submodules(
    'pyqtgraph',
    filter=lambda name: not name.startswith(('pyqtgraph.examples', 'pyqtgraph.opengl', 'pyqtgraph.jupyter')))
# secs 하위 모듈은 --check 가 importlib 로 동적 임포트하므로 전부 넣는다.
hiddenimports += collect_submodules('secs')


a = Analysis(
    ['run_secs.py'],
    pathex=[SRC],
    binaries=[],
    # 로고(창 아이콘·사이드바). 코드에서는 secs.wizard.branding 이 패키지 옆 assets 폴더에서 읽는다.
    # sample_dolby_714.npz(샘플 모드 실측 IR, `tools/make_sample_dolby.py` 로 재생성)도 같은 방식 —
    # secs.rewapi.fake 가 패키지 옆에서 읽는다(`_sample_dolby_path`).
    datas=[(os.path.join(SRC, 'secs', 'wizard', 'assets', 'app.ico'), os.path.join('secs', 'wizard', 'assets')),
           (os.path.join(SRC, 'secs', 'wizard', 'assets', 'app_256.png'), os.path.join('secs', 'wizard', 'assets')),
           (os.path.join(SRC, 'secs', 'wizard', 'assets', 'loading_drip.png'), os.path.join('secs', 'wizard', 'assets')),
           (os.path.join(SRC, 'secs', 'rewapi', 'sample_dolby_714.npz'), os.path.join('secs', 'rewapi')),
           # 튜토리얼이 보여 줄 데모 보정 결과(계산하지 않는다, 10-11) — `tools/make_sample_calibration.py` 로 재생성.
           (os.path.join(SRC, 'secs', 'rewapi', 'sample_calibration.pkl.gz'), os.path.join('secs', 'rewapi')),
           # REW 예측을 File -> Open 으로 여는 .mdat 을 만들 때 복제하는 단일 측정 템플릿.
           (os.path.join(SRC, 'secs', 'rew_mdat', 'template_1.mdat'), os.path.join('secs', 'rew_mdat')),
           # 도움말 튜토리얼 그림(2026-09-24) — `tools/make_tutorial_shots.py` 로 재생성. 도움말 창이
           # 패키지 옆 assets/tutorial 에서 MANIFEST.json 과 <key>_<lang>.png 를 읽는다.
           (os.path.join(SRC, 'secs', 'wizard', 'assets', 'tutorial', '*.png'),
            os.path.join('secs', 'wizard', 'assets', 'tutorial')),
           (os.path.join(SRC, 'secs', 'wizard', 'assets', 'tutorial', 'MANIFEST.json'),
            os.path.join('secs', 'wizard', 'assets', 'tutorial'))],
    hiddenimports=hiddenimports,
    hookspath=[],
    hooksconfig={},
    runtime_hooks=[],
    excludes=['tkinter', 'matplotlib', 'PySide6', 'PyQt5', 'pytest', '_pytest', 'pygments', 'PIL'],
    noarchive=False,
    optimize=0,
)


# 안 쓰는 Qt 파일을 뺀다(2026-10-10 슬림화, 약 30 MB):
#   opengl32sw.dll(소프트웨어 OpenGL 19.7 MB) — src 에 OpenGL 사용이 없고 pyqtgraph 기본은 래스터.
#   Qt6Pdf.dll · imageformats/qpdf.dll(4.4 MB) — PDF 를 읽지 않는다.
#   Qt 번역 96개(6.4 MB) — `secs.i18n` 은 한국어 표준 버튼('확인'·'취소')용 qtbase_ko.qm 하나만 읽는다.
def _qt_needed(dest):
    name = dest.replace('\\', '/').lower()
    if name.endswith(('/opengl32sw.dll', '/qt6pdf.dll', '/imageformats/qpdf.dll')):
        return False
    if '/qt6/translations/' in name:
        return name.rsplit('/', 1)[-1] in ('qtbase_ko.qm', 'qt_ko.qm')
    return True


a.binaries = [entry for entry in a.binaries if _qt_needed(entry[0])]
a.datas = [entry for entry in a.datas if _qt_needed(entry[0])]
pyz = PYZ(a.pure)

# 실행 파일은 하나: "SECS <버전 표시>.exe" = 측정 위저드. 이름은 src/secs/version.py 의 VERSION_LABEL 에서 온다.
# (2026-09-20~10-09 에는 옛 1.41 화면용 "SECS 1.41.exe" 가 하나 더 있었다 — 10-10 슬림화에서 옛 화면과 함께 지움.)
from secs.version import APP_NAME, VERSION_LABEL, __version__

ICON = os.path.join(SRC, 'secs', 'wizard', 'assets', 'app.ico')


def version_file(description, version_text, filename):
    """exe 에 넣을 버전 정보 파일을 만든다 — 작업 관리자·파일 속성에 프로그램 이름과 버전이 나온다.
    없으면 작업 관리자가 파일 이름만 보여 주고, 설명이 빈 프로그램으로 보인다."""
    from PyInstaller.utils.win32.versioninfo import (FixedFileInfo, StringFileInfo, StringStruct,
                                                     StringTable, VarFileInfo, VarStruct, VSVersionInfo)
    import re
    digits = [int(n) for n in re.findall(r'\d+', version_text)][:4]
    numbers = tuple(digits + [0] * (4 - len(digits)))
    info = VSVersionInfo(
        ffi=FixedFileInfo(filevers=numbers, prodvers=numbers),
        kids=[StringFileInfo([StringTable('040904B0', [
                  StringStruct('FileDescription', description),
                  StringStruct('ProductName', description),
                  StringStruct('FileVersion', version_text),
                  StringStruct('ProductVersion', version_text),
                  StringStruct('InternalName', description),
                  StringStruct('OriginalFilename', description + '.exe')])]),
              VarFileInfo([VarStruct('Translation', [1033, 1200])])])
    os.makedirs('build', exist_ok=True)
    path = os.path.join('build', filename)
    with open(path, 'w', encoding='utf-8') as handle:
        handle.write(str(info))
    return path


def make_exe(name, icon=None, version=None):
    return EXE(
        pyz,
        a.scripts,
        [],
        exclude_binaries=True,
        name=name,
        icon=icon,
        version=version,
        debug=False,
        bootloader_ignore_signals=False,
        strip=False,
        upx=True,
        console=False,
        disable_windowed_traceback=False,
        argv_emulation=False,
        target_arch=None,
        codesign_identity=None,
        entitlements_file=None,
    )


exe = make_exe(f'{APP_NAME} {VERSION_LABEL}', icon=ICON,          # 위저드 — 로고 아이콘 + 이름·버전 정보
               version=version_file(f'{APP_NAME} {VERSION_LABEL}', __version__, 'version_wizard.txt'))
coll = COLLECT(
    exe,
    a.binaries,
    a.datas,
    strip=False,
    upx=True,
    upx_exclude=[],
    name='SECS',
)
