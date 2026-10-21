# Downloads Bonsai's slim LGPL FFmpeg (built by .github/workflows/ffmpeg.yml from scripts/build-ffmpeg.sh)
# from this repo's GitHub releases, verifies the checksums, and installs ffmpeg/ffprobe as Tauri sidecars.
# Never swap in a GPL build (e.g. gyan "full_build"): it would force the whole app to GPL.
param(
    [string]$Tag = "ffmpeg-n9.0.2-bonsai1"
)
$ErrorActionPreference = "Stop"

$base = "https://github.com/flp2113/bonsai-video-compressor/releases/download/$Tag"
$triple = "x86_64-pc-windows-msvc"
$binDir = Join-Path $PSScriptRoot "..\src-tauri\binaries"

$work = Join-Path ([IO.Path]::GetTempPath()) ("bonsai-ffmpeg-" + [guid]::NewGuid())
New-Item -ItemType Directory -Path $work | Out-Null
try {
    foreach ($file in "SHA256SUMS", "ffmpeg.exe", "ffprobe.exe") {
        Write-Host "Downloading $base/$file"
        Invoke-WebRequest -Uri "$base/$file" -OutFile (Join-Path $work $file)
    }

    # Each line of SHA256SUMS is "<hash>  <file name>".
    $expected = @{}
    foreach ($line in Get-Content (Join-Path $work "SHA256SUMS")) {
        $hash, $name = $line -split "\s+\*?", 2
        $expected[$name.Trim()] = $hash.ToLower()
    }
    foreach ($tool in "ffmpeg", "ffprobe") {
        $actual = (Get-FileHash (Join-Path $work "$tool.exe") -Algorithm SHA256).Hash.ToLower()
        if ($actual -ne $expected["$tool.exe"]) { throw "Checksum mismatch for $tool.exe" }
    }

    New-Item -ItemType Directory -Force -Path $binDir | Out-Null
    foreach ($tool in "ffmpeg", "ffprobe") {
        Copy-Item (Join-Path $work "$tool.exe") (Join-Path $binDir "$tool-$triple.exe") -Force
    }
    Write-Host "Installed ffmpeg/ffprobe ($Tag) into $((Resolve-Path $binDir).Path)"
}
finally {
    Remove-Item -Recurse -Force $work
}
