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

function Set-FlacMd5IfMissing {
    param(
        [Parameter(Mandatory)][string]$Path,
        [Parameter(Mandatory)][string]$ExpectedMd5,
        [Parameter(Mandatory)][string]$NullHash
    )

    if ([string]::IsNullOrWhiteSpace($ExpectedMd5)) { return $false }

    $current = Try-GetFlacMd5 -Path $Path
    if (-not [string]::IsNullOrWhiteSpace($current) -and ($current -ne $NullHash)) {
        return ($current -eq $ExpectedMd5)
    }

    try {
        # Only write when MD5 is absent; keep existing valid embedded values unchanged.
        $null = (& metaflac --set-md5sum=$ExpectedMd5 -- $Path 2>$null)
    }
    catch {
        return $false
    }

    $after = Try-GetFlacMd5 -Path $Path
    if ([string]::IsNullOrWhiteSpace($after)) { return $false }
    return ($after -eq $ExpectedMd5)
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
    param(
        [Parameter(Mandatory)][string]$ErrLogPath,
        [hashtable]$Cache
    )

    $default = @{ Pct = 0; Ratio = 'N/A' }
    if (-not (Test-Path -LiteralPath $ErrLogPath)) { return $default }

    $cacheKey = $ErrLogPath.ToLowerInvariant()
    $entry = $null
    if ($null -ne $Cache -and $Cache.ContainsKey($cacheKey)) { $entry = $Cache[$cacheKey] }

    try {
        $fi = Get-Item -LiteralPath $ErrLogPath -ErrorAction Stop
        $length = [long]$fi.Length

        if ($null -ne $entry -and $entry.Length -eq $length) {
            return @{ Pct = $entry.Pct; Ratio = $entry.Ratio }
        }

        $tailSize = 24576L
        $startOffset = [Math]::Max(0L, $length - $tailSize)
        $raw = ''

        $fs = [System.IO.FileStream]::new($ErrLogPath, [System.IO.FileMode]::Open, [System.IO.FileAccess]::Read, [System.IO.FileShare]::ReadWrite)
        try {
            [void]$fs.Seek($startOffset, [System.IO.SeekOrigin]::Begin)
            $sr = [System.IO.StreamReader]::new($fs)
            try {
                $raw = $sr.ReadToEnd()
            }
            finally { $sr.Dispose() }
        }
        finally { $fs.Dispose() }

        $pct = if ($null -ne $entry) { [int]$entry.Pct } else { 0 }
        $ratio = if ($null -ne $entry) { [string]$entry.Ratio } else { 'N/A' }

        $matches = [regex]::Matches($raw, '(\d{1,3})% complete, ratio=([0-9.]+)')
        if ($matches.Count -gt 0) {
            $m = $matches[$matches.Count - 1]
            $pct = [int]$m.Groups[1].Value
            if ($pct -lt 0) { $pct = 0 }
            if ($pct -gt 100) { $pct = 100 }
            $ratio = $m.Groups[2].Value
        }

        if ($null -ne $Cache) {
            $Cache[$cacheKey] = [PSCustomObject]@{
                Length = $length
                Pct    = $pct
                Ratio  = $ratio
            }
        }

        return @{ Pct = $pct; Ratio = $ratio }
    }
    catch {
        if ($null -ne $entry) { return @{ Pct = $entry.Pct; Ratio = $entry.Ratio } }
        return $default
    }
}

function Format-HashForUi {
    param(
        [AllowNull()][string]$Hash,
        [Parameter(Mandatory)][string]$NullHash
    )

    if ([string]::IsNullOrWhiteSpace($Hash)) { return 'N/A' }
    if ($Hash -eq $NullHash) { return 'NULL-EMBEDDED' }
    $h = $Hash.ToLowerInvariant()
    if ($h.Length -le 20) { return $h }
    return ('{0}...{1}' -f $h.Substring(0, 10), $h.Substring($h.Length - 6))
}

function Truncate-Text {
    param(
        [AllowNull()][string]$Text,
        [Parameter(Mandatory)][int]$Width
    )

    if ($Width -lt 1) { return '' }
    if ($null -eq $Text) { $Text = '' }
    if ($Text.Length -le $Width) { return $Text }
    if ($Width -le 3) { return $Text.Substring(0, $Width) }
    return ($Text.Substring(0, $Width - 3) + '...')
}

