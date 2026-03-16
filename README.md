# FlacCrunch

A cross-platform FLAC optimizer with a modern desktop UI and full CLI support. Losslessly recompresses FLAC files at maximum compression, verifies decoded-audio integrity via MD5, and optimizes embedded album art — all with native performance via statically linked libFLAC. No external binaries required.

Built with **Tauri v2** (Rust) and **React** (TypeScript). Runs on Windows, macOS, Linux, and Android.

> The original PowerShell script is preserved at the [`powershell-original`](../../tree/powershell-original) tag.

---

## Downloads

Get the latest build from [GitHub Releases](../../releases).

| Platform | File |
|----------|------|
| Windows (portable) | `FlacCrunch.exe` |
| Windows (installer) | `.msi` or NSIS `.exe` |
| macOS (Apple Silicon) | `.dmg` |
| Linux | `.deb` or `.AppImage` |
| Android | `.apk` |

---

## Features

- **Maximum FLAC compression** — re-encodes at level 8 with exhaustive model search (all encoder passes enabled)
- **Audio integrity verification** — MD5 hash of decoded PCM computed before and after; original replaced only if hashes match
- **Album art optimization** — PNG images processed with oxipng (lossless); JPEG images losslessly reoptimized by stripping non-essential metadata markers and rebuilding Huffman tables with optimal prefix codes (original quantization tables preserved — no pixel data is altered); PADDING blocks stripped
- **Multi-threaded worker pool** — configurable thread count, defaults to CPU count − 1
- **Live dashboard** — per-worker progress bars, live compression ratio, hash values with color-coded verification status
- **CLI mode** — pass `-silent` to skip the GUI entirely and print results to stdout
- **GUI pre-load** — pass paths on the command line to open the GUI with folders already loaded
- **Drag-and-drop** — drop folders or files directly onto the app window (desktop)
- **Export log** — save a full run log as a text file at any time after completion
- **Configurable retries** — failed files are retried up to N times (default 3)
- **Theme support** — light, dark, or follow system

---

## CLI Usage

```
FlacCrunch.exe [paths...] [options]

Options:
  -silent           No GUI, print progress to console
  -threads <N>      Worker thread count (default: CPU count - 1)
  -retries <N>      Max retries per file (default: 3)
  -logdir <path>    Log output directory
  -help             Show this help

Examples:
  FlacCrunch.exe "D:\Music"                         Open GUI with folder pre-loaded
  FlacCrunch.exe -silent "D:\Music"                 Process silently, console output only
  FlacCrunch.exe -silent -threads 4 "D:\Music" "E:\More"
```

On Linux/macOS the binary is named `flaccrunch`.

---

## Processing Pipeline

Each file is processed through five stages in order:

| Stage | What happens |
|-------|-------------|
| **1. Hashing source** | Reads embedded MD5 from FLAC STREAMINFO; decodes original audio and computes MD5 (PRE hash). Result emitted to UI immediately. |
| **2. Converting** | Re-encodes at FLAC level 8, exhaustive mode. Live progress % and compression ratio (output bytes / PCM bytes consumed) are emitted every 100 ms. All metadata blocks are re-applied after encode. |
| **3. Artwork** | Optimizes embedded PNG images (oxipng, lossless) and JPEG images (lossless: strips non-essential APP markers, then rebuilds Huffman tables with optimal prefix codes using the original DQT quantization tables — identical pixel output). Strips PADDING blocks. |
| **4. Hashing output** | Decodes the fully-assembled output file and computes MD5 (OUT hash). Verified against PRE. Mismatch → temp file deleted, file marked FAIL. |
| **5. Finalizing** | Atomically replaces the original file; restores filesystem timestamps. |

Files are only replaced when verification passes. Verification result codes:

| Code | Meaning |
|------|---------|
| `MATCH` | PRE == OUT (original had embedded MD5) |
| `MATCH\|NEW` | PRE == OUT (original had null embedded MD5 — newly written) |
| `MATCH\|EMB` | No PRE hash computed; OUT matched embedded MD5 |
| `FAIL\|MISMATCH` | PRE != OUT — audio changed during re-encode |
| `FAIL\|NO_SRC` | Could not compute PRE hash |
| `FAIL\|NO_HASH` | Hash computation failed entirely |

---

## UI Overview

### Run Status Bar
Shown while processing and after completion. Contains:
- Overall progress bar (live-blended: completed files + fractional in-flight worker progress)
- File count, elapsed time, byte savings breakdown (audio / art / meta)
- Cancel, New Run, and Export Log buttons

### Worker Grid
One card per worker thread. Each card shows:
- Current filename
- Encoding progress bar + percent (during converting)
- Live compression ratio inline with the last hash row (e.g. `≈0.433`)
- Three hash rows: `EMB` (original embedded MD5), `PRE` (pre-encode decoded audio), `OUT` (post-encode decoded audio)
- Hash values color-coded: **green** = verified match, **red** = mismatch, **yellow** = warning, **dim** = present but unverified
- After file completes: saved percent shown inline (e.g. `22.1%↓`)

