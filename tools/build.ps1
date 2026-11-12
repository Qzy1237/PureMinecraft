# =============================================================================
#  PureMinecraft 一键构建
#    1. cargo build --release
#    2. gcc 编译 uninstall.exe
#    3. 组装 stage\（bin / config / src / logs）
#    4. makensis -> dist\PureMinecraft-Setup-<ver>.exe
#    5. 桌面快捷方式
# =============================================================================

param([switch]$NoShortcut)

$ErrorActionPreference = 'Stop'

$ToolsDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$Root     = Split-Path -Parent $ToolsDir
$BinDir   = Join-Path $Root 'bin'
$CfgDir   = Join-Path $Root 'config'
$SrcDir   = Join-Path $Root 'src'
$ResDir   = Join-Path $Root 'resources'
$StageDir = Join-Path $Root 'stage'
$DistDir  = Join-Path $Root 'dist'
$TauriDir = Join-Path $Root 'src-tauri'

$AppName = 'PureMinecraft'
$AppVer  = '1.0.0'
$LangTag = ''

Write-Host "=== 构建 $AppName ===" -ForegroundColor Cyan

# 工具链自适应：本机有的路径才用，否则走系统 PATH
if (Test-Path 'E:\Rust\cargo')     { $env:CARGO_HOME  = 'E:\Rust\cargo' }
if (Test-Path 'E:\Rust\rustup')    { $env:RUSTUP_HOME = 'E:\Rust\rustup' }
if (Test-Path 'E:\Rust\cargo\bin') { $env:PATH = "E:\Rust\cargo\bin;$env:PATH" }
if (Test-Path 'E:\msys64\mingw64\bin') { $env:PATH = "E:\msys64\mingw64\bin;$env:PATH" }

$msvcRoot = $null
foreach ($cand in @('E:\VS2022', 'C:\Program Files\Microsoft Visual Studio\2022\BuildTools',
                    'C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools')) {
    if (Test-Path (Join-Path $cand 'VC\Tools\MSVC')) { $msvcRoot = $cand; break }
}
if (-not $msvcRoot) { throw "找不到 MSVC 工具链" }
$vcvars = Join-Path $msvcRoot 'VC\Auxiliary\Build\vcvars64.bat'
$dump = cmd /c "`"$vcvars`" >nul 2>&1 && set"
foreach ($line in $dump) {
    if ($line -match '^([^=]+)=(.*)$') { Set-Item -Path ("env:" + $matches[1]) -Value $matches[2] }
}

Write-Host "[1/5] cargo build --release ..." -ForegroundColor Yellow
# 图标变更不会触发 build.rs 重跑，主动 touch 保证重新嵌入资源
$buildRs = Join-Path $TauriDir 'build.rs'
if (Test-Path $buildRs) { (Get-Item $buildRs).LastWriteTime = Get-Date }
Push-Location $TauriDir
cargo build --release
$rc = $LASTEXITCODE
Pop-Location
if ($rc -ne 0) { throw "cargo build 失败" }

if (-not (Test-Path $BinDir)) { New-Item -ItemType Directory -Path $BinDir | Out-Null }
Copy-Item (Join-Path $TauriDir "target\release\$AppName.exe") (Join-Path $BinDir "$AppName.exe") -Force
Write-Host ("      bin\$AppName.exe  ({0:N0} KB)" -f ((Get-Item (Join-Path $BinDir "$AppName.exe")).Length / 1KB))

Write-Host "[2/5] 编译 uninstall.exe ..." -ForegroundColor Yellow
$unExe = Join-Path $ToolsDir 'uninstall.exe'
& gcc.exe -municode -O2 -mwindows -o $unExe (Join-Path $ToolsDir 'uninstall.c') `
    -lole32 -lshell32 -lshlwapi -static
if ($LASTEXITCODE -ne 0) { throw "uninstall 编译失败" }
& strip.exe --strip-all $unExe

