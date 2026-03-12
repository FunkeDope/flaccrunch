# Exact FLAC Cruncher

`Start-ExactFlacCrunch.ps1` recompresses FLAC files in place, verifies decoded-audio integrity, and replaces originals only after verification succeeds.

## Requirements

- PowerShell 7+ (`pwsh`)
- `flac` and `metaflac` available in `PATH` or in the script folder
- Read/write access to every target folder

Optional album-art tools:

- `oxipng` or `pngcrush` (PNG)
- `jpegtran` (JPEG)

## Parameters

### `-RootFolder` (alias: `-Path`)

- One or more folder paths to scan recursively for `.flac` files.
- Multi-input is supported by passing multiple positional paths or multiple values to `-Path`.
- Files are not valid input; each input must be a directory.

### `-LogFolder`

- Parent log directory.
- Default: `Desktop\EFC-logs`
- Fallback if Desktop is unavailable: `%USERPROFILE%\EFC-logs`

### `-Threads` (alias: `-Workers`)

- Optional worker count (`1..Int32.MaxValue`).
- Default: logical CPU count minus one (minimum 1), capped by number of FLAC files found.

## Usage

Single folder:

```powershell
.\Start-ExactFlacCrunch.ps1 "D:\Music"
```

Multiple folders:

```powershell
.\Start-ExactFlacCrunch.ps1 "D:\Music\A" "D:\Music\B" "E:\Archive\FLAC"
```

Equivalent explicit multi-input form:

```powershell
.\Start-ExactFlacCrunch.ps1 -Path "D:\Music\A","D:\Music\B"
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
