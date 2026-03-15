# FlacCrunch-ng

A cross-platform FLAC optimizer with a modern desktop UI. Losslessly recompresses FLAC files at maximum compression, verifies decoded audio integrity, and optimizes embedded album art — all with native performance via libFLAC.

Built with **Tauri v2** (Rust) and **React** (TypeScript).

> The original PowerShell script version is preserved under the [`powershell-original`](../../tree/powershell-original) tag.

## Features

- **Maximum FLAC compression** — re-encodes at level 8 with exhaustive model search
- **Audio integrity verification** — MD5 hash comparison of decoded audio before and after; originals are only replaced on match
- **Album art optimization** — PNG (oxipng) and JPEG (jpegtran) compression, PADDING block removal
- **Multi-threaded processing** — configurable worker pool (default: CPU count - 1)
- **Live dashboard** — real-time worker status, compression stats, and file event feed
- **Comprehensive logging** — run log, EFC-style summary, and failed-file tracking
- **Light/Dark/System theme** support

## Supported Platforms

| Platform | Architecture | Artifact |
|----------|-------------|----------|
| Windows  | x86_64      | `.msi`, `.exe` |
| Linux    | x86_64      | `.deb`, `.AppImage` |
| macOS    | ARM64       | `.dmg` |
| Android  | arm64, armv7, x86_64, i686 | `.apk` |

All platforms compile libFLAC natively — no external `flac` binary required.

## Building

### Prerequisites

- **Rust** 1.70+ (stable)
- **Node.js** 22+
- **Linux only**: `libgtk-3-dev`, `libwebkit2gtk-4.1-dev`, `libjavascriptcoregtk-4.1-dev`, `libayatana-appindicator3-dev`, `librsvg2-dev`
- **Android only**: Java 17, Android SDK, Android NDK 27.0

### Commands

```bash
# Install JS dependencies
npm ci

# Development (hot-reload on port 1420)
npm run dev

# Production desktop build
npm run tauri build

# Android APK
npm run tauri android init
npm run tauri android build --apk
```

## Processing Pipeline

Each file goes through five stages:

1. **Converting** — decode source FLAC, re-encode at level 8 exhaustive
2. **Hashing source** — MD5 of original decoded audio
3. **Hashing output** — MD5 of recompressed decoded audio
4. **Artwork** — optimize embedded PNG/JPEG, strip PADDING blocks
5. **Finalizing** — restore metadata, replace original, clean up temp files

Files are only replaced when all hash checks pass.

## UI Overview

- **Folder Selector** — add/remove target folders, start/cancel/reset runs
- **Overall Progress** — file count and completion percentage
- **Stats Bar** — live byte savings breakdown (FLAC, metadata, artwork, padding)
- **Worker Grid** — per-worker status and current file
- **Recent Events** — last 25 processed files with status and savings
- **Top Compression** — top 3 files by byte savings
- **Settings** — thread count, log folder, max retries, theme

## CI/CD

GitHub Actions workflows in `.github/workflows/`:

- **ci.yml** — runs on push/PR to `main`: TypeScript check, Vite build, `cargo test`, `cargo build`
- **release.yml** — triggers on `v*` tags or manual dispatch: builds desktop installers (Windows, Linux, macOS) and signed Android APKs, uploads as GitHub release assets

## Project Structure

```
flaccrunch-ng/
├── src/                    # React frontend
│   ├── components/         # UI components (folders, processing, settings)
│   ├── hooks/              # useProcessing, useSettings, etc.
│   ├── types/              # TypeScript interfaces
│   └── lib/                # Tauri API wrapper
├── src-tauri/
│   ├── src/
│   │   ├── lib.rs          # Tauri setup and IPC command handlers
│   │   ├── pipeline/       # Job queue, worker pool, processing stages
│   │   ├── flac/           # Encoding, hashing, metadata
│   │   ├── artwork/        # Album art optimization
│   │   ├── fs/             # File scanning, temp handling
│   │   ├── logging/        # Run log, EFC log, failed log
│   │   └── state/          # App state and settings
│   ├── Cargo.toml          # Rust dependencies
│   └── tauri.conf.json     # Tauri configuration
├── .github/workflows/      # CI and release pipelines
└── package.json
```

## Development

```bash
# Run frontend type check
npx tsc --noEmit

# Run backend tests
cd src-tauri && cargo test

# Run frontend dev server only
npm run dev
```

## License

See repository for license information.