### Top Compression
Top 3 files by total byte savings, sortable table, always shows 3 rows (placeholder dashes until populated).

### Recent Events Table
All processed files sorted by most recent. Columns: time, status (OK/FAIL/RETRY), file name, audio saved, art saved, total %, verification. Verification column is color-coded. Expandable to show all files.

### Folder Selector
- Desktop: drag-and-drop zone + "Add Folder" (recursive scan) + "Add Files" (individual FLACs)
- Mobile: tap to open native file picker
- Shows all selected paths with remove buttons

### Settings
Thread count, log output folder, max retries per file, UI theme.

---

## Building

### Prerequisites

- **Rust** 1.70+ stable (`rustup`)
- **Node.js** 22+
- **Linux only**: `libgtk-3-dev libwebkit2gtk-4.1-dev libjavascriptcoregtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev`
- **Android only**: Java 17, Android SDK, Android NDK 27.0.12077973

### Desktop

```bash
npm ci
npm run dev          # Dev server + hot reload (Tauri window on port 1420)
npm run tauri build  # Production build + installer
```

### Android

```bash
npm ci
npx tauri android init
npx tauri android build --apk
```

The APK requires these manifest permissions for full functionality: `READ_MEDIA_AUDIO`, `READ_EXTERNAL_STORAGE` (≤API 32), `WRITE_EXTERNAL_STORAGE` (≤API 29), `MANAGE_EXTERNAL_STORAGE`.

---

## Project Structure

```
flaccrunch/
├── src/                        # React frontend (TypeScript)
│   ├── components/
│   │   ├── folders/            # FolderSelector
│   │   ├── layout/             # AppShell, header
│   │   ├── processing/         # RunStatusBar, WorkerCard, WorkerGrid,
│   │   │                       # RecentEventsTable, TopCompression, OverallProgress
│   │   └── settings/           # SettingsPanel
│   ├── hooks/                  # useProcessing, useSettings, useWorkerStatus
│   ├── types/                  # TypeScript interfaces (processing, settings)
│   └── lib/                    # Tauri IPC wrapper, format utilities
├── src-tauri/
│   └── src/
│       ├── lib.rs              # Tauri plugin setup, IPC command registration
│       ├── main.rs             # Entry point: CLI detection, GUI launch
│       ├── cli.rs              # CLI mode (--silent processing)
│       ├── pipeline/
│       │   ├── job.rs          # Per-file pipeline: 5 stages, cancellation, retry
│       │   ├── worker_pool.rs  # Worker pool, event dispatch, Android write-back
│       │   ├── queue.rs        # Lock-free job queue with retry support
│       │   └── stages.rs       # PipelineEvent, PipelineStage, JobResult types
│       ├── flac/
│       │   ├── encoder.rs      # libFLAC level-8 encoder, live progress, ratio
│       │   ├── hasher.rs       # Decoded-audio MD5 via libFLAC decoder
│       │   └── metadata.rs     # FLAC metadata block copy, STREAMINFO MD5 read
│       ├── artwork/
│       │   └── optimize.rs     # PNG (oxipng) + JPEG (zune-jpeg/jpeg-encoder) lossless optimization
│       ├── image/              # PNG/JPEG detect, recompress helpers
│       ├── fs/
│       │   ├── scanner.rs      # Recursive FLAC scan, permission error collection
│       │   ├── tempfile.rs     # Atomic temp-file move/remove
│       │   └── metadata.rs     # Filesystem timestamp snapshot/restore
│       ├── commands/
│       │   ├── processing.rs   # start/cancel/status IPC commands
│       │   ├── folders.rs      # File/folder picker, Android content URI resolution
│       │   ├── settings.rs     # Settings load/save via tauri-plugin-store
│       │   └── logs.rs         # write_text_file command for export log
│       ├── state/
│       │   ├── app_state.rs    # Global Tauri state (active run, settings, URI map)
│       │   ├── run_state.rs    # Per-run counters, worker states, recent events
│       │   └── settings.rs     # AppSettings, ProcessingSettings structs
│       ├── logging/            # run_log, efc_log, failed_log
│       └── util/               # platform defaults, format helpers
├── .github/workflows/
│   ├── ci.yml                  # Push/PR: tsc, vite build, cargo test, cargo build
│   └── release.yml             # Tag/main push: desktop installers + Android APK
└── package.json
```

---

## CI/CD

**`ci.yml`** runs on every push and pull request to `main`:
- TypeScript type check (`tsc --noEmit`)
- Vite production build
- `cargo test`
- `cargo build`

**`release.yml`** runs on every push to `main` and on `v*` tags:
- Builds desktop installers for Windows (x86_64), macOS (aarch64), Linux (x86_64)
- Builds and signs Android APK for arm64, armv7, x86_64, x86
- On tag push: creates a GitHub Release and attaches all artifacts

---

## License

See repository for license information.
