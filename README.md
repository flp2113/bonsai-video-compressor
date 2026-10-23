<p align="center">
  <img src="src-tauri/icons/128x128.png" width="96" alt="Bonsai icon">
</p>

<h1 align="center">Bonsai Video Compressor</h1>

<p align="center">
  Shrink a video so it fits Discord, X or WhatsApp. Right-click it, pick where it's going, done.
</p>

<!-- Screenshot: add docs/screenshot.png and uncomment.
<p align="center"><img src="docs/screenshot.png" width="420" alt="Bonsai window"></p>
-->

## What it does

- **Fits, guaranteed.** Pick "Discord Free" and the result is under 20 MB. Bonsai works out the bitrate from the
  video's length, so there's no guessing and no second try.
- **Two ways in.** Drop a file on the window, or right-click one or more videos in Explorer →
  *Compress with Bonsai* → a destination. Several files go into a queue.
- **Fast.** Uses your graphics card's video encoder (NVIDIA, Intel or AMD), with Windows' own encoder as a fallback.
- **Ready to paste.** A single compressed file is copied to the clipboard, so you can paste it straight into
  Discord. A notification tells you when it's done if the window isn't in front.
- **Stays out of the way.** Lives in the tray and starts with Windows (you can turn that off from the tray menu).
- **Small and private.** An 11 MB installer. Everything happens on your PC; nothing is uploaded.

Output files are saved next to the original (`clip_discord.mp4`), or in a folder you choose from the tray menu.

## Presets

| Destination | Limit | Max resolution | Max frame rate |
|---|---|---|---|
| Discord Free | 20 MB | 1080p | 60 |
| Discord Nitro Basic | 50 MB | 1080p | 60 |
| Discord Nitro | 500 MB | 1080p | 60 |
| X / Twitter | 512 MB (warns above 2:20, the limit for free accounts) | 1080p | 60 |
| WhatsApp | 100 MB | 720p | 30 |

Every output is H.264 + AAC in MP4, which plays everywhere. Videos are never upscaled, and phone videos keep
their orientation.

## Install

Download `Bonsai_x.y.z_x64-setup.exe` from [Releases](https://github.com/flp2113/bonsai-video-compressor/releases)
and run it. It installs for your user only, without admin rights. Windows 10 and 11 (64-bit).

The installer isn't code-signed yet, so SmartScreen may show "Windows protected your PC". Click **More info** →
**Run anyway**. Each release lists SHA-256 checksums if you want to verify the download.

Uninstall from *Settings → Apps*. It also removes the right-click menu and the start-with-Windows entry.

## How it compares

| | Bonsai | HandBrake | Shutter Encoder | Online compressors | FFmpeg command line |
|---|---|---|---|---|---|
| Hit an exact size limit | Yes, built in | No target-size option; you calculate a bitrate yourself | You set the bitrate yourself | Yes | You calculate the bitrate |
| Steps to "fits Discord" | Right-click → preset | Open, pick preset, set bitrate, encode | Open, pick function and settings, encode | Upload, pick size, download | Write the command |
| GPU encoding | Yes, automatic | Yes, chosen in settings | Yes | Runs on their server | Yes, with the right flags |
| Files stay on your PC | Yes | Yes | Yes | No, the video is uploaded | Yes |
| Explorer right-click | Yes | No | No | No | No |
| Scope | One job: fit a destination | Full transcoder | Full converter with many tools | One job | Everything |

Bonsai deliberately does less. If you need filters, subtitles, trimming or other codecs, HandBrake and
Shutter Encoder are excellent tools.

## Build from source

Requirements: Windows 10/11, [Node.js](https://nodejs.org) 22+, [Rust](https://rustup.rs) (stable, MSVC), and
WebView2 (already part of Windows 11).

```powershell
./scripts/fetch-ffmpeg.ps1   # downloads our FFmpeg build into src-tauri/binaries/
npm ci
npm run tauri dev            # run with hot reload
npm run tauri build          # installer in src-tauri/target/release/bundle/nsis/
```

Tests:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml               # unit tests
cargo test --manifest-path src-tauri/Cargo.toml -- --ignored  # real encodes with FFmpeg (slow)
npm run check                                                 # Svelte / TypeScript
```

Project layout: the Rust side (`src-tauri/src/`) probes videos, plans the bitrate, runs FFmpeg, and handles the
queue, tray and right-click menu. The UI (`src/`) is SvelteKit with Svelte 5.

### FFmpeg

Bonsai ships its own small FFmpeg build (about 31 MB instead of about 250 MB), with only the formats and encoders
it uses. It's built by [`scripts/build-ffmpeg.sh`](scripts/build-ffmpeg.sh) in GitHub Actions and published as a
`ffmpeg-<version>-bonsai<N>` release. To change it, edit the script, run the **FFmpeg build** workflow with
*publish* ticked, and update the tag in `scripts/fetch-ffmpeg.ps1`.

The build must stay LGPL: never enable GPL or nonfree parts such as libx264 or libx265.

### Releasing

1. Set the same version in `src-tauri/tauri.conf.json`, `src-tauri/Cargo.toml` and `package.json`.
2. Push a tag `vX.Y.Z`. The **Release** workflow builds the installer and creates a draft release.
3. Check the draft on GitHub and publish it.

## Contributing

Issues and pull requests are welcome. Please keep changes small and in scope: a feature should help fit a video
to a destination. Commits follow [Conventional Commits](https://www.conventionalcommits.org)
(`feat(ui): ...`, `fix(encoder): ...`).

## License

[MIT](LICENSE). Bonsai bundles FFmpeg under the LGPL; see [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).
