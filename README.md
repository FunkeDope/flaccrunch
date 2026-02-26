# Exact FLAC Cruncher

`Start-ExactFlacCrunch.ps1` recompresses FLAC files in place while protecting audio integrity.

It recursively scans a folder, runs `flac -8 -V` for each file, verifies decoded-audio MD5 before and after conversion, and only replaces the original file when verification passes.

## What It Does

- Finds all `*.flac` files under `-RootFolder` (recursive).
- Uses per-file temp outputs with `.tmp` extension (for example `track.tmp`).
- Runs multiple workers in parallel (up to CPU count or file count, whichever is smaller).
- Redirects each job's stdout/stderr to per-job log files.
- Verifies integrity with decoded-audio MD5 checks:
- Uses embedded FLAC MD5 when present.
- Computes decoded-audio MD5 hashes before and after conversion.
- Handles both 3-way verification (embedded + pre + post) and 2-way verification (pre + post when embedded MD5 is null).
- Embeds MD5 into output if source embedded MD5 is null and post hash is available.
- Replaces original only after verification succeeds.
- Relies on FLAC default metadata preservation (timestamps/permissions) and preserves modtime when writing missing MD5 metadata.
- Supports safe cancellation (`Ctrl+C` in interactive console): active jobs are stopped and temp files are cleaned.

## Requirements

- Windows PowerShell host (script is written/tested for Windows behavior).
- `flac` in `PATH`.
- `metaflac` in `PATH`.
- Read/write permissions to target files and log folder.

## Parameters

### `-RootFolder` (required)

Root directory to process.

### `-LogFolder` (optional)

Log root folder. A per-run timestamped subfolder is created under this location.

Default value in script:

`$Desktop\flaccruch-logs`

Note: the folder name is spelled `flaccruch-logs` in the script.

## Usage

### Basic

```powershell
.\Start-ExactFlacCrunch.ps1 -RootFolder "D:\Music\Album"
```

### Custom log location

```powershell
.\Start-ExactFlacCrunch.ps1 `
  -RootFolder "D:\Music" `
  -LogFolder "D:\Logs\FlacCrunch"
```

## Runtime Behavior

- Interactive console (`ConsoleHost`):
- Renders a live dashboard with worker status, progress, recent results, and total saved space.
- `Ctrl+C` triggers graceful cancellation and cleanup.
- Non-interactive host:
- Prints throttled status updates approximately every 10 seconds.

## Verification Logic

For a successful replacement, the script expects:

- `flac` exits with code `0`.
- Temp output exists.
- Hash verification passes:
- If embedded source MD5 exists:
- Embedded MD5 == pre-conversion decoded-audio MD5 == post-conversion decoded-audio MD5.
- If embedded source MD5 is null:
- pre-conversion decoded-audio MD5 == post-conversion decoded-audio MD5.

If verification fails:

- Temp file is deleted.
- File may be retried (up to 3 attempts total), except known non-retryable decode corruption cases.
- Final failures are recorded in a failed-files log.

## Logging and Output

Each run creates:

- Main run log (`<album>_<timestamp>.log`)
- Job logs folder (`jobs\`)
- One `*.err.log` and one `*.out.log` per conversion attempt
- Failed files report (`failed-files_<timestamp>.log`)

At the end, the script prints and logs:

- Processed/success/failed/pending counts
- Total elapsed time
- Total bytes saved and reduction percentage
- Success rate
- Top 3 per-file space savings
- Log file locations

## Safety Notes

- Originals are only replaced after verification success.
- On cancellation, originals remain untouched for in-flight jobs.
- Script deletes stale `.tmp` files only when a same-base `.flac` exists.

## Exit Conditions

- No FLAC files found: exits cleanly.
- Missing `flac`/`metaflac` in `PATH`: throws and stops.
- Invalid `RootFolder`: throws and stops.
