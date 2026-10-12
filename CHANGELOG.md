# Changelog

## v1.5.0 — 2026-10-11

### Fixed: Android layout under the status bar and camera cutout

- On Android 15 (e.g. Pixel 9a) apps are drawn edge-to-edge, so the toolbar sat under the status bar and the camera cutout and could not be tapped. The app now keeps its content clear of the status bar, display cutout, gesture bar and on-screen keyboard, and tints the system bars to match the active theme (light or dark icons as needed). This applies to every theme.

### New: Material theme (default on Android)

- A Material 3 / Material You look built for phones: top app bar with a settings icon, large **Add folder** / **Add files** buttons, the queue as a list with remove buttons, a big **Start** button pinned above the gesture bar, a single scrolling page while running, results as two-line rows with sort chips, full-screen Settings with switches.
- Uses your wallpaper colours on Android 12+ and follows the system light/dark setting. Selectable on desktop too, where it keeps the panel layout.
- New Android installs start on Material. If a theme was already saved, pick **Settings → Theme → Material**.

### Changed: simpler command-line switches

- The CLI now has just: `-t/--threads`, `-r/--retries`, `--max`, `--tag`, `--skip-tagged`, `--log <file or folder>`, `-n/--dry-run`, `-q/--quiet`, `-V/--version`, `-h/--help` (plus `--silent` on the GUI binary).
- Renamed so tagging and skipping use the same word: `--mark` → `--tag`, `--skip-crunched` → `--skip-tagged`. `--logdir` is folded into `--log`: give it a folder and a timestamped log is created inside.
- Removed the libFLAC tuning switches (`--lpc-order`, `--blocksize`, `--apodization`, `--rice-order`, `--encoder-threads`) and the extra aliases (`-j`, `--lax`, `--skip`, `--logfile`, …). `--max` is the only compression switch. Old names stop with an error that says what to use instead.

### Changed

- Settings: clearer description of **Maximum compression**.

---

## v1.4.1 — 2026-10-11

### Changed: display settings are saved with the other settings

- The display theme is now saved in `settings.json` with every other setting, instead of in the app's web view storage.
- New **Settings → Show quick switch menu** checkbox (off by default). The floating theme menu in the bottom-left corner and its Ctrl+Shift+T shortcut only work when it is on.

---

## v1.4.0 — 2026-10-11

### New: display themes

- Thirteen switchable looks: **Basic (Dark)** (new default), Basic, Wizard, Windows 95, Winamp, Matrix, Flash Game, Arcade, Casino, Comic Book, DOS, Game Boy and LimeWire. Each theme has its own layout and chrome, not just a colour swap. The choice is saved per machine and applied before first paint, so the app doesn't flash the wrong theme on launch.
- Switch from **Settings → Theme**, or with the floating theme menu in the bottom-left corner (Ctrl+Shift+T toggles it, Escape closes it). Collapsed, the menu is a single small dot.
- Plain themes (Basic, Basic (Dark), Wizard) show workers as a sortable table; the rest use themed cards.
- The workers pane fits its content by default; drag the divider to resize it, double-click to fit it again.
- 98.css is now scoped to the Windows 95 theme at build time, so the other themes start from unstyled elements.

---

## v1.3.2 — 2026-10-11

### New: settings are remembered

- Thread count, retries, logging, marker, skip and **Maximum compression** are saved automatically to `settings.json` in the app's config directory (`%APPDATA%\com.flaccrunch.app\` on Windows, `~/.config/com.flaccrunch.app/` on Linux, `~/Library/Application Support/com.flaccrunch.app/` on macOS, app data on Android) and loaded at startup. Previously every launch reset them. The Settings dialog shows where the file lives; a write failure is shown there too and the session still uses the new values.

---

## v1.3.1 — 2026-10-11

### New: maximum (non-subset) compression mode

- `--max` (CLI) / **Settings → Maximum compression** (GUI): `flac -8 -e -p --lax -l 32 -r 8`. Prediction order 32 is what release-group WEBFLACs typically use, which is why the subset default could not shrink them. Measured on such a track: 1.1 % smaller than the source at ~13× the encode time (5.5 min vs 26 s for 2:15 of 24-bit audio). Off by default: non-subset streams are refused by some older hardware players. Audio and metadata verification apply exactly as before.
- Advanced CLI knobs: `--lpc-order`, `--blocksize`, `--apodization`, `--rice-order`, `--encoder-threads`. Block size 8192 and `subdivide_tukey(5)` were benchmarked and are *not* in the preset (the former loses, the latter costs 4× the time for 0.1 %).
- libFLAC 1.5 per-file multithreading: each worker gets `cores ÷ workers` encoder threads so short queues still use the whole CPU (Linux/macOS builds; the MSVC build of libFLAC has no pthreads and stays single-threaded per file).

---

## v1.3.0 — 2026-10-11

