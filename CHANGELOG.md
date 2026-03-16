# Changelog

## v1.0.2 — 2026-03-15

### Changed

- **Logging overhaul**: The app no longer writes logs to disk automatically on every run
  - Export Log button now produces an EFC/EAC-style structured log (matches the format previously written by the backend)
  - On a run with failures, a save-log dialog is still prompted automatically
  - Scratch files now use the system temp directory instead of the log folder
- **Settings — Verbose Logging toggle** (default off): when enabled, an EFC log is automatically written to disk after each run to a configurable folder (defaults to the desktop)

### Fixed

- TypeScript type errors in test files that caused CI builds to fail after v1.0.1

---

## v1.0.0 — 2026-03-15

First production release. Complete rewrite from the original PowerShell script into a native cross-platform desktop + Android application built with Tauri v2 (Rust) and React (TypeScript).

---

### New: Native libFLAC encoding pipeline

The original app shelled out to `flac.exe` and `metaflac.exe` sidecars. Those are gone entirely. All FLAC encoding and metadata handling is done in-process via `libflac-sys` (statically linked). This means:

- No external binaries to bundle, download, or version-manage
- Works on Android (no external process spawning possible there)
- Faster startup, no subprocess overhead

### New: GUI desktop application

Full Tauri v2 + React UI with a single-screen layout:

- **Run status bar** — overall progress bar with live blended worker progress, elapsed time, byte savings breakdown (audio / art / meta), cancel and new-run controls, export log button
- **Worker grid** — one card per worker showing current file, encoding % with progress bar, live compression ratio (≈0.433), and the three hash values (EMB, PRE, OUT) color-coded: green = verified, red = mismatch, yellow = warning
- **Top compression** — top 3 files by total savings, sortable table with placeholder rows
- **Recent events table** — all processed files with time, status, file name, audio saved, art saved, total %, and color-coded verification result (MATCH / MATCH|NEW / MISMATCH / FAIL)
- **Folder selector** — add folders (recursive scan) or individual FLAC files; drag-and-drop from Explorer/Finder; mobile: tap to pick files
- **Settings panel** — thread count, log folder, max retries per file

Progress bar uses live blended weighting: completed files + fractional in-flight worker contribution. Post-encoding stages (hashing output, artwork, finalizing) are weighted at 1.0 to prevent the bar jumping backward.

### New: CLI mode

`FlacCrunch.exe` doubles as a command-line tool. Passing `-silent` skips the GUI entirely and prints results to stdout. Passing paths without `-silent` opens the GUI with those folders pre-loaded.

```
FlacCrunch.exe [paths...] [options]

Options:
  -silent           No GUI, print progress to console
  -threads <N>      Worker thread count (default: CPU count - 1)
  -retries <N>      Max retries per file (default: 3)
  -logdir <path>    Log output directory
  -help             Show this help

Examples:
  FlacCrunch.exe "D:\Music"
  FlacCrunch.exe -silent -threads 4 "D:\Music" "E:\More"
```

### New: Android support

The app builds and runs on Android (API 24+, tested on Pixel 9a / API 35):

- File picker via the native Android file chooser
- External storage URIs (`content://com.android.externalstorage.documents/...`) resolved directly to real filesystem paths
- Media store URIs (`msf:XXXX`) copied to app cache before processing; compressed result written back to original content URI after success
- Manifest permissions: `READ_MEDIA_AUDIO`, `READ_EXTERNAL_STORAGE`, `WRITE_EXTERNAL_STORAGE`, `MANAGE_EXTERNAL_STORAGE`
- Log directory uses app cache dir on mobile (the system user dirs path is read-only)

### Processing pipeline (per file)

Each file goes through five stages in this order:

| # | Stage | What happens |
|---|-------|-------------|
| 1 | **Hashing source** | Read embedded MD5 from STREAMINFO; decode original audio and compute MD5 (PRE hash). Emits live to UI. |
| 2 | **Converting** | Re-encode at FLAC level 8 with exhaustive model search (all encoder passes). Live progress % and compression ratio (output size / PCM bytes consumed) emitted every 100 ms. Metadata blocks re-applied after encode. |
| 3 | **Artwork** | Optimize embedded PNG images with oxipng (lossless). Losslessly reoptimize JPEG images: strip non-essential APP markers, then rebuild Huffman tables with optimal prefix codes using the original DQT quantization tables (pixel data bit-identical). Strip PADDING blocks. |
| 4 | **Hashing output** | Decode final output file and compute MD5 (OUT hash). Verified against PRE. If mismatch: temp file deleted, file marked FAIL. |
| 5 | **Finalizing** | Atomic replace of original file, filesystem timestamp restore. |

Verification result strings: `MATCH` (PRE==OUT, original had embedded MD5), `MATCH|NEW` (PRE==OUT, original had null embedded MD5), `MATCH|EMB` (no PRE hash, but OUT==EMB), `FAIL|MISMATCH`, `FAIL|NO_SRC`, `FAIL|NO_HASH`.

### New: Artwork optimization

Embedded images in FLAC PICTURE blocks are optimized in-place:

- **PNG**: passed through oxipng (lossless, preset 4)
- **JPEG**: non-essential APP markers stripped (EXIF, ICC, XMP, COM), then Huffman tables rebuilt with optimal prefix codes using the original DQT quantization tables — equivalent to `jpegtran -optimize`, pixel data is bit-identical
- **PADDING blocks**: stripped from all files (savings tracked separately)
- Savings reported as: raw bytes removed + net bytes saved after re-import

### Build and release artifacts

| Platform | Artifacts |
|----------|-----------|
| Windows | `FlacCrunch.exe` portable, `.msi` installer, NSIS `.exe` setup |
| macOS (ARM) | `.dmg` disk image |
| Linux | `.deb` package, `.AppImage` |
| Android | Signed `.apk` (arm64, armv7, x86_64, x86) |

All built by GitHub Actions on every push to `main` and on `v*` tags. Tagged releases are published to GitHub Releases automatically.

### Bug fixes in this release

- Workers not showing status: serde field name mismatch between Rust and TypeScript (`snake_case` vs `camelCase`)
- Progress bar jumping backward: post-encoding stages (hashing output, artwork, finalizing) reset worker `percent` to 0, causing `livePct` to drop. Fixed by weighting those stages at 1.0 instead of `percent/100`
- Worker card showing `22% 22%` ratio: ratio was incorrectly formatted as the encoding percent. Fixed by computing actual output-file-size / PCM-consumed ratio during encoding
- Android log crash (`Read-only file system, os error 30`): `directories::UserDirs` returns a read-only path on Android. Fixed by using `app.path().app_cache_dir()` on mobile
- Android file names showing as `msf%3A7247.flac`: URL-encoded media store IDs. Fixed by percent-decoding URIs and mapping external storage URIs to real paths
- Folder picker erroneously shown on mobile: mobile now shows only the file picker
- Export log button invisible: was `btn-ghost` (no border). Changed to `btn-secondary`

---

> The original PowerShell script is preserved at the [`powershell-original`](../../tree/powershell-original) tag.
