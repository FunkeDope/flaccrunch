<#
Exact Flac Cruncher (Corrected / Linted)

Goals (per requirements):
- Per-file temp outputs ending in ".tmp" with NO ".flac" substring (track.tmp, not track.flac.tmp).
- Invoke flac correctly with spaces/quotes handled; enforce flac -o single-file restriction by guaranteeing only one input arg.
- Redirect stdout+stderr to per-job logs (no console spam).
- Bind each flac process to a single logical core when possible (Windows affinity mask up to 64 logical processors; if >64, warn and skip affinity).
- Preserve original timestamps WITHOUT inheriting permissions (use --no-preserve-modtime, then restore timestamps).
- Normalize final file attributes/ACLs (clear ReadOnly, restore original ACL).
- Stable CLI UI (interactive ConsoleHost) and non-interactive status output.

Assumptions:
- Windows host (ProcessorAffinity semantics); flac.exe and metaflac.exe in PATH.

#>

[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [ValidateNotNullOrEmpty()]
    [string]$RootFolder,

    [Parameter(Mandatory = $false)]
    [ValidateNotNullOrEmpty()]
    [string]$LogFolder = (Join-Path -Path ([Environment]::GetFolderPath('Desktop')) -ChildPath 'flaccruch-logs')
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

# ----------------------------
# Helpers (required)
# ----------------------------

function Format-Bytes {
    param([Parameter(Mandatory)][long]$Bytes)
    if ($Bytes -lt 0) { $Bytes = 0 }
    $units = @('B', 'KB', 'MB', 'GB', 'TB', 'PB')
    $i = 0
    $v = [double]$Bytes
    while ($v -ge 1024 -and $i -lt ($units.Count - 1)) { $v /= 1024; $i++ }
    '{0:N2} {1}' -f $v, $units[$i]
}

function Get-SafeName {
    param(
        [Parameter(Mandatory)][string]$Value,
        [int]$MaxLength = 100
    )

    $invalid = [regex]::Escape(([string]::Join('', [System.IO.Path]::GetInvalidFileNameChars())))
    $safe = [regex]::Replace($Value, "[{0}]" -f $invalid, '_')
    # Avoid wildcard tokens that break Start-Process redirect path binding.
    $safe = [regex]::Replace($safe, '[\[\]\*\?]', '_')
    # Keep only filename-friendly characters for stable folder/log names.
    $safe = [regex]::Replace($safe, '[^A-Za-z0-9._ -]', '_')
    $safe = [regex]::Replace($safe, '\s+', ' ')
    $safe = $safe.Trim(' ', '.')
    if ([string]::IsNullOrWhiteSpace($safe)) { $safe = 'flac-job' }
    if ($safe.Length -gt $MaxLength) { $safe = $safe.Substring(0, $MaxLength) }
    return $safe
}

function Escape-WildcardPath {
    param([Parameter(Mandatory)][string]$Path)
    return [System.Management.Automation.WildcardPattern]::Escape($Path)
}

function Write-RunLog {
    param(
        [Parameter(Mandatory)][string]$Message,
        [ValidateSet('INFO', 'WARN', 'ERROR', 'SUCCESS')]
        [string]$Level = 'INFO'
    )

    if ([string]::IsNullOrWhiteSpace($script:LogFile)) { return }
    $ts = Get-Date -Format 'yyyy-MM-ddTHH:mm:ss.fffK'
    "{0} | {1} | {2}" -f $ts, $Level, $Message |
    Out-File -LiteralPath $script:LogFile -Append -Encoding UTF8
}

function Safe-RemoveFile {
    param([Parameter(Mandatory)][string]$Path)
    try {
        if (Test-Path -LiteralPath $Path) {
            Remove-Item -LiteralPath $Path -Force -ErrorAction SilentlyContinue
        }
    }
    catch { }
}

function Quote-WinArg {
    <#
      CreateProcess-style quoting suitable for Start-Process when UseShellExecute is false
      (which is the case when redirecting StdOut/StdErr).
      This prevents path-with-spaces splitting that caused flac to think multiple input files existed.
    #>
    param([Parameter(Mandatory)][string]$Arg)

    if ($Arg.Length -eq 0) { return '""' }
    if ($Arg -notmatch '[\s"]') { return $Arg }

    $sb = New-Object System.Text.StringBuilder
    [void]$sb.Append('"')

    $backslashes = 0
    foreach ($ch in $Arg.ToCharArray()) {
        if ($ch -eq '\') { $backslashes++; continue }

        if ($ch -eq '"') {
            if ($backslashes -gt 0) { [void]$sb.Append(('\') * ($backslashes * 2)) }
            [void]$sb.Append('\\"')
            $backslashes = 0
            continue
        }

        if ($backslashes -gt 0) { [void]$sb.Append(('\') * $backslashes); $backslashes = 0 }
        [void]$sb.Append($ch)
    }

    if ($backslashes -gt 0) { [void]$sb.Append(('\') * ($backslashes * 2)) }
    [void]$sb.Append('"')
    $sb.ToString()
}

function Try-GetFlacMd5 {
    param([Parameter(Mandatory)][string]$Path)
    try {
        # Use "--" to end options in case the path begins with '-'
        $h = (& metaflac --show-md5sum --no-filename -- $Path 2>$null).Trim()
        if ([string]::IsNullOrWhiteSpace($h)) { return $null }
        return $h
    }
    catch {
        return $null
    }
}

function Normalize-FileSecurity {
    param(
        [Parameter(Mandatory)][string]$Path,
        [System.Security.AccessControl.FileSecurity]$BaselineAcl
    )

    if (-not (Test-Path -LiteralPath $Path)) { return }

    # Ensure resulting file is not ReadOnly.
    try {
        $item = Get-Item -LiteralPath $Path -Force
        if (($item.Attributes -band [System.IO.FileAttributes]::ReadOnly) -ne 0) {
            $item.Attributes = ($item.Attributes -bxor [System.IO.FileAttributes]::ReadOnly)
        }
    }
    catch { }

    # Keep access the same as the original file so elevated runs do not
    # leave behind admin-only ACLs.
    if ($null -ne $BaselineAcl) {
        try {
            Set-Acl -LiteralPath $Path -AclObject $BaselineAcl
            return
        }
        catch { }
    }

    # Fallback: if ACL inheritance was protected, re-enable parent inheritance.
    try {
        $acl = Get-Acl -LiteralPath $Path
        if ($acl.AreAccessRulesProtected) {
            $acl.SetAccessRuleProtection($false, $true)
            Set-Acl -LiteralPath $Path -AclObject $acl
        }
    }
    catch { }
}

# ----------------------------
# Additional internal helpers
# ----------------------------

function Read-FlacProgress {
    param([Parameter(Mandatory)][string]$ErrLogPath)

    if (-not (Test-Path -LiteralPath $ErrLogPath)) { return @{ Pct = 0; Ratio = 'N/A' } }

    try {
        $fs = [System.IO.FileStream]::new($ErrLogPath, [System.IO.FileMode]::Open, [System.IO.FileAccess]::Read, [System.IO.FileShare]::ReadWrite)
        try {
            $sr = [System.IO.StreamReader]::new($fs)
            try {
                $raw = $sr.ReadToEnd()
                $matches = [regex]::Matches($raw, '(\d{1,3})% complete, ratio=([0-9.]+)')
                if ($matches.Count -gt 0) {
                    $m = $matches[$matches.Count - 1]
                    $pct = [int]$m.Groups[1].Value
                    if ($pct -lt 0) { $pct = 0 }
                    if ($pct -gt 100) { $pct = 100 }
                    return @{ Pct = $pct; Ratio = $m.Groups[2].Value }
                }
            }
            finally { $sr.Dispose() }
        }
        finally { $fs.Dispose() }
    }
    catch { }

    return @{ Pct = 0; Ratio = 'N/A' }
}

# Avoid touching $IsWindows (automatic var in PowerShell 7)
$script:IsWindowsHost = ($env:OS -eq 'Windows_NT')

# ----------------------------
# Preconditions
# ----------------------------

if (-not (Test-Path -LiteralPath $RootFolder)) { throw "RootFolder does not exist: $RootFolder" }
$rootItem = Get-Item -LiteralPath $RootFolder -Force
if (-not $rootItem.PSIsContainer) { throw "RootFolder is not a directory: $RootFolder" }

$flacCmd = Get-Command flac -ErrorAction SilentlyContinue
$metaflacCmd = Get-Command metaflac -ErrorAction SilentlyContinue
if (-not $flacCmd -or -not $metaflacCmd) { throw "'flac' and/or 'metaflac' not found in PATH." }

$albumName = $rootItem.Name
$safeAlbumName = Get-SafeName -Value $albumName
$runStamp = Get-Date -Format 'yyyyMMdd-HHmmss-fff'
if ([string]::IsNullOrWhiteSpace($LogFolder)) {
    $desktopPath = [Environment]::GetFolderPath('Desktop')
    if ([string]::IsNullOrWhiteSpace($desktopPath)) {
        $desktopPath = Join-Path -Path $env:USERPROFILE -ChildPath 'Desktop'
    }
    $LogFolder = Join-Path -Path $desktopPath -ChildPath 'flaccruch-logs'
}

New-Item -ItemType Directory -Path $LogFolder -Force | Out-Null
$runLogDir = Join-Path -Path $LogFolder -ChildPath ("{0}_{1}" -f $safeAlbumName, $runStamp)
New-Item -ItemType Directory -Path $runLogDir -Force | Out-Null

$jobLogDir = Join-Path -Path $runLogDir -ChildPath 'jobs'
New-Item -ItemType Directory -Path $jobLogDir -Force | Out-Null

$logFile = Join-Path -Path $runLogDir -ChildPath ("{0}_{1}.log" -f $safeAlbumName, $runStamp)
$script:LogFile = $logFile

@"
Exact Flac Cruncher v20250225.codex5.3
Target: $RootFolder
Log Root: $LogFolder
Run Logs: $runLogDir
Started: $(Get-Date -Format o)
===================================================================
"@ | Out-File -LiteralPath $logFile -Encoding UTF8

# Affinity support: classic mask covers 64 logical processors in a single group
$cpuCount = [Environment]::ProcessorCount
$affinityEnabled = $false
if ($script:IsWindowsHost -and $cpuCount -le 64) {
    $affinityEnabled = $true
}
elseif ($script:IsWindowsHost -and $cpuCount -gt 64) {
    "WARN | Host has $cpuCount logical processors; processor groups not handled. Affinity disabled." |
    Out-File -LiteralPath $logFile -Append -Encoding UTF8
}

function Set-SingleCoreAffinity {
    param(
        [Parameter(Mandatory)][System.Diagnostics.Process]$Process,
        [Parameter(Mandatory)][int]$CoreIndexZeroBased
    )

    if (-not $affinityEnabled) { return }

    if ($CoreIndexZeroBased -lt 0 -or $CoreIndexZeroBased -gt 63) {
        "WARN | Core index $CoreIndexZeroBased out of mask range; affinity skipped for PID $($Process.Id)." |
        Out-File -LiteralPath $logFile -Append -Encoding UTF8
        return
    }

    try {
        # Use signed Int64 shifting so bit 63 works (1L -shl 63 == Int64.MinValue)
        $mask = [IntPtr](1L -shl $CoreIndexZeroBased)
        $Process.ProcessorAffinity = $mask
    }
    catch {
        "WARN | Failed to set affinity | PID $($Process.Id) | $($_.Exception.Message)" |
        Out-File -LiteralPath $logFile -Append -Encoding UTF8
    }
}

# ----------------------------
# Cleanup stale .tmp (conservative)
# Delete *.tmp only if same-base .flac exists (track.tmp <-> track.flac)
# ----------------------------

Get-ChildItem -LiteralPath $RootFolder -Recurse -File -Force -ErrorAction SilentlyContinue |
Where-Object { $_.Extension -ieq '.tmp' } |
ForEach-Object {
    $maybeFlac = [System.IO.Path]::ChangeExtension($_.FullName, '.flac')
    if (Test-Path -LiteralPath $maybeFlac) {
        Safe-RemoveFile -Path $_.FullName
    }
}

# ----------------------------
# Collect FLAC files
# ----------------------------

$files = @(
    Get-ChildItem -LiteralPath $RootFolder -Recurse -File -Force -ErrorAction SilentlyContinue -Filter *.flac
)

$totalFiles = $files.Count
if ($totalFiles -eq 0) {
    Write-RunLog -Level INFO -Message "No FLAC files found. Exiting."
    Write-Host "No FLAC files found. Exiting."
    return
}

# Conservative default worker count
$maxWorkers = [Math]::Min([Environment]::ProcessorCount, $totalFiles)
if ($maxWorkers -lt 1) { $maxWorkers = 1 }

$maxAttemptsPerFile = 3
$queue = [System.Collections.Generic.Queue[object]]::new()
$fileOrdinal = 0
foreach ($f in $files) {
    $fileOrdinal++
    $queue.Enqueue([PSCustomObject]@{
            FileId   = $fileOrdinal
            Path     = $f.FullName
            Name     = $f.Name
            Attempts = 1
        })
}

$nullHash = '00000000000000000000000000000000'

[long]$totalOriginalBytes = 0
[long]$totalNewBytes = 0
[long]$totalSavedBytes = 0
[int]$processed = 0
[int]$failed = 0
[int]$conversionAttempts = 0

$compressionResults = [System.Collections.Generic.List[object]]::new()

$recent = [System.Collections.Generic.List[string]]::new()

$workers = for ($i = 0; $i -lt $maxWorkers; $i++) {
    [PSCustomObject]@{
        Id      = $i + 1
        CoreIdx = $i
        Job     = $null
    }
}

$interactive = ($Host.Name -eq 'ConsoleHost')
if ($interactive) {
    try { [Console]::CursorVisible = $false; Clear-Host } catch { $interactive = $false }
}

# Non-interactive throttled status output
$lastStatusUtc = [DateTime]::UtcNow
$statusInterval = [TimeSpan]::FromSeconds(10)

try {
    while ($queue.Count -gt 0 -or (@($workers | Where-Object { $_.Job -ne $null }).Count -gt 0)) {

        foreach ($w in $workers) {

            # Finalize completed job
            if ($null -ne $w.Job) {
                $job = $w.Job

                if ($job.Proc.HasExited) {
                    $exitCode = $job.Proc.ExitCode
                    $job.Proc.Dispose()

                    $errText = ""
                    if (Test-Path -LiteralPath $job.ErrLog) {
                        try { $errText = (Get-Content -LiteralPath $job.ErrLog -Raw -ErrorAction SilentlyContinue).Trim() } catch { }
                    }

                    $postHash = $null
                    $finalized = $false
                    $failureReason = $null
                    $newSize = 0
                    $saved = 0

                    if ($exitCode -eq 0 -and (Test-Path -LiteralPath $job.Temp)) {
                        $postHash = Try-GetFlacMd5 -Path $job.Temp
                        $hashOK = $false
                        if ($null -ne $postHash) {
                            if ($job.PreHash -eq $nullHash -or $job.PreHash -eq $postHash) { $hashOK = $true }
                        }

                        if ($hashOK) {
                            $newSize = (Get-Item -LiteralPath $job.Temp -Force).Length
                            $saved = $job.OrigSize - $newSize

                            # Replace original (retry a few times for transient locks)
                            $replaced = $false
                            for ($attempt = 1; $attempt -le 5 -and -not $replaced; $attempt++) {
                                try {
                                    Move-Item -LiteralPath $job.Temp -Destination $job.Original -Force
                                    $replaced = $true
                                }
                                catch {
                                    Start-Sleep -Milliseconds 200
                                }
                            }

                            if ($replaced) {
                                $processed++
                                $totalOriginalBytes += $job.OrigSize
                                $totalNewBytes += $newSize
                                $totalSavedBytes += $saved

                                # Normalize for tagging and restore timestamps
                                Normalize-FileSecurity -Path $job.Original -BaselineAcl $job.OrigAcl
                                try {
                                    $dest = Get-Item -LiteralPath $job.Original -Force
                                    $dest.CreationTimeUtc = $job.OrigCreationUtc
                                    $dest.LastWriteTimeUtc = $job.OrigLastWriteUtc
                                    $dest.LastAccessTimeUtc = $job.OrigLastAccessUtc
                                }
                                catch { }

                                $savedPct = 0.0
                                if ($job.OrigSize -gt 0) {
                                    $savedPct = [Math]::Round((($saved / [double]$job.OrigSize) * 100.0), 2)
                                }
                                $compressionResults.Add([PSCustomObject]@{
                                        Path       = $job.Original
                                        SavedBytes = $saved
                                        SavedPct   = $savedPct
                                        OrigBytes  = $job.OrigSize
                                        NewBytes   = $newSize
                                    }) | Out-Null

                                $hashNote = if ($job.PreHash -eq $nullHash) { 'EMBEDDED' } else { 'MATCH' }
                                $msg = "[OK] $($job.Name) | Attempt $($job.Attempt)/$maxAttemptsPerFile | Hash $hashNote | Saved $(Format-Bytes $saved)"
                                $recent.Insert(0, $msg)

                                Write-RunLog -Level SUCCESS -Message ("File: {0} | Attempt: {1}/{2} | Hash: {3}->{4} | Size: {5}->{6} | Saved: {7}" -f $job.Original, $job.Attempt, $maxAttemptsPerFile, $job.PreHash, $postHash, (Format-Bytes $job.OrigSize), (Format-Bytes $newSize), (Format-Bytes $saved))
                                $finalized = $true
                            }
                            else {
                                $failureReason = "Could not replace original after retries (file may be locked)"
                            }
                        }
                        else {
                            $failureReason = "Hash mismatch/unreadable"
                        }
                    }
                    else {
                        if ($exitCode -ne 0) {
                            $failureReason = "flac exit $exitCode"
                        }
                        else {
                            $failureReason = "flac produced no temp output"
                        }
                    }

                    if (-not $finalized) {
                        Safe-RemoveFile -Path $job.Temp

                        if ($job.Attempt -lt $maxAttemptsPerFile) {
                            $nextAttempt = $job.Attempt + 1
                            $queue.Enqueue([PSCustomObject]@{
                                    FileId   = $job.FileId
                                    Path     = $job.Original
                                    Name     = $job.Name
                                    Attempts = $nextAttempt
                                })

                            $msg = "[RETRY] $($job.Name) | Attempt $($job.Attempt)/$maxAttemptsPerFile failed: $failureReason"
                            $recent.Insert(0, $msg)
                            Write-RunLog -Level WARN -Message ("Retrying | File: {0} | NextAttempt: {1}/{2} | Reason: {3} | STDERR: {4} | ErrLog: {5} | OutLog: {6}" -f $job.Original, $nextAttempt, $maxAttemptsPerFile, $failureReason, $errText, $job.ErrLog, $job.OutLog)
                        }
                        else {
                            $failed++
                            $processed++
                            $msg = "[FAIL] $($job.Name) | Attempt $($job.Attempt)/$maxAttemptsPerFile | $failureReason"
                            $recent.Insert(0, $msg)
                            Write-RunLog -Level ERROR -Message ("Failed permanently | File: {0} | Attempts: {1}/{2} | Reason: {3} | STDERR: {4} | ErrLog: {5} | OutLog: {6}" -f $job.Original, $job.Attempt, $maxAttemptsPerFile, $failureReason, $errText, $job.ErrLog, $job.OutLog)
                        }
                    }

                    if ($recent.Count -gt 30) { $recent.RemoveRange(30, $recent.Count - 30) }
                    $w.Job = $null
                }
            }

            # Assign new job
            if ($null -eq $w.Job -and $queue.Count -gt 0) {
                $queueItem = $queue.Dequeue()
                $original = $queueItem.Path

                # Temp must be track.tmp (no ".flac" substring)
                $temp = [System.IO.Path]::ChangeExtension($original, '.tmp')

                $jobId = "{0:D5}_{1}_a{2}" -f $queueItem.FileId, ([guid]::NewGuid().ToString('N').Substring(0, 8)), $queueItem.Attempts
                $errLog = Join-Path -Path $jobLogDir -ChildPath "$jobId.err.log"
                $outLog = Join-Path -Path $jobLogDir -ChildPath "$jobId.out.log"
                $errLogRedirect = Escape-WildcardPath -Path $errLog
                $outLogRedirect = Escape-WildcardPath -Path $outLog

                Safe-RemoveFile -Path $temp
                Safe-RemoveFile -Path $errLog
                Safe-RemoveFile -Path $outLog

                # Snapshot metadata before conversion / replacement
                $origItem = Get-Item -LiteralPath $original -Force
                $origAcl = $null
                try { $origAcl = Get-Acl -LiteralPath $original } catch { }
                $preHash = Try-GetFlacMd5 -Path $original
                if ($null -eq $preHash) { $preHash = $nullHash }

                # Build ONE properly-quoted ArgumentList string; include "--" to end options.
                # We restore timestamps and ACL ourselves after replacement.
                $argString =
                "-8 -V -f --no-preserve-modtime -o " +
                (Quote-WinArg $temp) +
                " -- " +
                (Quote-WinArg $original)

                $conversionAttempts++
                $proc = Start-Process -FilePath $flacCmd.Source `
                    -ArgumentList $argString `
                    -NoNewWindow `
                    -PassThru `
                    -RedirectStandardError $errLogRedirect `
                    -RedirectStandardOutput $outLogRedirect

                Set-SingleCoreAffinity -Process $proc -CoreIndexZeroBased $w.CoreIdx

                $w.Job = [PSCustomObject]@{
                    Proc              = $proc
                    Original          = $original
                    Temp              = $temp
                    ErrLog            = $errLog
                    OutLog            = $outLog
                    FileId            = $queueItem.FileId
                    Name              = $queueItem.Name
                    Attempt           = $queueItem.Attempts
                    PreHash           = $preHash
                    OrigSize          = $origItem.Length
                    OrigAcl           = $origAcl
                    OrigCreationUtc   = $origItem.CreationTimeUtc
                    OrigLastWriteUtc  = $origItem.LastWriteTimeUtc
                    OrigLastAccessUtc = $origItem.LastAccessTimeUtc
                }
            }
        }

        # UI / Status
        if ($interactive) {
            $width = 120
            try { $width = [Math]::Max(80, [Math]::Min([Console]::WindowWidth - 1, 140)) } catch { }

            [Console]::SetCursorPosition(0, 0)
            Write-Host ("Exact Flac Cruncher | {0}" -f $albumName).PadRight($width)
            Write-Host ("Progress: {0}/{1}  Failed: {2}  Saved: {3}" -f $processed, $totalFiles, $failed, (Format-Bytes $totalSavedBytes)).PadRight($width)
            Write-Host ("-" * $width)

            foreach ($w in $workers) {
                if ($null -eq $w.Job) {
                    Write-Host ("Core {0:D2}: IDLE" -f $w.Id).PadRight($width)
                }
                else {
                    $p = Read-FlacProgress -ErrLogPath $w.Job.ErrLog
                    $pct = [int]$p.Pct
                    $ratio = $p.Ratio

                    $name = $w.Job.Name
                    if ($name.Length -gt 60) { $name = $name.Substring(0, 57) + "..." }

                    $barLen = 24
                    $fill = [int][Math]::Floor(($pct / 100.0) * $barLen)
                    if ($fill -lt 0) { $fill = 0 }
                    if ($fill -gt $barLen) { $fill = $barLen }
                    $bar = ("#" * $fill).PadRight($barLen, '.')

                    Write-Host ("Core {0:D2}: [{1}] {2,3}%  r={3}  a={4}/{5}  {6}" -f $w.Id, $bar, $pct, $ratio, $w.Job.Attempt, $maxAttemptsPerFile, $name).PadRight($width)
                }
            }

            Write-Host ("-" * $width)
            $linesAvail = 8
            try { $linesAvail = [Math]::Max(3, [Math]::Min(12, [Console]::WindowHeight - ($workers.Count + 6))) } catch { }

            for ($i = 0; $i -lt $linesAvail; $i++) {
                if ($i -lt $recent.Count) {
                    $s = $recent[$i]
                    if ($s.Length -gt $width) { $s = $s.Substring(0, $width) }
                    Write-Host $s.PadRight($width)
                }
                else {
                    Write-Host ("".PadRight($width))
                }
            }
        }
        else {
            $nowUtc = [DateTime]::UtcNow
            if (($nowUtc - $lastStatusUtc) -ge $statusInterval) {
                $lastStatusUtc = $nowUtc
                Write-Host ("Progress: {0}/{1}  Failed: {2}  Saved: {3}" -f $processed, $totalFiles, $failed, (Format-Bytes $totalSavedBytes))
            }
        }

        Start-Sleep -Milliseconds 150
    }
}
finally {
    if ($interactive) {
        try { [Console]::CursorVisible = $true } catch { }
    }
    Write-RunLog -Level INFO -Message ("Run logs kept at: {0}" -f $runLogDir)
}

$topCompression = @(
    $compressionResults |
    Where-Object { $_.SavedBytes -gt 0 } |
    Sort-Object -Property SavedBytes -Descending |
    Select-Object -First 5
)
$maxCompression = $null
if ($topCompression.Count -gt 0) { $maxCompression = $topCompression[0] }

$maxCompressionLine = "Max Single-File Save : N/A"
if ($null -ne $maxCompression) {
    $maxCompressionLine = "Max Single-File Save : {0} ({1:N2}%) | {2}" -f (Format-Bytes $maxCompression.SavedBytes), $maxCompression.SavedPct, $maxCompression.Path
}

$summaryLines = [System.Collections.Generic.List[string]]::new()
$summaryLines.Add("===================================================================") | Out-Null
$summaryLines.Add("JOB SUMMARY: $albumName") | Out-Null
$summaryLines.Add(("Processed Files      : {0}" -f $processed)) | Out-Null
$summaryLines.Add(("Failed Files         : {0}" -f $failed)) | Out-Null
$summaryLines.Add(("Total Files Found    : {0}" -f $totalFiles)) | Out-Null
$summaryLines.Add(("Total Attempts       : {0}" -f $conversionAttempts)) | Out-Null
$summaryLines.Add(("Total Original Size  : {0}" -f (Format-Bytes $totalOriginalBytes))) | Out-Null
$summaryLines.Add(("Total New Size       : {0}" -f (Format-Bytes $totalNewBytes))) | Out-Null
$summaryLines.Add(("Total Space Saved    : {0}" -f (Format-Bytes $totalSavedBytes))) | Out-Null
$summaryLines.Add($maxCompressionLine) | Out-Null
$summaryLines.Add("Top 5 Compression    :") | Out-Null

if ($topCompression.Count -eq 0) {
    $summaryLines.Add("  (No successful file conversions)") | Out-Null
}
else {
    $rank = 0
    foreach ($entry in $topCompression) {
        $rank++
        $summaryLines.Add(("  {0}. Saved {1} ({2:N2}%) | {3}" -f $rank, (Format-Bytes $entry.SavedBytes), $entry.SavedPct, $entry.Path)) | Out-Null
    }
}

$summaryLines.Add(("Finished             : {0}" -f (Get-Date -Format o))) | Out-Null
$summaryLines.Add("Logs                 : $runLogDir") | Out-Null
$summaryLines.Add("===================================================================") | Out-Null

$summaryText = [string]::Join([Environment]::NewLine, $summaryLines)
$summaryText | Out-File -LiteralPath $logFile -Append -Encoding UTF8

Write-Host ""
Write-Host "JOB COMPLETE"
Write-Host ("Saved: {0}" -f (Format-Bytes $totalSavedBytes))
Write-Host $maxCompressionLine
Write-Host "Top 5 Compression:"
if ($topCompression.Count -eq 0) {
    Write-Host "  (No successful file conversions)"
}
else {
    $rank = 0
    foreach ($entry in $topCompression) {
        $rank++
        Write-Host ("  {0}. Saved {1} ({2:N2}%) | {3}" -f $rank, (Format-Bytes $entry.SavedBytes), $entry.SavedPct, $entry.Path)
    }
}
Write-Host ("Logs:  {0}" -f $runLogDir)
Write-Host ("Log:   {0}" -f $logFile)