Strictly lossless, and proven per file. Supersedes v1.2.0, which rejected files whose tags libFLAC's encoder considered malformed.

### Fixed

- **v1.2.0 regression: `FLAC__STREAM_ENCODER_INIT_STATUS_INVALID_METADATA` on whole albums.** v1.2.0 handed tags and pictures to the libFLAC encoder at init; libFLAC validates them (UTF-8 values, tag-name characters, one VORBIS_COMMENT block, picture header fields) and refused anything a careless tagger had written. Tags, pictures, application and cuesheet blocks are again cloned into the output with the metadata-chain API, which validates nothing and keeps every byte. A test plants deliberately illegal tags and checks they survive.
- The encoder-attached path also overwrote the Vorbis **vendor string**; it is now preserved verbatim. The encoder's own empty VORBIS_COMMENT is replaced in place, so the output has exactly one comment block in the same position as the source.

### Changed: everything non-audio is byte-identical and verified

- **JPEG artwork keeps all metadata.** The APP/COM-segment stripping (EXIF, XMP, ICC, comments) that both the original app and v1.2.0 performed is gone: only the Huffman coding is rebuilt (mozjpeg, `jpegtran -optimize`), and every segment is copied over. ICC profiles are therefore never discarded.
- **PNG artwork keeps its stored pixel values.** oxipng's colour-type / bit-depth / palette reductions and alpha optimization are disabled (they are visually lossless but change stored values); all ancillary chunks are kept. Only the compression changes.
- **Post-encode equivalence check** (`FAIL|METADATA` on mismatch, original kept): tags byte-for-byte incl. vendor string, pictures (header fields, decoded pixels, embedded segments), application and cuesheet blocks, stream parameters, ID3v2 prefix. Audio is covered by the existing PRE/OUT MD5 check.
- **SEEKTABLE is regenerated** (one point per 10 s, as the `flac` tool does) instead of silently dropped as since v1.0.0. The source table cannot be copied because its byte offsets refer to the old encoding.
- PADDING is still removed (zero bytes, no information).
- **A re-encode that is not smaller no longer replaces the original.** Sources encoded non-subset (LPC order up to 32, e.g. `flac --lax -l 32` or ffmpeg's top level) beat subset `-8 -e -p`; such files are reported `MATCH|KEPT` with the size difference and left byte-for-byte untouched.

### Tests

- A synthetic FLAC fixture (tags, vendor string, PNG + JPEG pictures with metadata segments, application block, padding) generated in milliseconds now drives the pipeline, CLI, verifier and encoder tests; the 45 MB fixture is used by one roundtrip test only. The core suite runs in under two minutes.

---

## v1.2.0 — 2026-10-11

Full repass and refactor of the Rust core and the React hooks.

### New: headless `flaccrunch-cli`

- Separate binary with **no Tauri / WebKit / GTK dependency** — runs on a Linux box with no X server. Built with `cargo build --release --no-default-features --bin flaccrunch-cli` (≈1.7 MB on Windows). Published for Linux x86_64, Windows x86_64 and macOS arm64 by the release workflow.
- Options: `--threads`, `--retries`, `--mark`, `--skip-crunched`, `--log <file>`, `--logdir <dir>`, `--dry-run`, `--quiet`, `--version`; `-flag` and `--flag` spellings both accepted; `--` ends options. Unknown options are errors instead of being treated as paths.
- Writes a real EFC log when asked (`-logdir` previously created an empty `run_*/scratch` directory and no log).
- Ctrl+C cancels cleanly: in-flight temp files are removed, originals untouched, summary printed, exit code 130.
- Exit codes: 0 success, 1 failures, 2 usage error / no FLAC files.
- The GUI binary still honours `-silent` and shares the same code.

### New: bad-file reporting

- A file that cannot be decoded or does not verify is the most useful thing a run can tell you. Failed rows are now highlighted as a whole (with the reason as a tooltip), the Files pane shows a **Folders with bad files** panel (deduplicated parent folders — usually one album each — with counts), and the same list appears in the EFC log and at the end of the CLI output together with each failure and its reason. Bad files are never modified.

### Fixed (behaviour)

- **Retries were counted as failures.** A file that failed once and succeeded on retry showed up as both FAIL and OK, inflated `processed` past `totalFiles`, and gave exit code 1 in CLI mode. First attempts are now reported as `RETRY` (not counted as processed) and only the final attempt counts.
- **Cancelling no longer schedules retries** for the files that were aborted.
- **Artwork could be swapped between files.** Pictures were exported to a shared scratch directory as `art_block_<index>`, so two workers in the artwork stage at the same time overwrote each other's export before re-import. Artwork is now optimized in memory on the metadata chain — no scratch files at all.
- **Savings percentage in the status bar was roughly halved**: `totalOriginalBytes` was set from the scan *and* accumulated per file.
- **Exported logs never said "Processing canceled by user"**: the frontend always passed `runCanceled: false`.
- Elapsed time shown after completion no longer keeps growing on re-render.
- ID3v2 prefix detection read the entire file into memory to inspect the first four bytes; it now reads only the header (or at most 1 MiB).
- Settings slider updates no longer race each other (merge against the latest value).

### Changed: JPEG artwork is now actually lossless

- The previous "Huffman reoptimization" decoded the JPEG to pixels and re-encoded it with the original quantization tables. That is **not** lossless (chroma subsampling, colour conversion and the DCT are recomputed), so the earlier "pixel data is bit-identical" claim was wrong. It is replaced by a coefficient-level transcode with mozjpeg — the same transform as `jpegtran -optimize`: the quantized DCT coefficients are read and written back unchanged with optimal Huffman tables (baseline and progressive scan layouts tried, smaller kept). Every result is additionally verified to decode to identical pixels before it replaces the original. On the test fixture this saves 10% of the cover art.

### Changed (pipeline)

- **Streaming encoder**: audio is decoded frame by frame straight into the encoder instead of materialising the whole track as `Vec<i32>` (4× the PCM size per worker).
- Tags, pictures, application and cuesheet blocks are attached to the encoder up front, so the output is written once; previously the file was rewritten for the metadata copy, again per picture removal/import, again for padding removal and again for the marker. The Vorbis vendor string now reflects the actual encoder (`reference libFLAC 1.5.0`), as `flac` itself does.
- Decoded-audio MD5 is hashed per frame instead of per sample.
- Padding savings are reported separately from artwork savings (new `paddingSavedBytes` on file events, `Meta` chip in the status bar, `Audio delta` / `Artwork delta` / `Padding removed` lines and an `EFC Savings` block in the log).
- libFLAC error states are reported by name (`FLAC__STREAM_ENCODER_VERIFY_MISMATCH_IN_AUDIO_DATA`) instead of a number.

### Removed / trimmed

- Unused Tauri plugins `shell` (including the broad `shell:allow-execute` capability), `store`, `notification`; unused crates `uuid`, `num_cpus`, `log`, `env_logger`, `jpeg-encoder` (now a dev-dependency only); oxipng's `binary`/`zopfli` features; tokio `full`.
- Dead code: sidecar-era `flac/progress.rs`, `RunLog`, `failed_log.rs`, metaflac listing parser, `build_flac_args`, `art_temp_path`, unused IPC commands (`get_processing_status`, `get_worker_statuses`, `get_recent_events`, `get_top_compression`, `validate_folder`, `select_output_folder`), `useWorkerStatus` hook, `jobHistory`/`topCompression` frontend state, `recentFolders` setting (never populated).

### Build / CI

- `gui` cargo feature (default) gates everything Tauri; the core is Tauri-free.
- The `cmake` crate drops every `/O` flag on MSVC and left libFLAC unoptimized in the dev/test profile (release was unaffected); `cmake/msvc-flags.cmake` forces `/O2` back on and dependencies are compiled optimized in dev builds, so fixture-based tests run ~5× faster.
- `strip = true` in the release profile.
- CI: clippy with `-D warnings` for both feature sets, frontend unit tests, a GTK-free `check-core` job on Ubuntu and Windows that builds and smoke-tests the CLI. Release workflow publishes CLI binaries (built with `target-cpu=x86-64-v3` for portability).

---

## v1.0.6 — 2026-03-15

### Fixed

- OS window title bar now shows `FlacCrunch v1.0.6` — required `core:window:allow-set-title` Tauri capability was missing
- Export log and auto-error log now match the original PowerShell EFC format exactly
- Export log now includes source folder, run start/finish times, worker thread count, and retry limit

---

## v1.0.5 — 2026-03-15

### Fixed

- Version now displayed in the OS window title bar (`FlacCrunch v1.0.5`) instead of the in-app header

---

## v1.0.4 — 2026-03-15

### Changed

- **Binary size**: Added `lto = "thin"` and `codegen-units = 1` to the release profile — enables cross-crate dead code elimination, expected to reduce exe size by ~20-35%
- **Version display**: App version now shown in the title bar (e.g. `v1.0.4`) and at the top of the Settings modal
- **Drag zones**: Both drop zones (folder and file) now highlight when dragging from the OS, since file vs. folder type cannot be determined until drop time. File drop zone also now accepts HTML5 drag events
- **Mobile**: Tap-to-select zone is taller; Start Processing button has more separation from the file list

---

## v1.0.3 — 2026-03-15

### Changed

- **Windows release**: NSIS setup `.exe` dropped — only MSI installer and portable `FlacCrunch.exe` are published
- **Android APK filenames**: renamed to `FlacCrunch-{version}-arm64.apk` etc. — "unsigned" suffix removed (APKs are signed; the suffix was a Gradle artifact naming convention that no longer applies)

---

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