function Get-CompressionColor {
    param([AllowNull()][string]$CompressionPct)

    if ([string]::IsNullOrWhiteSpace($CompressionPct) -or $CompressionPct -eq 'N/A') {
        return 'DarkGray'
    }

    $valueText = $CompressionPct.Trim().TrimEnd('%')
    [double]$value = 0
    if (-not [double]::TryParse($valueText, [System.Globalization.NumberStyles]::Float, [System.Globalization.CultureInfo]::InvariantCulture, [ref]$value)) {
        return 'DarkGray'
    }

    # Positive-only gradient: more compression => stronger "good" color, no warning colors.
    if ($value -ge 5.0) { return 'White' }           # Outstanding
    if ($value -ge 3.0) { return 'Green' }           # Very good
    if ($value -gt 1.0) { return 'Cyan' }            # Good
    if ($value -gt 0.0) { return 'DarkCyan' }        # Any gain is positive
    if ($value -eq 0.0) { return 'DarkGray' }        # No change
    return 'DarkGray'                                # Expansion / worse result
}

function Get-StatusColor {
    param([AllowNull()][string]$Status)
    switch ($Status) {
        'OK' { return 'Green' }
        'RETRY' { return 'Yellow' }
        'FAIL' { return 'Red' }
        default { return 'White' }
    }
}

function Get-VerificationColor {
    param([AllowNull()][string]$Verification)

    if ([string]::IsNullOrWhiteSpace($Verification)) { return 'DarkGray' }
    if ($Verification -like 'PASS*') { return 'Green' }
    if ($Verification -like 'FAIL*') { return 'Red' }
    return 'Yellow'
}

function Format-VerificationText {
    param([AllowNull()][string]$Verification)

    if ([string]::IsNullOrWhiteSpace($Verification)) { return 'N/A' }
    return $Verification
}

