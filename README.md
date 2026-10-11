# FlacCrunch

A cross-platform FLAC optimizer with a desktop UI and a standalone headless CLI. Losslessly recompresses FLAC files at maximum compression, verifies decoded-audio integrity via MD5, and losslessly optimizes embedded album art — all with native performance via statically linked libFLAC and mozjpeg. No external binaries required.

Built with **Tauri v2** (Rust) and **React** (TypeScript). The GUI runs on Windows, macOS, Linux, and Android. The CLI (`flaccrunch-cli`) is a single binary with no GUI dependencies at all — it runs on a headless Linux box with no X server.

---

## Downloads

Get the latest build from [GitHub Releases](../../releases).

| Platform | File |
|----------|------|
| Windows (portable GUI) | `FlacCrunch.exe` |
| Windows (installer) | `.msi` |
| macOS (Apple Silicon) | `.dmg` |
| Linux (GUI) | `.deb` or `.AppImage` |
| **Linux (headless CLI, no X needed)** | `flaccrunch-cli-linux-x86_64` |
| Windows / macOS (headless CLI) | `flaccrunch-cli-windows-x86_64.exe`, `flaccrunch-cli-macos-arm64` |
| Android | `.apk` |

The Linux CLI binary needs glibc 2.35+ (Ubuntu 22.04, Debian 12, Fedora 36 or newer).

---

## Features

- **Maximum FLAC compression** — re-encodes at level 8 with exhaustive model search and exhaustive QLP coefficient precision search (equivalent to `flac -8 -e -p`)
- **Audio integrity verification** — MD5 of decoded PCM computed before and after re-encoding; original file replaced only on hash match
- **Lossless album art optimization** — PNG via oxipng; JPEG via metadata stripping plus coefficient-level Huffman re-optimization (the same transform as `jpegtran -optimize`, done with mozjpeg: the quantized DCT coefficients are rewritten with optimal Huffman tables and never decoded). Every result is verified to decode to identical pixels before it replaces the original
- **PADDING removal** — all PADDING metadata blocks stripped; savings tracked separately from artwork
- **Bad-file reporting** — a file that fails to decode or verify is left untouched and highlighted; the run summary lists the folders (albums) that contain bad files so they can be re-ripped
- **Full metadata preservation** — all Vorbis comments, PICTURE blocks, cue sheets, and application metadata copied to re-encoded output
- **Multi-threaded worker pool** — configurable thread count, defaults to CPU count − 1
- **Live dashboard** — per-worker progress with collapse/expand, drag-to-resize split between workers and file results, live hash verification display
- **In-app log viewer** — view the full run log in a modal window; also exportable as a text file
- **Headless CLI** — `flaccrunch-cli` has no Tauri/WebKit/GTK dependency; the GUI binary also accepts `-silent`
- **GUI pre-load** — pass paths on the command line to open the GUI with folders already loaded
- **Drag-and-drop** — drop folders or files directly onto the app window (desktop)
- **Configurable retries** — failed files automatically re-queued up to N times (default 3)

---

## CLI Usage

```
flaccrunch-cli [options] <path>...

  <path> may be a folder (scanned recursively) or a .flac file.

Options:
  -t, --threads <N>     Worker threads (default: CPU count - 1)
  -r, --retries <N>     Max retries per file (default: 3)
      --mark            Stamp output with a FLACCRUNCH_INFO marker tag
      --skip-crunched   Skip files that already carry the marker
      --log <file>      Write an EFC-format log to <file>
      --logdir <dir>    Write a timestamped EFC-format log into <dir>
  -n, --dry-run         Scan only: list what would be processed
  -q, --quiet           Only print the final summary
  -V, --version         Show version
  -h, --help            Show this help

Examples:
  flaccrunch-cli ~/Music
  flaccrunch-cli --threads 4 --mark --skip-crunched ~/Music /mnt/nas/More
  flaccrunch-cli --log run.log "D:\Music\Album"
```

Both `-flag` and `--flag` spellings work; `--` ends option parsing. Ctrl+C cancels cleanly (in-flight temp files are removed, originals untouched). The GUI binary accepts the same options plus `-silent` to run headless, e.g. `FlacCrunch.exe -silent "D:\Music"`; without `-silent` it opens the GUI with the paths pre-loaded.

**Exit codes:** `0` = all files successful, `1` = one or more failures, `2` = usage error or no FLAC files found, `130` = interrupted.

After the summary, the CLI prints **Folders with bad files**: every folder (usually one album) that contains a file that could not be decoded or verified, with a count, followed by the individual failures and reasons. Bad files are never modified.

