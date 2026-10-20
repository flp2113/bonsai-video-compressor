# Downloads a prebuilt LGPL FFmpeg (BtbN/FFmpeg-Builds) and installs ffmpeg/ffprobe as Tauri sidecars.
# Dev-only stopgap until we ship our own minimal LGPL build (see milestone 10).
# Never swap in a GPL build (e.g. gyan "full_build"): it would force the whole app to GPL.
param(
    [string]$Version = "9.0"
)
$ErrorActionPreference = "Stop"

$asset = "ffmpeg-n$Version-latest-win64-lgpl-$Version"
$url = "https://github.com/BtbN/FFmpeg-Builds/releases/download/latest/$asset.zip"
$triple = "x86_64-pc-windows-msvc"
$binDir = Join-Path $PSScriptRoot "..\src-tauri\binaries"

$work = Join-Path ([IO.Path]::GetTempPath()) ("bonsai-ffmpeg-" + [guid]::NewGuid())
New-Item -ItemType Directory -Path $work | Out-Null
try {
    Write-Host "Downloading $url"
    Invoke-WebRequest -Uri $url -OutFile (Join-Path $work "ffmpeg.zip")
    Expand-Archive -Path (Join-Path $work "ffmpeg.zip") -DestinationPath $work

    New-Item -ItemType Directory -Force -Path $binDir | Out-Null
    foreach ($tool in "ffmpeg", "ffprobe") {
        Copy-Item (Join-Path $work "$asset\bin\$tool.exe") (Join-Path $binDir "$tool-$triple.exe") -Force
    }
    Write-Host "Installed ffmpeg/ffprobe sidecars into $((Resolve-Path $binDir).Path)"
}
finally {
    Remove-Item -Recurse -Force $work
}
