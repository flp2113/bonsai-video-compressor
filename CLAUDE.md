# Bonsai Video Compressor

Open-source (MIT), ultra-lightweight, Windows-first video compressor. One job: compress a video
to fit a destination preset (Discord Free 10 MB, Nitro Basic 50 MB, Nitro 500 MB, X, Instagram, YouTube).
Started from the system tray drop window or the Explorer right-click menu.

## Stack
- Tauri 2 (Rust, `src-tauri/`) + SvelteKit/Svelte 5 + TypeScript (`src/`, static adapter, no SSR)
- FFmpeg/ffprobe shipped as Tauri sidecars (target: custom minimal **LGPL** build — never add GPL
  encoders like libx264/libx265; use NVENC/QSV/AMF with `h264_mf` fallback)

## Commands
- `./scripts/fetch-ffmpeg.ps1` — first-time setup: downloads LGPL ffmpeg/ffprobe into `src-tauri/binaries/` (gitignored)
- `npm run tauri dev` — run app with hot reload
- `npm run check` — Svelte/TS type check
- `cargo test --manifest-path src-tauri/Cargo.toml` — Rust unit tests
- `cargo test --manifest-path src-tauri/Cargo.toml -- --ignored` — slow real-ffmpeg encode test (needs sidecars)
- `npm run tauri build` — release installer

## Git workflow
- Commit when each milestone is complete (milestones listed in the project plan); push once a remote exists.
- Conventional Commits: `type(scope): imperative summary` (≤72 chars), types `feat|fix|refactor|test|docs|build|chore|ci`,
  scopes like `probe`, `planner`, `encoder`, `ui`, `tray`, `shell-menu`, `ffmpeg`, `deps`.
- Body: short bullets of what/why. Footer: `Milestone: N` line.
- Never add `Co-Authored-By: Claude` or any AI attribution to commits or PRs.
- Before committing: `cargo test`, `npm run check`, `cargo build` must pass; never commit `src-tauri/binaries/*.exe`.

## Conventions
- Keep scope minimal: no feature without a clear preset/compression purpose.
- Heavy logic lives in Rust; pure functions (e.g. bitrate planner) get unit tests.
- The maintainer is new to Rust: prefer clear, idiomatic, well-commented Rust over clever code.