function Render-InteractiveUi {
    param(
        [Parameter(Mandatory)][string]$AlbumName,
        [Parameter(Mandatory)][int]$Processed,
        [Parameter(Mandatory)][int]$TotalFiles,
        [Parameter(Mandatory)][int]$Failed,
        [Parameter(Mandatory)][long]$TotalSavedBytes,
        [Parameter(Mandatory)][int]$QueueCount,
        [Parameter(Mandatory)][int]$MaxAttemptsPerFile,
        [Parameter(Mandatory)][object[]]$Workers,
        [AllowNull()][System.Collections.Generic.List[object]]$RecentEvents,
        [Parameter(Mandatory)][hashtable]$ProgressCache,
        [int]$PreviousRows = 0,
        [switch]$ForceClear,
        [string]$Banner = ''
    )

    $width = 120
    $height = 30
    try {
        $width = [Math]::Max(100, [Console]::WindowWidth - 1)
        $height = [Math]::Max(24, [Console]::WindowHeight)
    }
    catch { }

    $maxRows = [Math]::Max(18, $height - 1)
    if ($ForceClear) {
        try { Clear-Host } catch { }
    }

    try { [Console]::SetCursorPosition(0, 0) } catch { }

    if ($null -eq $RecentEvents) {
        $RecentEvents = [System.Collections.Generic.List[object]]::new()
    }

    $activeWorkers = @($Workers | Where-Object { $_.Job -ne $null })
    $activeCount = $activeWorkers.Count

    $rowsWritten = 0
    function Write-UiLine {
        param(
            [AllowNull()][string]$Text = '',
            [ConsoleColor]$Color = [ConsoleColor]::Gray
        )
        $content = Truncate-Text -Text $Text -Width $width
        Write-Host $content.PadRight($width) -ForegroundColor $Color
        $script:__uiRowsWritten++
    }

    $script:__uiRowsWritten = 0

    Write-UiLine -Text ("Exact Flac Cruncher | {0}" -f $AlbumName) -Color Cyan
    Write-UiLine -Text ("Progress {0}/{1} | Failed {2} | Saved {3} | Queue {4} | Active {5}/{6}" -f $Processed, $TotalFiles, $Failed, (Format-Bytes $TotalSavedBytes), $QueueCount, $activeCount, $Workers.Count) -Color White
    if ([string]::IsNullOrWhiteSpace($Banner)) {
        Write-UiLine -Text "Ctrl+C: Cancel safely. Will clean up temp files and restore original files on exit." -Color DarkGray
    }
    else {
        Write-UiLine -Text $Banner -Color Yellow
    }
    Write-UiLine -Text ("-" * $width) -Color DarkGray

    $fixedRows = 12
    $remainingRows = [Math]::Max(6, $maxRows - $fixedRows)
    $workerRows = [Math]::Min($Workers.Count, [Math]::Max(4, [int][Math]::Floor($remainingRows * 0.45)))
    $eventRows = [Math]::Max(4, $remainingRows - $workerRows)

    $wCore = 6
    $wState = 7
    $wTry = 5
    $wPct = 5
    $wRatio = 8
    $wBar = 22
    $workerFixed = $wCore + $wState + $wTry + $wPct + $wRatio + $wBar
    $workerSep = 6
    $wFile = [Math]::Max(18, $width - ($workerFixed + $workerSep))

    Write-UiLine -Text "Workers" -Color Cyan
    Write-UiLine -Text (('Core'.PadRight($wCore) + '|' +
            'State'.PadRight($wState) + '|' +
            'Try'.PadRight($wTry) + '|' +
            'Pct'.PadRight($wPct) + '|' +
            'Ratio'.PadRight($wRatio) + '|' +
            'Progress'.PadRight($wBar) + '|' +
            'File'.PadRight($wFile))) -Color DarkCyan

    for ($i = 0; $i -lt $workerRows; $i++) {
        if ($i -ge $Workers.Count) {
            Write-UiLine -Text '' -Color DarkGray
            continue
        }

        $w = $Workers[$i]
        if ($null -eq $w.Job) {
            $line = (("C{0:D2}" -f $w.Id).PadRight($wCore) + '|' +
                'IDLE'.PadRight($wState) + '|' +
                '-'.PadRight($wTry) + '|' +
                '-'.PadRight($wPct) + '|' +
                '-'.PadRight($wRatio) + '|' +
                ''.PadRight($wBar) + '|' +
                ''.PadRight($wFile))
            Write-UiLine -Text $line -Color DarkGray
            continue
        }

        $p = Read-FlacProgress -ErrLogPath $w.Job.ErrLog -Cache $ProgressCache
        $pct = [int]$p.Pct
        $ratio = [string]$p.Ratio
        if ($ratio -eq 'N/A') { $ratio = '-' }
        $attempt = ("{0}/{1}" -f $w.Job.Attempt, $MaxAttemptsPerFile)

        $barLen = [Math]::Max(8, $wBar - 2)
        $fill = [int][Math]::Floor(($pct / 100.0) * $barLen)
        if ($fill -lt 0) { $fill = 0 }
        if ($fill -gt $barLen) { $fill = $barLen }
        $bar = ('[' + ('#' * $fill).PadRight($barLen, '.') + ']')
        $name = Truncate-Text -Text $w.Job.Name -Width $wFile

        $line = (("C{0:D2}" -f $w.Id).PadRight($wCore) + '|' +
            'BUSY'.PadRight($wState) + '|' +
            $attempt.PadRight($wTry) + '|' +
            ("{0,3}%" -f $pct).PadRight($wPct) + '|' +
            $ratio.PadRight($wRatio) + '|' +
            $bar.PadRight($wBar) + '|' +
            $name.PadRight($wFile))
        $lineColor = if ($pct -ge 100) { 'Green' } else { 'White' }
        Write-UiLine -Text $line -Color $lineColor
    }

    if ($Workers.Count -gt $workerRows) {
        Write-UiLine -Text ("... {0} more workers not shown (resize taller to view)." -f ($Workers.Count - $workerRows)) -Color DarkGray
    }
    else {
        Write-UiLine -Text ("-" * $width) -Color DarkGray
    }

    $wTime = 8
    $wStat = 6
    $wTry2 = 7
    $wComp = 13
    $wSaved = 12
    $wHash = 10
    $eventFixed = $wTime + $wStat + $wTry2 + $wComp + $wSaved + $wHash
    $eventSep = 7
    $wTextTotal = [Math]::Max(34, $width - ($eventFixed + $eventSep))
    $wFile = [int][Math]::Floor($wTextTotal * 0.62)
    if ($wFile -lt 18) { $wFile = 18 }
    if ($wFile -gt ($wTextTotal - 12)) { $wFile = $wTextTotal - 12 }
    $wDetail = $wTextTotal - $wFile

    Write-UiLine -Text "Recent Results (latest first)" -Color Cyan
    Write-UiLine -Text (('Time'.PadRight($wTime) + '|' +
            'Status'.PadRight($wStat) + '|' +
            'Attempt'.PadRight($wTry2) + '|' +
            'Compression %'.PadRight($wComp) + '|' +
            'Saved'.PadRight($wSaved) + '|' +
            'Verify'.PadRight($wHash) + '|' +
            'File'.PadRight($wFile) + '|' +
            'Detail'.PadRight($wDetail))) -Color DarkCyan

    $rowsToPrint = [Math]::Min($eventRows, $RecentEvents.Count)
    for ($i = 0; $i -lt $rowsToPrint; $i++) {
        $row = $RecentEvents[$i]
        $statusColor = Get-StatusColor -Status $row.Status
        $cmpColor = Get-CompressionColor -CompressionPct $row.CompressionPct
        $savedColor = $cmpColor
        $verificationDisplay = Format-VerificationText -Verification ([string]$row.Verification)
        $verificationColor = Get-VerificationColor -Verification $row.Verification
        $fileText = Truncate-Text -Text ([string]$row.File) -Width $wFile
        $detailText = Truncate-Text -Text ([string]$row.Detail) -Width $wDetail

        Write-Host ([string]$row.Time).PadRight($wTime) -NoNewline -ForegroundColor DarkGray
        Write-Host '|' -NoNewline -ForegroundColor DarkGray
        Write-Host ([string]$row.Status).PadRight($wStat) -NoNewline -ForegroundColor $statusColor
        Write-Host '|' -NoNewline -ForegroundColor DarkGray
        Write-Host ([string]$row.Attempt).PadRight($wTry2) -NoNewline -ForegroundColor Gray
        Write-Host '|' -NoNewline -ForegroundColor DarkGray
        Write-Host ([string]$row.CompressionPct).PadRight($wComp) -NoNewline -ForegroundColor $cmpColor
        Write-Host '|' -NoNewline -ForegroundColor DarkGray
        Write-Host ([string]$row.Saved).PadRight($wSaved) -NoNewline -ForegroundColor $savedColor
        Write-Host '|' -NoNewline -ForegroundColor DarkGray
        Write-Host (Truncate-Text -Text $verificationDisplay -Width $wHash).PadRight($wHash) -NoNewline -ForegroundColor $verificationColor
        Write-Host '|' -NoNewline -ForegroundColor DarkGray
        Write-Host $fileText.PadRight($wFile) -NoNewline -ForegroundColor Gray
        Write-Host '|' -NoNewline -ForegroundColor DarkGray
        Write-Host $detailText.PadRight($wDetail) -ForegroundColor DarkGray
        $script:__uiRowsWritten++
    }

    for ($i = $rowsToPrint; $i -lt $eventRows; $i++) {
        Write-UiLine -Text '' -Color DarkGray
    }

    $rowsWritten = $script:__uiRowsWritten
    $targetRows = [Math]::Max($rowsWritten, $PreviousRows)
    for ($i = $rowsWritten; $i -lt $targetRows; $i++) {
        Write-Host ''.PadRight($width)
    }

    return [Math]::Min($maxRows, $targetRows)
}

