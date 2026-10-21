#!/usr/bin/env bash
# Builds Bonsai's slim, LGPL-only FFmpeg for Windows x64 (ffmpeg.exe + ffprobe.exe) by cross-compiling on Linux.
#
# Runs in GitHub Actions (.github/workflows/ffmpeg.yml), or on any Debian/Ubuntu machine with:
#   sudo apt install mingw-w64 nasm meson ninja-build cmake pkg-config git curl make
# Output goes to ./dist: ffmpeg.exe, ffprobe.exe, BUILD-INFO.txt (exact sources and configure flags).
#
# Only what Bonsai uses is enabled (see "What Bonsai needs" in the milestone 10 plan / CLAUDE.md).
# Never add --enable-gpl, --enable-nonfree, libx264 or libx265: the build must stay LGPL so it can ship
# with an MIT app.
set -euo pipefail

FFMPEG_VERSION="n9.0.2"
NV_CODEC_HEADERS_VERSION="n12.1.14.0" # 12.1 headers work with older NVIDIA drivers than 13.x
AMF_VERSION="v1.5.3"
LIBVPL_VERSION="v2.17.0"
DAV1D_VERSION="1.5.4"

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
WORK="${WORK:-$ROOT/build-ffmpeg}"
PREFIX="$WORK/prefix" # where the dependencies are installed
DIST="$ROOT/dist"
HOST="x86_64-w64-mingw32"
JOBS="$(nproc)"

mkdir -p "$WORK" "$PREFIX" "$DIST"
export PKG_CONFIG_PATH="$PREFIX/lib/pkgconfig"
export PKG_CONFIG_LIBDIR="$PREFIX/lib/pkgconfig" # never pick up the Linux system's own libraries

fetch() { # fetch <repo url> <tag> <dir>
  [ -d "$WORK/$3" ] || git clone --quiet --depth 1 --branch "$2" "$1" "$WORK/$3"
}

# --- NVIDIA NVENC headers (MIT). Header-only: the driver's DLL is loaded at runtime. ---
if [ ! -f "$PREFIX/lib/pkgconfig/ffnvcodec.pc" ]; then
  fetch https://github.com/FFmpeg/nv-codec-headers.git "$NV_CODEC_HEADERS_VERSION" nv-codec-headers
  make -C "$WORK/nv-codec-headers" PREFIX="$PREFIX" install
fi

# --- AMD AMF headers (MIT). Header-only: the driver's DLL is loaded at runtime. ---
if [ ! -d "$PREFIX/include/AMF" ]; then
  git clone --quiet --depth 1 --branch "$AMF_VERSION" --filter=blob:none --sparse \
    https://github.com/GPUOpen-LibrariesAndSDKs/AMF.git "$WORK/amf"
  git -C "$WORK/amf" sparse-checkout set amf/public/include
  mkdir -p "$PREFIX/include/AMF"
  cp -r "$WORK/amf/amf/public/include/." "$PREFIX/include/AMF/"
fi

# --- Intel oneVPL dispatcher (MIT), for Quick Sync. Finds the Intel driver's runtime at runtime. ---
if [ ! -f "$PREFIX/lib/pkgconfig/vpl.pc" ]; then
  fetch https://github.com/intel/libvpl.git "$LIBVPL_VERSION" libvpl
  cmake -S "$WORK/libvpl" -B "$WORK/libvpl/build" -G Ninja \
    -DCMAKE_SYSTEM_NAME=Windows \
    -DCMAKE_C_COMPILER="$HOST-gcc-posix" -DCMAKE_CXX_COMPILER="$HOST-g++-posix" \
    -DCMAKE_RC_COMPILER="$HOST-windres" \
    -DCMAKE_BUILD_TYPE=Release -DCMAKE_INSTALL_PREFIX="$PREFIX" -DCMAKE_INSTALL_LIBDIR=lib \
    -DBUILD_SHARED_LIBS=OFF -DBUILD_TOOLS=OFF -DBUILD_EXAMPLES=OFF -DBUILD_TESTS=OFF -DINSTALL_EXAMPLES=OFF
  cmake --build "$WORK/libvpl/build" -j "$JOBS"
  cmake --install "$WORK/libvpl/build"
  # The dispatcher is C++: make sure static linking pulls in the C++ runtime too.
  sed -i 's/^Libs: \(.*\)$/Libs: \1 -lstdc++/' "$PREFIX/lib/pkgconfig/vpl.pc"
fi

# --- dav1d (BSD-2): AV1 decoding, for AV1 input files. ---
if [ ! -f "$PREFIX/lib/pkgconfig/dav1d.pc" ]; then
  fetch https://code.videolan.org/videolan/dav1d.git "$DAV1D_VERSION" dav1d
  meson setup "$WORK/dav1d/build" "$WORK/dav1d" \
    --cross-file "$WORK/dav1d/package/crossfiles/$HOST.meson" \
    --prefix="$PREFIX" --libdir=lib --buildtype=release --default-library=static \
    -Denable_tools=false -Denable_tests=false
  ninja -C "$WORK/dav1d/build"
  ninja -C "$WORK/dav1d/build" install
fi