---

## Processing Pipeline

Each file passes through five stages in order:

| Stage | What happens |
|-------|-------------|
| **1. Hashing source** | Reads the embedded MD5 from FLAC STREAMINFO. Decodes the original audio and computes a PRE hash (MD5 of raw PCM). Result emitted to UI immediately. |
| **2. Converting** | Streams the decoded audio straight into a libFLAC encoder at level 8 with exhaustive model search and exhaustive QLP precision search (memory use is bounded by one block, not the track length). Tags, pictures, application and cuesheet blocks are attached to the encoder up front, so the output is written in one pass. Live progress % and compression ratio emitted every ~100 ms. Non-FLAC prefix bytes (e.g. ID3v2 tags prepended by some tools) are preserved and re-attached. |
| **3. Artwork** | Done in memory on the metadata chain in a single pass. PNG images optimized with oxipng preset 4 (lossless). JPEG images: non-essential APP markers stripped (EXIF, ICC, XMP, COM), then the quantized DCT coefficients are rewritten with optimal Huffman tables by mozjpeg (`jpegtran -optimize`; baseline and progressive layouts tried, smaller kept). The result is only accepted if it decodes to exactly the same pixels. PADDING blocks removed. |
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

The UI uses a Win98-inspired desktop window aesthetic with a toolbar, fieldset panels, and a status bar.

### Toolbar
Shown at the top of the app window at all times:
- **Folder...** / **Files...** buttons — add folders or individual FLAC files to the queue (hidden during active processing)
- **Settings...** — opens the settings dialog
- Brand tagline on the right

### Idle Summary Bar
Shown when no run is active. Displays current queue count, thread count, retry limit, and verbose logging state at a glance.

### Run Status Bar
Shown during and after processing:
- Live-blended overall progress bar + percentage
- **Cancel** button during processing
- On completion: **Show Full Log** (opens in-app viewer), **Export Log** (save to file), **New Run**
- Status bar strip below the progress bar: total saved (bytes + %), file counter, elapsed time, OK/Fail counts, per-category savings (audio, artwork, metadata), success rate, artwork file count

### Worker Pane
A collapsible `fieldset` panel showing one card per worker thread. A summary line always shows worker state breakdown (e.g. `4 converting | 2 hashing | 1 idle`). Each card shows:
- Current filename and processing stage badge
- Encoding progress bar + live percentage
- Live compression ratio (output bytes ÷ PCM bytes consumed), e.g. `≈0.433`
- Three hash rows — `EMB` (original embedded MD5), `PRE` (pre-encode decoded audio), `OUT` (post-encode decoded audio)
- Hash color coding: **green** = verified match, **red** = mismatch, **yellow** = alternative verification, **dim** = present but unverified
- Saved percentage shown after file completes, e.g. `22.1%↓`
- **Collapse/Expand** toggle to hide cards and reclaim screen space

### Drag-to-Resize Split
A draggable divider between the Workers pane and the Files pane. Double-click to reset to 50/50. Drag to any ratio from 10/90 to 85/15.

### Files Pane
All processed files in a `fieldset` panel, sorted by savings % descending by default. Columns: time, status (OK / FAIL / RETRY), filename, audio saved, artwork saved, total savings %, verification code. Color-coded rows by compression tier. All columns sortable. Count shown in pane header.

### Log Viewer
Opens as a modal on "Show Full Log". Displays the full EFC summary log text. Maximize/restore toggle. Save button exports to file.

### Folder Selector (idle state)
- **Desktop:** drag-and-drop zones for folders and files, plus toolbar buttons
- **Android:** native file picker (folder picker unavailable on Android)
- Permission errors from scan reported inline

### Settings
Thread count, log output folder, max retries per file, verbose logging toggle.

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

The log also lists **Folders with bad files** (deduplicated, with counts) when any file failed, and an **EFC Savings** block with the audio / artwork / padding breakdown.

The CLI prints one line per file as it completes, a live progress line on stderr, and a full summary on completion; `--log` / `--logdir` write the same EFC log the GUI exports.

---

## Android

Content URIs from the native file picker are automatically resolved:
- **External storage documents** — resolved to real filesystem paths (e.g. `/storage/emulated/0/Music/`)
- **Media store URIs** — copied to app cache before processing; compressed output written back to the original URI on success

Required manifest permissions: `READ_MEDIA_AUDIO`, `READ_EXTERNAL_STORAGE` (≤ API 32), `WRITE_EXTERNAL_STORAGE` (≤ API 29), `MANAGE_EXTERNAL_STORAGE`.

---

## Building

