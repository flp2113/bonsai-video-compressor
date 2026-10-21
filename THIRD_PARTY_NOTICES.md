# Third-party notices

Bonsai Video Compressor is MIT-licensed (see `LICENSE`). It ships the following third-party software.

## FFmpeg (`ffmpeg.exe`, `ffprobe.exe`)

- License: **GNU Lesser General Public License v2.1 or later** (LGPL-2.1-or-later). No GPL or nonfree
  components are enabled.
- Project: https://ffmpeg.org. Source: https://github.com/FFmpeg/FFmpeg
- Bonsai runs FFmpeg as separate programs; it does not link to it.
- How our copy is built: `scripts/build-ffmpeg.sh` in this repository. Each FFmpeg release of this repository
  (tags `ffmpeg-<version>-bonsai<N>`) includes `BUILD-INFO.txt`, which lists the exact FFmpeg version, the
  sources of every library below, and the configure flags, so the same binaries can be rebuilt.

Libraries compiled into that FFmpeg build:

| Library | Purpose | License |
|---|---|---|
| [dav1d](https://code.videolan.org/videolan/dav1d) | AV1 decoding | BSD-2-Clause |
| [libvpl](https://github.com/intel/libvpl) | Intel Quick Sync encoding | MIT |
| [AMF headers](https://github.com/GPUOpen-LibrariesAndSDKs/AMF) | AMD hardware encoding | MIT |
| [nv-codec-headers](https://github.com/FFmpeg/nv-codec-headers) | NVIDIA NVENC encoding | MIT |

The GPU encoders use the drivers already installed on the user's computer; no driver code is bundled.
