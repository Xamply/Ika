# Rebuilds the wgpu launcher (target/release) and the Wallpaper Engine/video
# bridge (scratch/we-bridge-bin) from the current checkout.
# `python -B bridge/run.py` already calls this when needed; run it by hand only
# to force a full rebuild. -BridgeOnly skips cargo.
param([switch]$BridgeOnly)
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

if (-not $BridgeOnly) {
    Write-Host '== Launcher (cargo)' -ForegroundColor Cyan
    cargo build --release --locked
    if ($LASTEXITCODE) { throw 'cargo build failed' }
}

$detours = Join-Path $root 'scratch/we-detours'
if (-not (Test-Path (Join-Path $detours 'src'))) {
    git clone --depth 1 --branch v4.0.1 https://github.com/microsoft/Detours.git $detours
    if ($LASTEXITCODE) { throw 'Detours clone failed' }
}
$vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio/Installer/vswhere.exe'
$vs = & $vswhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
if (-not $vs) { throw 'Visual Studio Build Tools with C++ are required' }
$vcvars = Join-Path $vs 'VC/Auxiliary/Build/vcvars64.bat'

Write-Host '== Bridge (Wallpaper Engine + video)' -ForegroundColor Cyan
$bin = Join-Path $root 'scratch/we-bridge-bin'
foreach ($part in 'host.exe', 'producer.dll', 'video.exe', 'video-mpv.exe', 'video-profile-mpv.exe') {
    Remove-Item -LiteralPath (Join-Path $bin $part) -Force -ErrorAction SilentlyContinue
}
cmd /c "`"$vcvars`" >nul && `"$root\bridge\build.cmd`" `"$detours`" `"$bin`""
if ($LASTEXITCODE) { throw 'bridge build failed' }

# libmpv and the D3D compiler are third-party runtime DLLs, not built here.
foreach ($dll in 'libmpv-2.dll', 'd3dcompiler_43.dll') {
    if (-not (Test-Path (Join-Path $bin $dll))) {
        Write-Warning "Missing $dll in $bin (copy it there for video playback)."
    }
}
