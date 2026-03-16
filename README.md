# FlacCrunch

A cross-platform FLAC optimizer with a modern desktop UI and full CLI support. Losslessly recompresses FLAC files at maximum compression, verifies decoded-audio integrity via MD5, and optimizes embedded album art — all with native performance via statically linked libFLAC. No external binaries required.

Built with **Tauri v2** (Rust) and **React** (TypeScript). Runs on Windows, macOS, Linux, and Android.

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

- **Maximum FLAC compression** — re-encodes at level 8 with exhaustive model search and exhaustive QLP coefficient precision search (equivalent to `flac -8 -e -p`)
- **Audio integrity verification** — MD5 of decoded PCM computed before and after re-encoding; original file replaced only on hash match
- **Lossless album art optimization** — PNG via oxipng (lossless); JPEG via metadata stripping + Huffman table reoptimization (original quantization tables preserved — pixel data is bit-identical)
- **PADDING removal** — all PADDING metadata blocks stripped; savings tracked separately
- **Full metadata preservation** — all Vorbis comments, PICTURE blocks, cue sheets, and application metadata copied to re-encoded output
- **Multi-threaded worker pool** — configurable thread count, defaults to CPU count − 1
- **Live dashboard** — per-worker progress bars, live compression ratio, hash values with color-coded verification
- **CLI mode** — `-silent` flag skips the GUI entirely and prints results to stdout
- **GUI pre-load** — pass paths on the command line to open the GUI with folders already loaded
- **Drag-and-drop** — drop folders or files directly onto the app window (desktop)
- **Export log** — save a full run summary as a text file after completion
- **Configurable retries** — failed files automatically re-queued up to N times (default 3)

---

## CLI Usage

```
FlacCrunch.exe [paths...] [options]

Options:
  -silent              No GUI — process folders and print results to stdout
  -threads <N>         Worker thread count (default: CPU count - 1, min: 1)
  -retries <N>         Max retries per file (default: 3)
  -logdir <path>       Log output directory (default: platform log folder)
  -help                Show this help

Examples:
  FlacCrunch.exe "D:\Music"                              Open GUI with folder pre-loaded
  FlacCrunch.exe -silent "D:\Music"                      Process silently, console output
  FlacCrunch.exe -silent -threads 4 "D:\Music" "E:\More" Multiple folders, 4 threads
```

On Linux/macOS the binary is named `flaccrunch`. Paths can be directories (recursively scanned) or individual `.flac` files.

**Exit codes:** `0` = all files successful, `1` = one or more failures.

---

## Processing Pipeline

Each file passes through five stages in order:

| Stage | What happens |
|-------|-------------|
| **1. Hashing source** | Reads the embedded MD5 from FLAC STREAMINFO. Decodes the original audio and computes a PRE hash (MD5 of raw PCM). Result emitted to UI immediately. |
| **2. Converting** | Re-encodes audio at FLAC level 8 with exhaustive model search and exhaustive QLP precision search. Live progress % and compression ratio emitted every ~100 ms. All metadata blocks re-applied from the original after encoding. Non-FLAC prefix bytes (e.g. ID3v2 tags prepended by some tools) are extracted, preserved, and re-attached. |
| **3. Artwork** | PNG images optimized with oxipng preset 4 (lossless). JPEG images losslessly reoptimized: non-essential APP markers stripped (EXIF, ICC, XMP, COM), then Huffman tables rebuilt with optimal prefix codes using the original DQT quantization tables — equivalent to `jpegtran -optimize`, pixel data is bit-identical. PADDING blocks removed. |
| **4. Hashing output** | Decodes the fully assembled output file and computes an OUT hash. Verified against PRE. On mismatch: temp file deleted, file marked FAIL. |
| **5. Finalizing** | Atomically replaces the original file. Restores original filesystem timestamps. |

Files are only replaced when verification passes. The live progress bar blends completed files with fractional in-flight worker progress so it never jumps backward.

---

## Verification Result Codes

| Code | Meaning |
|------|---------|
| `MATCH` | PRE == OUT; original had an embedded MD5 |
| `MATCH\|NEW` | PRE == OUT; original had no embedded MD5 (newly written by re-encode) |
| `MATCH\|EMB` | PRE unavailable; OUT matched the embedded MD5 |
| `FAIL\|MISMATCH` | PRE ≠ OUT — audio changed during re-encode (first 8 hex chars shown) |
| `FAIL\|NO_SRC` | PRE hash unavailable and no embedded MD5 to fall back on |
| `FAIL\|NO_HASH` | Hash computation failed entirely |

---

## UI Overview

### Run Status Bar
Shown during and after processing:
- Live-blended overall progress bar + percentage
- File counter (`X / Y files`), elapsed time
- Byte savings breakdown: audio saved, artwork saved, metadata saved
- **Cancel** button during processing; **New Run** and **Export Log** buttons on completion

### Worker Grid
One card per worker thread. Each card shows:
- Current filename and processing stage
- Encoding progress bar + live percentage
- Live compression ratio (output bytes ÷ PCM bytes consumed), e.g. `≈0.433`
- Three hash rows — `EMB` (original embedded MD5), `PRE` (pre-encode decoded audio), `OUT` (post-encode decoded audio)
- Hash color coding: **green** = verified match, **red** = mismatch, **yellow** = alternative verification, **dim** = present but unverified
- Saved percentage shown after file completes, e.g. `22.1%↓`

### Top Compression
Top 3 files by total byte savings (audio + artwork), updated live. Shows filename, bytes saved, and savings percentage. Placeholder rows shown until results arrive.