function Wrap-CellText {
    param(
        [AllowNull()][string]$Text,
        [Parameter(Mandatory)][int]$Width
    )

    if ($Width -lt 1) { return @('') }
    if ($null -eq $Text -or $Text -eq '') { return @('') }

    $chunks = [System.Collections.Generic.List[string]]::new()
    $remaining = $Text

    while ($remaining.Length -gt $Width) {
        $take = $Width
        $slice = $remaining.Substring(0, $take)
        $lastSpace = $slice.LastIndexOf(' ')
        if ($lastSpace -ge 1) {
            $chunks.Add($remaining.Substring(0, $lastSpace))
            $remaining = $remaining.Substring($lastSpace + 1)
        }
        else {
            $chunks.Add($slice)
            $remaining = $remaining.Substring($take)
        }
    }

    $chunks.Add($remaining)
    return , $chunks.ToArray()
}

function Push-RecentEvent {
    param(
        [Parameter(Mandatory)][object]$List,
        [ValidateSet('OK', 'RETRY', 'FAIL')][string]$Status,
        [Parameter(Mandatory)][string]$File,
        [Parameter(Mandatory)][string]$Attempt,
        [string]$EmbeddedHash = 'N/A',
        [string]$CalculatedBeforeHash = 'N/A',
        [string]$CalculatedAfterHash = 'N/A',
        [string]$Verification = 'N/A',
        [string]$BeforeAfter = 'N/A',
        [string]$Saved = 'N/A',
        [string]$CompressionPct = 'N/A',
        [string]$Detail = ''
    )

    if ($null -eq $List) { throw "Push-RecentEvent: event list is null." }
    if (-not ($List -is [System.Collections.Generic.List[object]])) {
        throw ("Push-RecentEvent: expected List[object], got {0}" -f $List.GetType().FullName)
    }

    $List.Insert(0, [PSCustomObject]@{
            Time           = (Get-Date).ToString('HH:mm:ss')
            Status         = $Status
            File           = $File
            Attempt        = $Attempt
            EmbeddedHash   = $EmbeddedHash
            CalcBeforeHash = $CalculatedBeforeHash
            CalcAfterHash  = $CalculatedAfterHash
            Verification   = $Verification
            BeforeAfter    = $BeforeAfter
            Saved          = $Saved
            CompressionPct = $CompressionPct
            Detail         = $Detail
        })

    if ($List.Count -gt 25) { $List.RemoveRange(25, $List.Count - 25) }
}