### Prerequisites

- **Rust** stable (`rustup`), a C compiler and CMake (libFLAC and mozjpeg are built from source and linked statically)
- **Node.js** 22+ (GUI only)
- **Linux GUI only:** `libgtk-3-dev libwebkit2gtk-4.1-dev libjavascriptcoregtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev`
- **Android only:** Java 17, Android SDK, Android NDK 27.0.12077973

### Headless CLI (no GUI dependencies)

```bash
cd src-tauri
cargo build --release --no-default-features --bin flaccrunch-cli
# → target/release/flaccrunch-cli
```

The `gui` cargo feature (on by default) gates everything Tauri; with `--no-default-features` nothing from Tauri, WebKit or GTK is compiled or linked.

### Desktop GUI

```bash
npm ci
npm run dev          # Dev server + hot reload
npm run tauri build  # Production build + installer
```

### Tests

```bash
cd src-tauri
cargo test --no-default-features   # core (fast, no GTK needed)
cargo test                         # with the GUI feature set
cargo clippy --all-targets -- -D warnings
cd .. && npx tsc --noEmit && npx vitest run
```

Fixture-based tests re-encode `Tests/un-optimized.flac`; dependencies are compiled optimized even in the dev profile so this takes seconds rather than minutes.

### Android

```bash
npm ci
npx tauri android init
npx tauri android build --apk
```

`build.rs` now auto-applies the Android SAF patches during Android builds so local builds use the same writable file-picker behavior as CI/release builds.

---

## Project Structure

```
flaccrunch/
├── src/                          # React frontend (TypeScript)
│   ├── components/
│   │   ├── common/               # Badge, ProgressBar, ByteDisplay, ElapsedTimer
│   │   ├── folders/              # FolderSelector, drag-and-drop
│   │   ├── layout/               # AppShell, toolbar
│   │   ├── processing/           # RunStatusBar, WorkerCard, WorkerGrid,
│   │   │                         # ProcessingDashboard, RecentEventsTable,
│   │   │                         # LogViewerModal
│   │   └── settings/             # SettingsPanel
│   ├── hooks/                    # useProcessing, useSettings, useWorkerStatus
│   ├── types/                    # TypeScript interfaces
│   └── lib/                      # Tauri IPC wrappers, format utilities
├── src-tauri/
│   └── src/
│       ├── main.rs               # GUI binary entry: -silent detection, GUI launch
│       ├── bin/flaccrunch-cli.rs # Headless CLI binary (no `gui` feature)
│       ├── gui.rs                # Tauri app wiring (gui feature only)
│       ├── cli.rs                # CLI argument parsing, console output, run loop
│       ├── pipeline/
│       │   ├── job.rs            # Per-file 5-stage pipeline, cancellation
│       │   ├── worker_pool.rs    # Worker pool, retries, event dispatch (shared by GUI + CLI)
│       │   ├── sink.rs           # EventSink: GUI forwards to webview, CLI prints
│       │   ├── queue.rs          # Job queue with live append + retry support
│       │   └── stages.rs         # PipelineEvent, PipelineStage, JobResult types
│       ├── flac/
│       │   ├── encoder.rs        # Streaming libFLAC level-8 encoder, metadata attach, progress
│       │   ├── hasher.rs         # Decoded-audio MD5 via libFLAC decoder
│       │   ├── metadata.rs       # Chain/iterator RAII wrappers, block cloning, ID3 prefix
│       │   ├── marker.rs         # FLACCRUNCH_INFO marker read/write
│       │   └── ffi.rs            # libFLAC state/status strings, path helpers
│       ├── artwork/
│       │   └── optimize.rs       # In-memory picture optimization + padding removal
│       ├── image/                # PNG (oxipng) + JPEG (mozjpeg coefficient transcode)
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
- TypeScript type check, vitest, Vite production build
- `check-core` (Ubuntu + Windows, no GTK): clippy `-D warnings`, `cargo test`, release build and smoke test of `flaccrunch-cli`
- `check-gui` (Ubuntu): clippy, `cargo test`, `cargo build` with the GUI feature set

**`release.yml`** runs on `v*` tag pushes (and builds, without publishing, on `main`):
- Builds `flaccrunch-cli` for Linux x86_64, Windows x86_64 and macOS arm64 (`target-cpu=x86-64-v3` for portability)
- Builds desktop installers for Windows (x86_64), macOS (aarch64), Linux (x86_64)
- Builds and signs Android APK (arm64, armv7, x86_64, x86)
- Creates a GitHub Release and attaches all artifacts

---

## License

See repository for license information.
