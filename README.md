# Exact FLAC Cruncher

`Start-ExactFlacCrunch.ps1` scans a folder for `.flac` files, recompresses them, verifies the audio, and replaces the original file only after the check passes.

## Requirements

- Windows PowerShell
- `flac` in `PATH`
- `metaflac` in `PATH`
- Read and write access to the target music folder

Optional:

- `oxipng` or `pngcrush` in `PATH` for PNG album art optimization
- `jpegtran` in `PATH` for JPEG album art optimization

The optional tools can also be placed in the same folder as `Start-ExactFlacCrunch.ps1`.

## What Must Be In PATH

Required:

- `flac`
- `metaflac`

Optional:

- `oxipng` or `pngcrush`
- `jpegtran`

If `flac` or `metaflac` are not available, the script stops.

## Installation

1. Install the FLAC command line tools so `flac.exe` and `metaflac.exe` are available.
2. Make sure those executables are in your system `PATH`, or place them beside `Start-ExactFlacCrunch.ps1`.
3. Optional: install `oxipng`, `pngcrush`, or `jpegtran` if you want album art optimization.
4. Save `Start-ExactFlacCrunch.ps1` somewhere you can run it from PowerShell.

## Parameters

### `-RootFolder`

Required. This is the folder the script scans recursively for `.flac` files.

### `-LogFolder`

Optional. This is the parent folder for log output.

If not provided, the script uses:

`Desktop\flaccruch-logs`

If no Desktop folder is available, it falls back to:

`%USERPROFILE%\flaccruch-logs`

## Usage

Run from PowerShell:

```powershell
.\Start-ExactFlacCrunch.ps1 -RootFolder "D:\Music"
```

With a custom log folder:

```powershell
.\Start-ExactFlacCrunch.ps1 -RootFolder "D:\Music" -LogFolder "D:\Logs\FlacCrunch"
```

## Basic Behavior

- The script scans `-RootFolder` recursively for `.flac` files.
- It creates logs in a timestamped subfolder under `-LogFolder`.
- It uses `.tmp` files during conversion.
- It checks the converted audio before replacing the original file.
- If verification fails, the original file is not replaced.
