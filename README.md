# Exact FLAC Cruncher

`Start-ExactFlacCrunch.ps1` recompresses FLAC files in place, verifies decoded-audio integrity, and replaces originals only after verification succeeds.

## Requirements

- PowerShell 7+ (`pwsh`)
- `flac` and `metaflac` available in `PATH` or in the script folder (or use `-InstallDeps` to auto-install)
- Read/write access to every target folder

Optional album-art tools:

- `oxipng` or `pngcrush` (PNG)
- `jpegtran` (JPEG)

## Platform Support

Works on both **Windows** and **Linux** with PowerShell 7+.

**Windows**: Uses `winget` or `choco` for dependency installation.

**Linux**: Detects `apt-get`, `dnf`, or `pacman` for dependency installation. Log files default to `$HOME/EFC-logs` (no Desktop folder required).

## Parameters

### `-RootFolder` (alias: `-Path`)

- One or more folder paths to scan recursively for `.flac` files.
- Multi-input is supported by passing multiple positional paths or multiple values to `-Path`.
- Files are not valid input; each input must be a directory.

### `-LogFolder`

- Parent log directory.
- Windows default: `Desktop\EFC-logs` (falls back to `%USERPROFILE%\EFC-logs`)
- Linux default: `$HOME/EFC-logs`

### `-Threads` (alias: `-Workers`)

- Optional worker count (`1..Int32.MaxValue`).
- Default: logical CPU count minus one (minimum 1), capped by number of FLAC files found.

### `-InstallDeps`

- Automatically install required (`flac`, `metaflac`) and optional (`oxipng`, `jpegtran`) dependencies using the system package manager.
- Windows: `winget` (preferred) or `choco` (fallback).
- Linux: `apt-get`, `dnf`, or `pacman`.

### `-RunTests`

- Run the built-in Pester test suite and exit. Installs Pester 5+ if not present.

### `-ShowVersion`

- Display version information and exit.

## Usage

Single folder:

```powershell
.\Start-ExactFlacCrunch.ps1 "D:\Music"
```

Linux:

```powershell
./Start-ExactFlacCrunch.ps1 ~/Music
```

Multiple folders:

```powershell
.\Start-ExactFlacCrunch.ps1 "D:\Music\A" "D:\Music\B" "E:\Archive\FLAC"
```

Auto-install dependencies and run:

```powershell
.\Start-ExactFlacCrunch.ps1 "D:\Music" -InstallDeps
```

Custom logs and thread count:

```powershell
.\Start-ExactFlacCrunch.ps1 "D:\Music" -LogFolder "D:\Logs\EFC" -Threads 8
```

## Behavior Summary

- Recursively scans all provided folders for `.flac`.
- Uses `.tmp` files for conversion work.
- Verifies decoded audio before replacement.
- Preserves timestamps/ACLs when replacing originals.
- Writes:
  - run log
  - EFC-style final log
  - failed-files log (only when failures occur)

## Development

### Running Tests

```powershell
# Via build script
./build.ps1

# Via the script itself
./Start-ExactFlacCrunch.ps1 -RunTests

# Direct Pester invocation
Invoke-Pester ./Tests -Output Detailed
```

### CI

GitHub Actions runs tests on both Ubuntu and Windows. See `.github/workflows/ci.yml`.

## Quick Tutorial: Add `shell:sendto` Support

Use this when you want to right-click one or more folders in Explorer and run EFC from **Send to**.

### 1. Open the SendTo folder

Press `Win+R`, run:

```text
shell:sendto
```

### 2. Add a shortcut

- Create a shortcut in that folder pointing to `pwsh.exe` (Powershell 7).
- Rename it to something like `FLAC Crunch`.
- Add these arguments: `-ExecutionPolicy Bypass -noexit -file "C:\Path\To\Script\Start-ExactFlacCrunch.ps1`
- Shortcut target should look like: `"C:\Program Files\PowerShell\7\pwsh.exe" -ExecutionPolicy Bypass -noexit -file "C:\Scripts\Start-ExactFlacCrunch.ps1"`

### 3. Use it

- In Explorer, select one or more music folders.
- Right-click -> `Send to` -> `FLAC Crunch`.