Write-Host "[3/5] 组装 stage\ ..." -ForegroundColor Yellow
if (Test-Path $StageDir) { Remove-Item $StageDir -Recurse -Force }
foreach ($d in @('bin', 'config', 'logs', 'src', 'resources')) {
    New-Item -ItemType Directory -Force -Path (Join-Path $StageDir $d) | Out-Null
}
Copy-Item (Join-Path $BinDir "$AppName.exe") (Join-Path $StageDir 'bin') -Force
Copy-Item (Join-Path $CfgDir '*') (Join-Path $StageDir 'config') -Recurse -Force
Copy-Item $unExe (Join-Path $StageDir 'uninstall.exe') -Force
if (Test-Path (Join-Path $Root 'README.txt')) {
    Copy-Item (Join-Path $Root 'README.txt') (Join-Path $StageDir 'README.txt') -Force
}
if (Test-Path (Join-Path $ResDir "$AppName.ico")) {
    Copy-Item (Join-Path $ResDir "$AppName.ico") (Join-Path $StageDir 'resources') -Force
}
Copy-Item (Join-Path $SrcDir '*') (Join-Path $StageDir 'src') -Recurse -Force
Copy-Item (Join-Path $TauriDir 'src\main.rs')     (Join-Path $StageDir 'src') -Force
Copy-Item (Join-Path $TauriDir 'Cargo.toml')      (Join-Path $StageDir 'src') -Force
Copy-Item (Join-Path $TauriDir 'tauri.conf.json') (Join-Path $StageDir 'src') -Force
Copy-Item (Join-Path $ToolsDir 'build.ps1')       (Join-Path $StageDir 'src') -Force

# ---------- 可选：把自带 JDK 一起打进安装包（体积大，默认只放 exe 旁） ----------
$javaSrc = Join-Path $Root 'java'
if ((Test-Path $javaSrc) -and ($env:PCM_BUNDLE_JAVA -eq '1')) {
    Copy-Item $javaSrc (Join-Path $StageDir 'java') -Recurse -Force
    Write-Host '      java\ 已打入（含 JDK）' -ForegroundColor DarkGray
}
Write-Host "[4/5] makensis ..." -ForegroundColor Yellow
# ---------- 打入 llama.cpp 运行时（不含任何模型） ----------
$llamaDir = Join-Path $PSScriptRoot 'llama'
if (Test-Path $llamaDir) {
    Copy-Item (Join-Path $llamaDir '*') (Join-Path $StageDir 'bin') -Force -ErrorAction SilentlyContinue
    Write-Host '      bin\ 已打入 llama.cpp 运行时' -ForegroundColor DarkGray
}
$makensis = 'C:\Program Files (x86)\NSIS\makensis.exe'
if (-not (Test-Path $makensis)) { throw "找不到 makensis" }
if (-not (Test-Path $DistDir)) { New-Item -ItemType Directory -Path $DistDir | Out-Null }

Push-Location $ToolsDir
& $makensis /V2 'installer.nsi'
$rc = $LASTEXITCODE
Pop-Location
if ($rc -ne 0) { throw "makensis 失败" }

$setup = Join-Path $DistDir "PureMinecraft-Setup-$AppVer.exe"
Write-Host ("      dist\PureMinecraft-Setup-$AppVer.exe  ({0:N1} MB)" -f ((Get-Item $setup).Length / 1MB))

if ($NoShortcut) {
    Write-Host "[5/5] 已跳过快捷方式" -ForegroundColor DarkGray
} else {
    Write-Host "[5/5] 创建桌面快捷方式 ..." -ForegroundColor Yellow
    $mkExe = Join-Path $ToolsDir 'mkshortcut.exe'
    if (-not (Test-Path $mkExe)) {
        & gcc.exe -municode -O2 -o $mkExe (Join-Path $ToolsDir 'mkshortcut.c') `
            -lole32 -luuid -lshell32 -static
    }
    $desktop = [Environment]::GetFolderPath('Desktop')
    $lnk = Join-Path $desktop "$AppName.lnk"
    $icon = Join-Path $ResDir "$AppName.ico"
    & $mkExe (Join-Path $BinDir "$AppName.exe") $BinDir $lnk $icon "PureMinecraft 启动器"
    Write-Host "      $lnk"
}

Write-Host "=== 构建完成 ===" -ForegroundColor Green
exit 0