function Stop-ActiveJobsAndCleanup {
    param([Parameter(Mandatory)][object[]]$Workers)

    [int]$killed = 0
    [int]$tmpDeleted = 0

    foreach ($w in $Workers) {
        if ($null -eq $w.Job) { continue }
        $job = $w.Job

        try {
            if ($null -ne $job.Proc -and -not $job.Proc.HasExited) {
                try {
                    $job.Proc.Kill($true)
                }
                catch {
                    try { Stop-Process -Id $job.Proc.Id -Force -ErrorAction Stop } catch { }
                }
                $killed++
            }
        }
        catch { }

        # Original files are only replaced after successful hash-chain verification.
        # On cancellation, delete temp artifacts so originals remain intact.
        if (-not [string]::IsNullOrWhiteSpace($job.Temp) -and (Test-Path -LiteralPath $job.Temp)) {
            Safe-RemoveFile -Path $job.Temp
            $tmpDeleted++
        }

        try {
            if ($null -ne $job.Proc) { $job.Proc.Dispose() }
        }
        catch { }

        $w.Job = $null
    }

    return [PSCustomObject]@{
        Killed      = $killed
        TempDeleted = $tmpDeleted
    }
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

function Try-GetDecodedAudioMd5 {
    param(
        [Parameter(Mandatory)][string]$FlacExePath,
        [Parameter(Mandatory)][string]$Path
    )

    function Invoke-FlacHashFromStdout {
        param(
            [Parameter(Mandatory)][string]$ExePath,
            [Parameter(Mandatory)][string]$Args
        )

        $psi = [System.Diagnostics.ProcessStartInfo]::new()
        $psi.FileName = $ExePath
        $psi.Arguments = $Args
        $psi.UseShellExecute = $false
        $psi.RedirectStandardOutput = $true
        $psi.RedirectStandardError = $true
        $psi.CreateNoWindow = $true

        $proc = [System.Diagnostics.Process]::new()
        $proc.StartInfo = $psi
        $null = $proc.Start()

        $md5 = [System.Security.Cryptography.MD5]::Create()
        try {
            $buffer = New-Object byte[] 65536
            $stream = $proc.StandardOutput.BaseStream

            while ($true) {
                $read = $stream.Read($buffer, 0, $buffer.Length)
                if ($read -le 0) { break }
                $null = $md5.TransformBlock($buffer, 0, $read, $buffer, 0)
            }

            $empty = [byte[]]::new(0)
            $null = $md5.TransformFinalBlock($empty, 0, 0)

            $stderr = $proc.StandardError.ReadToEnd()
            $proc.WaitForExit()
            if ($proc.ExitCode -ne 0) { return $null }

            return ([BitConverter]::ToString($md5.Hash)).Replace('-', '').ToLowerInvariant()
        }
        finally {
            $md5.Dispose()
            $proc.Dispose()
        }
    }

    try {
        $quotedPath = Quote-WinArg $Path

        # Prefer raw PCM hashing; use older-flac-compatible flags (-s instead of --totally-silent).
        $rawArgVariants = @(
            ("-d -c -s --force-raw-format --endian=little --sign=signed -- " + $quotedPath),
            ("-d -c -s --force-raw-format -- " + $quotedPath)
        )

        foreach ($args in $rawArgVariants) {
            $h = Invoke-FlacHashFromStdout -ExePath $FlacExePath -Args $args
            if (-not [string]::IsNullOrWhiteSpace($h)) { return $h }
        }
    }
    catch {
        # Fall through to null return.
    }

    return $null
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

$recentEvents = [System.Collections.Generic.List[object]]::new()

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

$script:CancelRequested = $false
$restoreTreatControlCAsInput = $null
if ($interactive) {
    try {
        $restoreTreatControlCAsInput = [Console]::TreatControlCAsInput
        [Console]::TreatControlCAsInput = $true
    }
    catch {
        $restoreTreatControlCAsInput = $null
    }
}

# Non-interactive throttled status output
$lastStatusUtc = [DateTime]::UtcNow
$statusInterval = [TimeSpan]::FromSeconds(10)
$runCanceled = $false
$progressCache = @{}
$lastUiFrameUtc = [DateTime]::MinValue
$uiFrameInterval = [TimeSpan]::FromMilliseconds(250)
$lastUiRenderRows = 0
$lastUiSizeKey = ''
$uiBanner = ''
$uiDirty = $interactive

try {
    while ($queue.Count -gt 0 -or (@($workers | Where-Object { $_.Job -ne $null }).Count -gt 0)) {
        if ($interactive -and -not $script:CancelRequested) {
            try {
                while ([Console]::KeyAvailable) {
                    $key = [Console]::ReadKey($true)
                    if ((($key.Modifiers -band [ConsoleModifiers]::Control) -ne 0) -and $key.Key -eq [ConsoleKey]::C) {
                        $script:CancelRequested = $true
                        break
                    }
                }
            }
            catch { }
        }

        if ($interactive) {
            $nowUtc = [DateTime]::UtcNow
            $sizeKey = ''
            try { $sizeKey = '{0}x{1}' -f [Console]::WindowWidth, [Console]::WindowHeight } catch { }
            $sizeChanged = ($sizeKey -ne $lastUiSizeKey)
            if ($sizeChanged) { $lastUiSizeKey = $sizeKey }

            if ($uiDirty -or $sizeChanged -or (($nowUtc - $lastUiFrameUtc) -ge $uiFrameInterval)) {
                $lastUiRenderRows = Render-InteractiveUi `
                    -AlbumName $albumName `
                    -Processed $processed `
                    -TotalFiles $totalFiles `
                    -Failed $failed `
                    -TotalSavedBytes $totalSavedBytes `
                    -QueueCount $queue.Count `
                    -MaxAttemptsPerFile $maxAttemptsPerFile `
                    -Workers @($workers) `
                    -RecentEvents $recentEvents `
                    -ProgressCache $progressCache `
                    -PreviousRows $lastUiRenderRows `
                    -ForceClear:$sizeChanged `
                    -Banner $uiBanner
                $lastUiFrameUtc = $nowUtc
                $uiDirty = $false
            }
        }

        if ($script:CancelRequested) {
            $runCanceled = $true
            $uiBanner = "Cancellation requested... stopping active jobs and cleaning temp files."
            $uiDirty = $true
            if ($interactive) {
                $lastUiRenderRows = Render-InteractiveUi `
                    -AlbumName $albumName `
                    -Processed $processed `
                    -TotalFiles $totalFiles `
                    -Failed $failed `
                    -TotalSavedBytes $totalSavedBytes `
                    -QueueCount $queue.Count `
                    -MaxAttemptsPerFile $maxAttemptsPerFile `
                    -Workers @($workers) `
                    -RecentEvents $recentEvents `
                    -ProgressCache $progressCache `
                    -PreviousRows $lastUiRenderRows `
                    -Banner $uiBanner
            }
            Write-RunLog -Level WARN -Message "Cancellation requested by user (Ctrl+C)."

            $cancelResult = Stop-ActiveJobsAndCleanup -Workers @($workers)
            $queue.Clear()

            Write-RunLog -Level WARN -Message ("Cancellation cleanup complete | ProcessesStopped: {0} | TempFilesDeleted: {1}" -f $cancelResult.Killed, $cancelResult.TempDeleted)
            break
        }

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

                    $postCalcHash = $null
                    $finalized = $false
                    $failureReason = $null
                    $newSize = 0
                    $saved = 0

                    if ($exitCode -eq 0 -and (Test-Path -LiteralPath $job.Temp)) {
                        if ([string]::IsNullOrWhiteSpace($job.PreCalcHash)) {
                            $job.PreCalcHash = Try-GetDecodedAudioMd5 -FlacExePath $flacCmd.Source -Path $job.Original
                        }
                        $postCalcHash = Try-GetDecodedAudioMd5 -FlacExePath $flacCmd.Source -Path $job.Temp
                        $hashOK = $false
                        $embeddedPresent = ($job.EmbeddedHash -ne $nullHash)
                        $calcBeforeOk = -not [string]::IsNullOrWhiteSpace($job.PreCalcHash)
                        $calcAfterOk = -not [string]::IsNullOrWhiteSpace($postCalcHash)
                        $calcMatch = $calcBeforeOk -and $calcAfterOk -and ($job.PreCalcHash -eq $postCalcHash)

                        if ($calcMatch) {
                            if ($embeddedPresent) {
                                if (($job.EmbeddedHash -eq $job.PreCalcHash) -and ($job.EmbeddedHash -eq $postCalcHash)) {
                                    $hashOK = $true
                                }
                            }
                            else {
                                $hashOK = $true
                            }
                        }

                        if ($hashOK) {
                            if (-not $embeddedPresent) {
                                $md5Embedded = Set-FlacMd5IfMissing -Path $job.Temp -ExpectedMd5 $postCalcHash -NullHash $nullHash
                                if (-not $md5Embedded) {
                                    $failureReason = "Could not embed MD5 into output FLAC"
                                    $hashOK = $false
                                }
                            }
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
                                catch { }
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
                                    $savedPctRaw = [Math]::Round((($saved / [double]$job.OrigSize) * 100.0), 2)
                                    $savedPct = [Math]::Max(0.0, $savedPctRaw)
                                }
                                $compressionResults.Add([PSCustomObject]@{
                                        Path       = $job.Original
                                        SavedBytes = $saved
                                        SavedPct   = $savedPct
                                        OrigBytes  = $job.OrigSize
                                        NewBytes   = $newSize
                                    }) | Out-Null

                                $verification = if ($job.EmbeddedHash -eq $nullHash) { 'PASS/NEW' } else { 'PASS' }
                                Push-RecentEvent -List $recentEvents `
                                    -Status 'OK' `
                                    -File $job.Name `
                                    -Attempt ("{0}/{1}" -f $job.Attempt, $maxAttemptsPerFile) `
                                    -EmbeddedHash (Format-HashForUi -Hash $job.EmbeddedHash -NullHash $nullHash) `
                                    -CalculatedBeforeHash (Format-HashForUi -Hash $job.PreCalcHash -NullHash $nullHash) `
                                    -CalculatedAfterHash (Format-HashForUi -Hash $postCalcHash -NullHash $nullHash) `
                                    -Verification $verification `
                                    -BeforeAfter ("{0} -> {1}" -f (Format-Bytes $job.OrigSize), (Format-Bytes $newSize)) `
                                    -Saved (Format-Bytes $saved) `
                                    -CompressionPct ("{0:N2}%" -f $savedPct) `
                                    -Detail 'Replaced original'

                                Write-RunLog -Level SUCCESS -Message ("File: {0} | Attempt: {1}/{2} | Embedded: {3} | CalcPre: {4} | CalcPost: {5} | Verification: {6} | Size: {7}->{8} | Saved: {9} ({10:N2}%)" -f $job.Original, $job.Attempt, $maxAttemptsPerFile, $job.EmbeddedHash, $job.PreCalcHash, $postCalcHash, $verification, (Format-Bytes $job.OrigSize), (Format-Bytes $newSize), (Format-Bytes $saved), $savedPct)
                                $finalized = $true
                            }
                            else {
                                $failureReason = "Could not replace original after retries (file may be locked)"
                            }
                        }
                        else {
                            if (-not $calcBeforeOk -or -not $calcAfterOk) {
                                $failureReason = "Could not calculate decoded-audio hash (pre or post)"
                            }
                            elseif ($embeddedPresent) {
                                $failureReason = "3-hash check failed (embedded/pre/post mismatch)"
                            }
                            else {
                                $failureReason = "2-hash calc check failed (no embedded hash present)"
                            }
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

                            $verification = 'FAIL-INCOMPLETE'
                            if (-not [string]::IsNullOrWhiteSpace($job.PreCalcHash) -and -not [string]::IsNullOrWhiteSpace($postCalcHash)) {
                                if ($job.EmbeddedHash -eq $nullHash) {
                                    $verification = if ($job.PreCalcHash -eq $postCalcHash) { 'PASS-AUDIO' } else { 'FAIL-AUDIO' }
                                }
                                else {
                                    $verification = if (($job.EmbeddedHash -eq $job.PreCalcHash) -and ($job.EmbeddedHash -eq $postCalcHash)) { 'PASS-3WAY' } else { 'FAIL-3WAY' }
                                }
                            }
                            $beforeAfter = if ($newSize -gt 0) { ("{0} -> {1}" -f (Format-Bytes $job.OrigSize), (Format-Bytes $newSize)) } else { ("{0} -> N/A" -f (Format-Bytes $job.OrigSize)) }
                            Push-RecentEvent -List $recentEvents `
                                -Status 'RETRY' `
                                -File $job.Name `
                                -Attempt ("{0}/{1}" -f $job.Attempt, $maxAttemptsPerFile) `
                                -EmbeddedHash (Format-HashForUi -Hash $job.EmbeddedHash -NullHash $nullHash) `
                                -CalculatedBeforeHash (Format-HashForUi -Hash $job.PreCalcHash -NullHash $nullHash) `
                                -CalculatedAfterHash (Format-HashForUi -Hash $postCalcHash -NullHash $nullHash) `
                                -Verification $verification `
                                -BeforeAfter $beforeAfter `
                                -Saved 'N/A' `
                                -CompressionPct 'N/A' `
                                -Detail $failureReason
                            Write-RunLog -Level WARN -Message ("Retrying | File: {0} | NextAttempt: {1}/{2} | Reason: {3} | Embedded: {4} | CalcPre: {5} | CalcPost: {6} | Verification: {7} | STDERR: {8} | ErrLog: {9} | OutLog: {10}" -f $job.Original, $nextAttempt, $maxAttemptsPerFile, $failureReason, $job.EmbeddedHash, $job.PreCalcHash, $postCalcHash, $verification, $errText, $job.ErrLog, $job.OutLog)
                        }
                        else {
                            $failed++
                            $processed++
                            $verification = 'FAIL-INCOMPLETE'
                            if (-not [string]::IsNullOrWhiteSpace($job.PreCalcHash) -and -not [string]::IsNullOrWhiteSpace($postCalcHash)) {
                                if ($job.EmbeddedHash -eq $nullHash) {
                                    $verification = if ($job.PreCalcHash -eq $postCalcHash) { 'PASS-AUDIO' } else { 'FAIL-AUDIO' }
                                }
                                else {
                                    $verification = if (($job.EmbeddedHash -eq $job.PreCalcHash) -and ($job.EmbeddedHash -eq $postCalcHash)) { 'PASS-3WAY' } else { 'FAIL-3WAY' }
                                }
                            }
                            $beforeAfter = if ($newSize -gt 0) { ("{0} -> {1}" -f (Format-Bytes $job.OrigSize), (Format-Bytes $newSize)) } else { ("{0} -> N/A" -f (Format-Bytes $job.OrigSize)) }
                            Push-RecentEvent -List $recentEvents `
                                -Status 'FAIL' `
                                -File $job.Name `
                                -Attempt ("{0}/{1}" -f $job.Attempt, $maxAttemptsPerFile) `
                                -EmbeddedHash (Format-HashForUi -Hash $job.EmbeddedHash -NullHash $nullHash) `
                                -CalculatedBeforeHash (Format-HashForUi -Hash $job.PreCalcHash -NullHash $nullHash) `
                                -CalculatedAfterHash (Format-HashForUi -Hash $postCalcHash -NullHash $nullHash) `
                                -Verification $verification `
                                -BeforeAfter $beforeAfter `
                                -Saved 'N/A' `
                                -CompressionPct 'N/A' `
                                -Detail $failureReason
                            Write-RunLog -Level ERROR -Message ("Failed permanently | File: {0} | Attempts: {1}/{2} | Reason: {3} | Embedded: {4} | CalcPre: {5} | CalcPost: {6} | Verification: {7} | STDERR: {8} | ErrLog: {9} | OutLog: {10}" -f $job.Original, $job.Attempt, $maxAttemptsPerFile, $failureReason, $job.EmbeddedHash, $job.PreCalcHash, $postCalcHash, $verification, $errText, $job.ErrLog, $job.OutLog)
                        }
                    }

                    $w.Job = $null
                    $uiDirty = $true
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
                $embeddedHash = Try-GetFlacMd5 -Path $original
                if ($null -eq $embeddedHash) { $embeddedHash = $nullHash }
                $preCalcHash = $null

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
                    EmbeddedHash      = $embeddedHash
                    PreCalcHash       = $preCalcHash
                    OrigSize          = $origItem.Length
                    OrigAcl           = $origAcl
                    OrigCreationUtc   = $origItem.CreationTimeUtc
                    OrigLastWriteUtc  = $origItem.LastWriteTimeUtc
                    OrigLastAccessUtc = $origItem.LastAccessTimeUtc
                }
                $uiDirty = $true
            }
        }

        if (-not $interactive) {
            $nowUtc = [DateTime]::UtcNow
            if (($nowUtc - $lastStatusUtc) -ge $statusInterval) {
                $lastStatusUtc = $nowUtc
                Write-Host ("Progress: {0}/{1}  Failed: {2}  Saved: {3}" -f $processed, $totalFiles, $failed, (Format-Bytes $totalSavedBytes))
            }
        }

        Start-Sleep -Milliseconds 75
    }
}
finally {
    if ($interactive -and $null -ne $restoreTreatControlCAsInput) {
        try { [Console]::TreatControlCAsInput = $restoreTreatControlCAsInput } catch { }
    }
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
if ($runCanceled) {
    Write-Host "JOB CANCELED" -ForegroundColor Yellow
}
else {
    Write-Host "JOB COMPLETE"
}
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