# --- FFmpeg itself ---
fetch https://github.com/FFmpeg/FFmpeg.git "$FFMPEG_VERSION" ffmpeg
cd "$WORK/ffmpeg"

# Each list is only what Bonsai uses. Inputs are broad (people drop all kinds of files); outputs are minimal.
DEMUXERS="mov,matroska,avi,asf,flv,mpegts,mpegps,mpegvideo,h264,hevc,aac,mp3,wav"
DECODERS="h264,hevc,mpeg4,mpeg1video,mpeg2video,h263,msmpeg4v1,msmpeg4v2,msmpeg4v3,flv,wmv1,wmv2,wmv3,vc1"
DECODERS="$DECODERS,prores,mjpeg,vp8,vp9,libdav1d"
DECODERS="$DECODERS,aac,aac_latm,mp3float,mp2float,ac3,eac3,opus,vorbis,flac,alac,wmav1,wmav2,wmapro"
DECODERS="$DECODERS,pcm_s16le,pcm_s16be,pcm_s24le,pcm_s24be,pcm_s32le,pcm_f32le,pcm_u8,pcm_alaw,pcm_mulaw"
DECODERS="$DECODERS,adpcm_ima_wav,adpcm_ms"
# The lavfi input (`-f lavfi -i color=...`, used by encoder detection) hands over frames in this wrapper.
DECODERS="$DECODERS,wrapped_avframe"
PARSERS="h264,hevc,mpeg4video,mpegvideo,vc1,vp8,vp9,av1,aac,aac_latm,mpegaudio,ac3,opus,vorbis,flac"
ENCODERS="h264_nvenc,h264_qsv,h264_amf,h264_mf,aac,wrapped_avframe"
MUXERS="mp4,null"
BSFS="aac_adtstoasc,extract_extradata,h264_mp4toannexb,hevc_mp4toannexb,vp9_superframe"
# scale/fps are what Bonsai asks for; the rest are inserted by ffmpeg itself (format conversion, audio
# resampling for -ac 2) or used by encoder detection (color) and the tests (testsrc2, noise, sine).
FILTERS="scale,fps,format,null,anull,aformat,aresample,color,testsrc2,noise,sine"

CONFIGURE_FLAGS=(
  --target-os=mingw32 --arch=x86_64 --cross-prefix="$HOST-"
  # Same compiler flavour as libvpl (posix threads), so its C++ code links cleanly.
  --cc="$HOST-gcc-posix" --cxx="$HOST-g++-posix"
  --pkg-config=pkg-config --pkg-config-flags=--static
  --extra-cflags="-I$PREFIX/include" --extra-ldflags="-L$PREFIX/lib -static"
  --disable-everything --disable-autodetect --disable-gpl --disable-nonfree --disable-network
  --disable-doc --disable-ffplay --disable-debug --enable-static --disable-shared --enable-stripping
  --enable-ffmpeg --enable-ffprobe --enable-avdevice --enable-avfilter --enable-swscale --enable-swresample
  --enable-w32threads --enable-mediafoundation --enable-d3d11va --enable-dxva2
  --enable-ffnvcodec --enable-nvenc --enable-amf --enable-libvpl --enable-libdav1d
  --enable-protocol=file,pipe --enable-indev=lavfi
  --enable-demuxer="$DEMUXERS" --enable-decoder="$DECODERS" --enable-parser="$PARSERS"
  --enable-encoder="$ENCODERS" --enable-muxer="$MUXERS" --enable-bsf="$BSFS" --enable-filter="$FILTERS"
)
./configure "${CONFIGURE_FLAGS[@]}" || { tail -n 60 ffbuild/config.log; exit 1; }
make -j "$JOBS"

cp ffmpeg.exe ffprobe.exe "$DIST/"
"$HOST-strip" "$DIST/ffmpeg.exe" "$DIST/ffprobe.exe"

# Everything someone needs to get the same sources and rebuild this exact binary (LGPL requirement).
{
  echo "FFmpeg for Bonsai Video Compressor"
  echo "License: LGPL-2.1-or-later (no GPL or nonfree components enabled)"
  echo "Built by: https://github.com/flp2113/bonsai-video-compressor/blob/${GITHUB_SHA:-$(git -C "$ROOT" rev-parse HEAD 2>/dev/null || echo unknown)}/scripts/build-ffmpeg.sh"
  echo
  echo "Sources:"
  echo "  FFmpeg            $FFMPEG_VERSION  https://github.com/FFmpeg/FFmpeg/tree/$FFMPEG_VERSION"
  echo "  nv-codec-headers  $NV_CODEC_HEADERS_VERSION  https://github.com/FFmpeg/nv-codec-headers (MIT)"
  echo "  AMF headers       $AMF_VERSION  https://github.com/GPUOpen-LibrariesAndSDKs/AMF (MIT)"
  echo "  libvpl            $LIBVPL_VERSION  https://github.com/intel/libvpl (MIT)"
  echo "  dav1d             $DAV1D_VERSION  https://code.videolan.org/videolan/dav1d (BSD-2-Clause)"
  echo
  echo "Configure flags:"
  printf '  %s\n' "${CONFIGURE_FLAGS[@]}"
} > "$DIST/BUILD-INFO.txt"

ls -l "$DIST"
