# SECS → SECS.exe 빌드 스크립트 (PyInstaller, onedir)
# 실행: powershell -NoProfile -ExecutionPolicy Bypass -File build_exe.ps1
# 진입점은 run_secs.py, 코드 본체는 src\secs 패키지다(1.41 원본 secs4.py 는 문서\지난 자료\ 에 보존).
#
# 반드시 SECS.spec 으로 빌드한다. CLI 플래그(--collect-submodules secs)로 빌드하면 안 된다:
#   PyInstaller 가 만들어 실행하는 spec 에는 src 가 sys.path 에 없어서
#   collect_submodules('secs') 가 조용히 0개를 돌려준다. 그러면 정적 분석으로 찾은 45개만
#   들어가고, importlib 로만 불리는 모듈(rewapi.callback, rewapi.rew 등)이 빠져
#   --check 가 ModuleNotFoundError 로 죽는다.
#   SECS.spec 은 collect_submodules 앞에서 src 를 sys.path 에 넣어 이 문제를 막는다.
#
# 빌드 보관(2026-09-20): dist\ 는 git 에 넣지 않는다(약 200 MB). 빌드마다 dist\SECS\BUILD_INFO.txt 에
#   버전·커밋을 적고, -Archive 를 주면 릴리스\SECS_<버전>_<git describe>.zip 으로 묶어 둔다.
#   실행: powershell -NoProfile -ExecutionPolicy Bypass -File build_exe.ps1 -Archive
#   더블클릭 런처(.cmd)의 원본은 배포\ 에 있고(git 관리) 빌드할 때 dist\ 로 복사한다.
#   이 파일은 UTF-8 BOM 으로 저장한다 — PowerShell 5.1 은 BOM 이 없으면 한글 경로를 깨뜨린다.
param([switch]$Archive)
$ErrorActionPreference = 'Stop'
Set-Location $PSScriptRoot

# dist\SECS\SECS.exe 가 실행 중이면 파일이 잠겨 빌드가 조용히 실패한다 → 먼저 종료.
$running = Get-Process -Name "SECS*" -ErrorAction SilentlyContinue   # "SECS 2 …" (옛 SECS 1.41.exe 도 이 패턴)
if ($running) {
    Write-Host "실행 중인 SECS.exe $($running.Count)개를 종료합니다."
    $running | Stop-Process -Force
    Start-Sleep -Seconds 1
}

$py = ".\.venv-secs\Scripts\python.exe"
if (-not (Test-Path $py)) {
    python -m venv .venv-secs
    & $py -m pip install --upgrade pip
    & $py -m pip install numpy scipy soundfile PyQt6 pyqtgraph requests pyinstaller
}

& $py -m PyInstaller --noconfirm --clean SECS.spec
if ($LASTEXITCODE -ne 0) { throw "PyInstaller 실패 (exit $LASTEXITCODE)" }

# src\secs 의 모든 하위 모듈이 실제로 번들에 들어갔는지 확인한다.
# importlib 로만 불리는 모듈은 빠져도 빌드는 성공하고 실행할 때에야 터지므로 여기서 잡는다.
$expected = & $py -c "import sys,os; sys.path.insert(0, os.path.abspath('src')); from PyInstaller.utils.hooks import collect_submodules; print('\n'.join(sorted(collect_submodules('secs'))))"
if ($LASTEXITCODE -ne 0) { throw "하위 모듈 목록 수집 실패" }
$toc = Get-Content ".\build\SECS\PYZ-00.toc" -Raw
$missing = $expected | Where-Object { $_ -and ($toc -notmatch [regex]::Escape("'$_'")) }
if ($missing) { throw "번들에서 빠진 모듈: $($missing -join ', ')" }
Write-Host "하위 모듈 $($expected.Count)개 모두 번들에 포함됨."

# 런처 복사 + 빌드 정보 + (선택) 버전별 보관
Remove-Item ".\dist\SECS 1.41*.cmd" -ErrorAction SilentlyContinue   # 10-10 에 지운 옛 화면 런처가 dist 에 남지 않게
Copy-Item ".\배포\*.cmd" ".\dist\" -Force
# GPL v3 공개(10-10): zip 에 라이선스 · 제3자 고지를 같이 넣는다(dist\ 가 zip 의 뿌리)
Copy-Item ".\LICENSE", ".\THIRD_PARTY_NOTICES.md" ".\dist\" -Force
$version = (& $py -c "import sys,os; sys.path.insert(0, os.path.abspath('src')); from secs.version import __version__; print(__version__)").Trim()
$describe = (git describe --tags --long --dirty 2>$null)
if (-not $describe) { $describe = 'no-git' }
$info = "version=$version`r`ngit=$describe`r`nbuilt=$(Get-Date -Format 'yyyy-MM-dd HH:mm:ss')`r`n"
[IO.File]::WriteAllText((Join-Path $PSScriptRoot 'dist\SECS\BUILD_INFO.txt'), $info)
Write-Host "BUILD_INFO: $version / $describe"
if ($Archive) {
    $folder = Join-Path $PSScriptRoot '릴리스'
    New-Item -ItemType Directory -Force $folder | Out-Null
    $zip = Join-Path $folder "SECS_${version}_$describe.zip"
    Compress-Archive -Path ".\dist\SECS", ".\dist\*.cmd", ".\dist\LICENSE", ".\dist\THIRD_PARTY_NOTICES.md" -DestinationPath $zip -CompressionLevel Optimal -Force
    Write-Host ("보관: {0} ({1:N0} MB)" -f $zip, ((Get-Item $zip).Length / 1MB))
}

Get-ChildItem ".\dist\SECS\*.exe" | ForEach-Object { Write-Host "완료: dist\SECS\$($_.Name)" }