### Recent Events Table
All processed files, newest first. Columns: time, status (OK / FAIL / RETRY), filename, audio saved, artwork saved, total savings %, verification code. Color-coded by verification result. Expandable to show full history.

### Folder Selector
- **Desktop:** drag-and-drop zone + Add Folder (recursive) + Add Files (individual FLACs)
- **Android:** native file picker (folder picker unavailable on Android)
- Shows all selected paths with individual remove buttons
- Permission errors from scan reported inline

### Settings
Thread count, log output folder, max retries per file.

---

## Scanning

- Recursive scan with symlink following
- Case-insensitive `.flac` extension matching
- Deduplication by canonical path
- Files sorted by size descending before processing (larger files first)
- Permission errors collected and displayed in the UI without halting the scan
- Stale `.tmp` files cleaned up before each scan

---

## Logging

Each run creates a timestamped directory under the configured log folder:

```
logs/
  run_YYYYMMDD_HHMMSS/
    run.log        # Per-file results appended as files complete
    summary.log    # Full EFC summary generated at end of run
    failed.log     # Failed files only (if any failures occurred)
```

The EFC summary includes: per-file results, total/success/fail counts, elapsed time, full savings breakdown (audio / artwork raw / artwork net / metadata / padding), success rate, average savings, top 3 compression results, and a SHA-256 checksum of the body for integrity verification.

CLI mode prints progress every 10 files and a full summary on completion.

---

## Android

Content URIs from the native file picker are automatically resolved:
- **External storage documents** — resolved to real filesystem paths (e.g. `/storage/emulated/0/Music/`)
- **Media store URIs** — copied to app cache before processing; compressed output written back to the original URI on success

Required manifest permissions: `READ_MEDIA_AUDIO`, `READ_EXTERNAL_STORAGE` (≤ API 32), `WRITE_EXTERNAL_STORAGE` (≤ API 29), `MANAGE_EXTERNAL_STORAGE`.

---

## Building

### Prerequisites

- **Rust** stable (`rustup`)
- **Node.js** 22+
- **Linux only:** `libgtk-3-dev libwebkit2gtk-4.1-dev libjavascriptcoregtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev`
- **Android only:** Java 17, Android SDK, Android NDK 27.0.12077973

### Desktop

```bash
npm ci
npm run dev          # Dev server + hot reload
npm run tauri build  # Production build + installer
```

### Android

```bash
npm ci
npx tauri android init
npx tauri android build --apk
```

---

## Project Structure

```
flaccrunch/
├── src/                          # React frontend (TypeScript)
│   ├── components/
│   │   ├── common/               # Badge, ProgressBar, ByteDisplay, ElapsedTimer
│   │   ├── folders/              # FolderSelector, drag-and-drop
│   │   ├── layout/               # AppShell, header
│   │   ├── processing/           # RunStatusBar, WorkerCard, WorkerGrid,
│   │   │                         # RecentEventsTable, TopCompression
│   │   └── settings/             # SettingsPanel
│   ├── hooks/                    # useProcessing, useSettings, useWorkerStatus
│   ├── types/                    # TypeScript interfaces
│   └── lib/                      # Tauri IPC wrappers, format utilities
├── src-tauri/
│   └── src/
│       ├── main.rs               # Entry point: CLI detection, GUI launch
│       ├── cli.rs                # CLI mode — silent processing, stdout output
│       ├── pipeline/
│       │   ├── job.rs            # Per-file 5-stage pipeline, cancellation, retry
│       │   ├── worker_pool.rs    # Worker pool, event dispatch, Android write-back
│       │   ├── queue.rs          # Lock-free job queue with retry support
│       │   └── stages.rs         # PipelineEvent, PipelineStage, JobResult types
│       ├── flac/
│       │   ├── encoder.rs        # libFLAC level-8 encoder, live progress, ratio
│       │   ├── hasher.rs         # Decoded-audio MD5 via libFLAC decoder
│       │   └── metadata.rs       # Metadata block copy, STREAMINFO MD5 read
│       ├── artwork/
│       │   └── optimize.rs       # PNG (oxipng) + JPEG (lossless Huffman) optimization
│       ├── image/                 # PNG/JPEG format detection, compression helpers
│       ├── fs/
│       │   ├── scanner.rs        # Recursive FLAC scan, dedup, permission errors
│       │   ├── tempfile.rs       # Atomic temp-file move/remove
│       │   └── metadata.rs       # Filesystem timestamp snapshot/restore
│       ├── commands/
│       │   ├── processing.rs     # start/cancel/status IPC commands
│       │   ├── folders.rs        # File/folder picker, Android URI resolution
│       │   ├── settings.rs       # Settings load/save
│       │   └── logs.rs           # Export log command
│       ├── state/
│       │   ├── app_state.rs      # Global Tauri state
│       │   ├── run_state.rs      # Per-run counters, worker states, recent events
│       │   └── settings.rs       # AppSettings, ProcessingSettings structs
│       └── logging/              # run_log, efc_log (summary), failed_log
├── .github/workflows/
│   ├── ci.yml                    # Push/PR: tsc, vite build, cargo test, cargo build
│   └── release.yml               # Tag: desktop installers + Android APK + GitHub Release
└── package.json
```

---

## CI/CD

**`ci.yml`** runs on every push and pull request to `main`:
- TypeScript type check (`tsc --noEmit`)
- Vite production build
- `cargo test`
- `cargo build`

**`release.yml`** runs on `v*` tag pushes:
- Builds desktop installers for Windows (x86_64), macOS (aarch64), Linux (x86_64)
- Builds and signs Android APK (arm64, armv7, x86_64, x86)
- Creates a GitHub Release and attaches all artifacts

---

## License

See repository for license information.
