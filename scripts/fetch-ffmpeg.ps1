param(
    [string]$RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
)

$ErrorActionPreference = "Stop"

$BinDir = Join-Path $RepoRoot "src-tauri\binaries"
New-Item -ItemType Directory -Force -Path $BinDir | Out-Null

$Dest = Join-Path $BinDir "ffmpeg-x86_64-pc-windows-msvc.exe"
if (Test-Path $Dest) {
    Write-Host "ffmpeg already present at $Dest"
    exit 0
}

$ZipUrl = "https://github.com/BtbN/FFmpeg-Builds/releases/download/latest/ffmpeg-master-latest-win64-gpl.zip"
$TempZip = Join-Path $env:TEMP "sprite-studio-ffmpeg-win64.zip"
$TempDir = Join-Path $env:TEMP "sprite-studio-ffmpeg-extract"

Write-Host "Downloading ffmpeg for Windows..."
Invoke-WebRequest -Uri $ZipUrl -OutFile $TempZip

if (Test-Path $TempDir) {
    Remove-Item -Recurse -Force $TempDir
}
New-Item -ItemType Directory -Force -Path $TempDir | Out-Null
Expand-Archive -Path $TempZip -DestinationPath $TempDir -Force

$Ffmpeg = Get-ChildItem -Path $TempDir -Recurse -Filter "ffmpeg.exe" | Select-Object -First 1
if (-not $Ffmpeg) {
    throw "ffmpeg.exe was not found in the downloaded archive"
}

Copy-Item $Ffmpeg.FullName $Dest -Force
Write-Host "Installed bundled ffmpeg to $Dest"
