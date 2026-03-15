<#
.SYNOPSIS
Batch recompress FLAC files in place with decoded-audio integrity verification.

.DESCRIPTION
Scans a root folder recursively for .flac files, runs `flac -8 -e -p -V` into per-file
`.tmp` outputs, verifies decoded-audio MD5 consistency, and replaces originals only
after verification succeeds. The script keeps one rolling run log, writes an
EFC-style final summary log, emits a failure log only when needed, restores source
timestamps/ACLs, and supports safe cancellation.

.PARAMETER RootFolder
Root directory to scan recursively for FLAC files.

.PARAMETER LogFolder
Directory where run logs are stored. A timestamped subfolder is created per run.

.PARAMETER Threads
Optional worker count. Default: logical CPU count minus one.

.PARAMETER RunTests
Run Pester test suite and exit.

.PARAMETER InstallDeps
Auto-install required and optional dependencies without prompting.

.PARAMETER ShowVersion
Display version information and exit.

.NOTES
Requires `flac` and `metaflac` in PATH or beside the script.
Supports PowerShell 7+ on Windows and Linux.
#>

[CmdletBinding()]
param(
    [Parameter(Mandatory = $false, Position = 0, ValueFromRemainingArguments = $true)]
    [Alias('Path')]
    [AllowNull()]
    $RootFolder,

    [Parameter(Mandatory = $false)]
    [string]$LogFolder,

    [Parameter(Mandatory = $false)]
    [Alias('Workers')]
    [ValidateRange(1, [int]::MaxValue)]
    [int]$Threads,

    [Parameter(Mandatory = $false)]
    [switch]$RunTests,

    [Parameter(Mandatory = $false)]
    [switch]$InstallDeps,

    [Parameter(Mandatory = $false)]
    [switch]$ShowVersion
)

#region Script Header

if (($null -eq $PSVersionTable) -or ($null -eq $PSVersionTable.PSVersion) -or ($PSVersionTable.PSVersion.Major -lt 7)) {
    throw "Start-ExactFlacCrunch.ps1 requires PowerShell 7 or newer. Windows PowerShell 5.x is not supported. Rerun this script with pwsh 7."
}

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

#endregion Script Header

#region Constants

$script:Version = '2.0.0'
$script:NullHash = '00000000000000000000000000000000'
$script:MaxAttemptsPerFile = 3
$script:MoveItemMaxRetries = 5
$script:MoveItemRetryBaseMs = 200
$script:RunLogFlushIntervalSeconds = 2
$script:RunLogMaxBufferedLines = 40
$script:UiFrameIntervalMs = 250
$script:StatusIntervalSeconds = 10
$script:RecentEventsCapacity = 250
$script:RecentEventsDisplayCount = 25
$script:FlacEncodeArgs = '-8 -e -p -V -f'
$script:ProgressTailBytes = 24576L
$script:HashBufferSize = 65536
$script:ProgressCacheDelayMs = 700
$script:MinFlacVersion = '1.3.0'

#endregion Constants

#region Platform Detection

# Avoid overriding $IsWindows (automatic variable in PowerShell 7).
$script:IsWindowsHost = ($env:OS -eq 'Windows_NT')

#endregion Platform Detection

$script:RunLogBuffer = [System.Collections.Generic.List[string]]::new()
$script:RunLogLastFlushUtc = [DateTime]::UtcNow
$script:RunLogFlushInterval = [TimeSpan]::FromSeconds($script:RunLogFlushIntervalSeconds)
$script:VerboseUiMessages = [System.Collections.Generic.List[string]]::new()
$script:UiInteractiveMode = $false
$script:VerboseLogFile = $null

# Ensure console I/O uses UTF-8 so filenames with non-ASCII characters render correctly.
try {
    $utf8NoBom = [System.Text.UTF8Encoding]::new($false)
    [Console]::OutputEncoding = $utf8NoBom
    [Console]::InputEncoding = $utf8NoBom
    $OutputEncoding = $utf8NoBom
}
catch { }

# Handle -ShowVersion early exit
if ($ShowVersion) {
    Write-Host ("Exact Flac Cruncher v{0}" -f $script:Version)
    Write-Host ("PowerShell {0} on {1}" -f $PSVersionTable.PSVersion, $(if ($script:IsWindowsHost) { 'Windows' } else { 'Linux/macOS' }))
    return
}

#region Formatting Functions

function Format-Bytes {
    param(
        [Parameter(Mandatory)][long]$Bytes,
        [switch]$Signed
    )

    if ($Signed) {
        $displayBytes = $Bytes
        $valueBytes = [Math]::Abs($Bytes)
    }
    else {
        # Use the Int64 overload so large byte counts (e.g., multi-GB totals) don't overflow Int32
        $displayBytes = [Math]::Max([long]0, $Bytes)
        $valueBytes = $displayBytes
    }

    $units = @('B', 'KB', 'MB', 'GB', 'TB', 'PB')
    $i = 0
    $v = [double]$valueBytes
    while ($v -ge 1024 -and $i -lt ($units.Count - 1)) { $v /= 1024; $i++ }
    $number = $v.ToString('00.00', [System.Globalization.CultureInfo]::InvariantCulture)
    if ($Signed) {
        $sign = if ($displayBytes -lt 0) { '-' } else { ' ' }
        return ('{0}{1,7} {2}' -f $sign, $number, $units[$i])
    }

    return ('{0,8} {1}' -f $number, $units[$i])
}

function Format-HeaderCount {
    param([Parameter(Mandatory)][int]$Value)

    return ('{0,8}' -f $Value)
}

function Format-CountPair {
    param(
        [Parameter(Mandatory)][int]$Left,
        [Parameter(Mandatory)][int]$Right
    )

    return ("{0} / {1}" -f (Format-HeaderCount -Value $Left), (Format-HeaderCount -Value $Right))
}

function Format-Percent {
    param([Parameter(Mandatory)][double]$Value)

    return ('{0,8:N2}%' -f $Value)
}

function Format-LabelValue {
    param(
        [Parameter(Mandatory)][string]$Label,
        [Parameter(Mandatory)][AllowEmptyString()][string]$Value,
        [int]$LabelWidth = 20
    )

    return ("{0}: {1}" -f $Label.PadRight($LabelWidth), $Value)
}

function Format-EacValueLine {
    param(
        [Parameter(Mandatory)][string]$Label,
        [Parameter(Mandatory)][AllowEmptyString()][string]$Value,
        [int]$LabelWidth = 18
    )

    return ("     {0} {1}" -f $Label.PadRight($LabelWidth), $Value)
}

function Write-SummaryLine {
    param(
        [Parameter(Mandatory)][string]$Label,
        [Parameter(Mandatory)][AllowEmptyString()][string]$Value,
        [int]$LabelWidth = 16,
        [ConsoleColor]$LabelColor = [ConsoleColor]::Gray,
        [ConsoleColor]$ValueColor = [ConsoleColor]::White
    )

    Write-Host ($Label.PadRight($LabelWidth)) -NoNewline -ForegroundColor $LabelColor
    Write-Host ': ' -NoNewline -ForegroundColor DarkGray
    Write-Host $Value -ForegroundColor $ValueColor
}

function Format-TopCompressionLine {
    param(
        [Parameter(Mandatory)][int]$Rank,
        [Parameter(Mandatory)][psobject]$Entry,
        [switch]$LeafName
    )

    $displayPath = [string]$Entry.Path
    if ($LeafName) {
        $displayPath = [System.IO.Path]::GetFileName($displayPath)
    }

    return ("  {0}. Saved {1} ({2:N2}%) | {3}" -f $Rank, (Format-Bytes $Entry.SavedBytes), $Entry.SavedPct, $displayPath)
}

function Format-Elapsed {
    param([Parameter(Mandatory)][TimeSpan]$Elapsed)

    if ($Elapsed.Ticks -lt 0) { $Elapsed = [TimeSpan]::Zero }
    if ($Elapsed.Days -gt 0) {
        return ('{0}d {1:00}:{2:00}:{3:00}' -f $Elapsed.Days, $Elapsed.Hours, $Elapsed.Minutes, $Elapsed.Seconds)
    }
    return ('{0:00}:{1:00}:{2:00}' -f [int]$Elapsed.TotalHours, $Elapsed.Minutes, $Elapsed.Seconds)
}

function Format-EacLogDateTime {
    param([Parameter(Mandatory)][DateTime]$Value)

    return $Value.ToString('d. MMMM yyyy, H:mm', [System.Globalization.CultureInfo]::InvariantCulture)
}

#endregion Formatting Functions

#region Utility Functions

function Get-TextSha256 {
    param([Parameter(Mandatory)][AllowEmptyString()][string]$Text)

    $sha = [System.Security.Cryptography.SHA256]::Create()
    try {
        $bytes = [System.Text.Encoding]::UTF8.GetBytes($Text)
        $hashBytes = $sha.ComputeHash($bytes)
        return ([System.BitConverter]::ToString($hashBytes)).Replace('-', '')
    }
    finally {
        $sha.Dispose()
    }
}

function Get-SafeName {
    param(
        [Parameter(Mandatory)][string]$Value,
        [int]$MaxLength = 100
    )

    $invalid = [regex]::Escape(([string]::Join('', [System.IO.Path]::GetInvalidFileNameChars())))
    $safe = [regex]::Replace($Value, "[{0}]" -f $invalid, '_')
    # Remove wildcard tokens that can break Start-Process redirection binding.
    $safe = [regex]::Replace($safe, '[\[\]\*\?]', '_')
    # Keep output predictable by limiting to filename-safe characters.
    $safe = [regex]::Replace($safe, '[^A-Za-z0-9._ -]', '_')
    $safe = [regex]::Replace($safe, '\s+', ' ')
    $safe = $safe.Trim(' ', '.')
    if ([string]::IsNullOrWhiteSpace($safe)) { $safe = 'flac-job' }
    if ($safe.Length -gt $MaxLength) { $safe = $safe.Substring(0, $MaxLength) }
    return $safe
}

function Get-RootDisplayName {
    param(
        [Parameter(Mandatory)][string]$Path,
        [System.IO.FileSystemInfo]$Item
    )

    if ($null -ne $Item -and -not [string]::IsNullOrWhiteSpace($Item.Name)) {
        return $Item.Name
    }

    $trimmed = $Path.TrimEnd('\', '/')
    if ([string]::IsNullOrWhiteSpace($trimmed)) { return 'root' }

    $leaf = Split-Path -Path $trimmed -Leaf
    if (-not [string]::IsNullOrWhiteSpace($leaf)) { return $leaf }

    if ($trimmed -match '^[\\/]{2,}([^\\/]+)[\\/]+([^\\/]+)$') {
        return ('{0}_{1}' -f $Matches[1], $Matches[2])
    }

    return $trimmed
}

function Get-DefaultLogFolder {
    $homePath = [Environment]::GetFolderPath('UserProfile')
    if ([string]::IsNullOrWhiteSpace($homePath)) {
        $homePath = $HOME
    }
    if ([string]::IsNullOrWhiteSpace($homePath)) {
        $homePath = (Get-Location).Path
    }

    # On Windows, prefer Desktop if it exists
    if ($script:IsWindowsHost) {
        $desktopPath = [Environment]::GetFolderPath('Desktop')
        if ([string]::IsNullOrWhiteSpace($desktopPath) -and -not [string]::IsNullOrWhiteSpace($homePath)) {
            $desktopPath = Join-Path -Path $homePath -ChildPath 'Desktop'
        }
        if (-not [string]::IsNullOrWhiteSpace($desktopPath) -and (Test-Path -LiteralPath $desktopPath)) {
            return (Join-Path -Path $desktopPath -ChildPath 'EFC-logs')
        }
    }

    # On Linux or when Desktop is unavailable, use $HOME/EFC-logs directly
    return (Join-Path -Path $homePath -ChildPath 'EFC-logs')
}

#endregion Utility Functions

#region File Operations

function Test-DirectoryWriteAccess {
    param([Parameter(Mandatory)][string]$Path)

    if ([string]::IsNullOrWhiteSpace($Path)) { return $false }

    $probePath = Join-Path -Path $Path -ChildPath ('.efc-write-test-{0}.tmp' -f [guid]::NewGuid().ToString('N'))
    try {
        [System.IO.File]::WriteAllText($probePath, '')
        Remove-Item -LiteralPath $probePath -Force -ErrorAction SilentlyContinue
        return $true
    }
    catch {
        try {
            if (Test-Path -LiteralPath $probePath) {
                Remove-Item -LiteralPath $probePath -Force -ErrorAction SilentlyContinue
            }
        }
        catch { }

        return $false
    }
}

function Test-IsPermissionText {
    param([AllowEmptyString()][string]$Text)

    if ([string]::IsNullOrWhiteSpace($Text)) { return $false }
    return ($Text -match '(?i)access is denied|permission denied|unauthorizedaccess|unauthorized access|not authorized|read-only|readonly')
}

function Get-FriendlyPermissionMessage {
    param(
        [Parameter(Mandatory)][string]$Operation,
        [string]$Path,
        [AllowNull()]$Exception,
        [AllowEmptyString()][string]$Details
    )

    $isPermissionIssue = $false
    $detailText = $Details

    if ($null -ne $Exception) {
        if ($Exception -is [System.UnauthorizedAccessException] -or $Exception -is [System.Security.SecurityException]) {
            $isPermissionIssue = $true
        }
        elseif (Test-IsPermissionText -Text $Exception.Message) {
            $isPermissionIssue = $true
        }

        if ([string]::IsNullOrWhiteSpace($detailText)) {
            $detailText = $Exception.Message
        }
    }
    elseif (Test-IsPermissionText -Text $detailText) {
        $isPermissionIssue = $true
    }

    if (-not $isPermissionIssue) { return $null }

    $suffix = ''
    if (-not [string]::IsNullOrWhiteSpace($Path)) {
        $suffix = " | Path: $Path"
    }

    if ([string]::IsNullOrWhiteSpace($detailText)) {
        return "Permission denied while $Operation$suffix"
    }

    return "Permission denied while $Operation$suffix | Detail: $detailText"
}

function Escape-WildcardPath {
    param([Parameter(Mandatory)][string]$Path)
    return [System.Management.Automation.WildcardPattern]::Escape($Path)
}

#endregion File Operations

#region Logging

function Flush-RunLog {
    param([switch]$Force)

    if ([string]::IsNullOrWhiteSpace($script:LogFile)) { return }
    if ($script:RunLogBuffer.Count -eq 0) { return }

    $nowUtc = [DateTime]::UtcNow
    if (-not $Force) {
        $withinCount = ($script:RunLogBuffer.Count -lt $script:RunLogMaxBufferedLines)
        $withinTime = (($nowUtc - $script:RunLogLastFlushUtc) -lt $script:RunLogFlushInterval)
        if ($withinCount -and $withinTime) { return }
    }

    $linesToWrite = @($script:RunLogBuffer)
    $script:RunLogBuffer.Clear()
    Add-Content -LiteralPath $script:LogFile -Value $linesToWrite -Encoding UTF8
    $script:RunLogLastFlushUtc = $nowUtc
}

function Write-RunLog {
    param(
        [Parameter(Mandatory)][string]$Message,
        [ValidateSet('INFO', 'WARN', 'ERROR', 'SUCCESS')]
        [string]$Level = 'INFO'
    )

    if ([string]::IsNullOrWhiteSpace($script:LogFile)) { return }
    $ts = Get-Date -Format 'yyyy-MM-ddTHH:mm:ss.fffK'
    $script:RunLogBuffer.Add(("{0} | {1} | {2}" -f $ts, $Level, $Message)) | Out-Null
    Flush-RunLog
}

function Test-VerboseUiEnabled {
    return ($VerbosePreference -ne 'SilentlyContinue')
}

function Write-VerboseUi {
    param([Parameter(Mandatory)][string]$Message)

    if (-not (Test-VerboseUiEnabled)) { return }

    if (-not $script:UiInteractiveMode) {
        Write-Verbose $Message
    }

    if ($null -eq $script:VerboseUiMessages) {
        $script:VerboseUiMessages = [System.Collections.Generic.List[string]]::new()
    }

    $timestamped = "{0} | {1}" -f (Get-Date -Format 'HH:mm:ss'), $Message
    $script:VerboseUiMessages.Insert(0, $timestamped)
    if ($script:VerboseUiMessages.Count -gt 20) {
        $script:VerboseUiMessages.RemoveRange(20, $script:VerboseUiMessages.Count - 20)
    }

    if (-not [string]::IsNullOrWhiteSpace($script:VerboseLogFile)) {
        try {
            $diskLine = "{0} | {1}" -f (Get-Date -Format 'yyyy-MM-ddTHH:mm:ss.fffK'), $Message
            Add-Content -LiteralPath $script:VerboseLogFile -Value $diskLine -Encoding UTF8
        }
        catch { }
    }
}

function Format-ErrSnippet {
    param(
        [AllowEmptyString()]
        [string]$Text,
        [int]$MaxLength = 400
    )

    if ([string]::IsNullOrWhiteSpace($Text)) { return '(none)' }

    $singleLine = [regex]::Replace($Text, '\s+', ' ').Trim()
    if ($singleLine.Length -le $MaxLength) { return $singleLine }
    return ("{0}..." -f $singleLine.Substring(0, $MaxLength))
}

#endregion Logging

#region File Metadata

function Safe-RemoveFile {
    param([Parameter(Mandatory)][string]$Path)
    try {
        if (Test-Path -LiteralPath $Path) {
            Remove-Item -LiteralPath $Path -Force -ErrorAction SilentlyContinue
        }
    }
    catch { }
}

function Get-FileMetadataSnapshot {
    param(
        [Parameter(Mandatory)][string]$Path,
        [System.IO.FileSystemInfo]$Item
    )

    if ($null -eq $Item) {
        try {
            $Item = Get-Item -LiteralPath $Path -Force -ErrorAction Stop
        }
        catch {
            return $null
        }
    }

    $acl = $null
    try {
        $acl = Get-Acl -LiteralPath $Path -ErrorAction Stop
    }
    catch { }

    return [PSCustomObject]@{
        CreationTimeUtc   = [DateTime]$Item.CreationTimeUtc
        LastAccessTimeUtc = [DateTime]$Item.LastAccessTimeUtc
        LastWriteTimeUtc  = [DateTime]$Item.LastWriteTimeUtc
        Attributes        = [System.IO.FileAttributes]$Item.Attributes
        Acl               = $acl
    }
}

function Restore-FileMetadata {
    param(
        [Parameter(Mandatory)][string]$Path,
        [AllowNull()]$Snapshot
    )

    if ($null -eq $Snapshot) { return $true }

    $issues = [System.Collections.Generic.List[string]]::new()

    try {
        [System.IO.File]::SetCreationTimeUtc($Path, [DateTime]$Snapshot.CreationTimeUtc)
    }
    catch {
        $issues.Add(("CreationTime: {0}" -f $_.Exception.Message)) | Out-Null
    }

    try {
        [System.IO.File]::SetLastAccessTimeUtc($Path, [DateTime]$Snapshot.LastAccessTimeUtc)
    }
    catch {
        $issues.Add(("LastAccessTime: {0}" -f $_.Exception.Message)) | Out-Null
    }

    try {
        [System.IO.File]::SetLastWriteTimeUtc($Path, [DateTime]$Snapshot.LastWriteTimeUtc)
    }
    catch {
        $issues.Add(("LastWriteTime: {0}" -f $_.Exception.Message)) | Out-Null
    }

    if ($Snapshot.PSObject.Properties.Name -contains 'Acl' -and $null -ne $Snapshot.Acl) {
        try {
            Set-Acl -LiteralPath $Path -AclObject $Snapshot.Acl -ErrorAction Stop
        }
        catch {
            $issues.Add(("ACL: {0}" -f $_.Exception.Message)) | Out-Null
        }
    }

    try {
        [System.IO.File]::SetAttributes($Path, [System.IO.FileAttributes]$Snapshot.Attributes)
    }
    catch {
        $issues.Add(("Attributes: {0}" -f $_.Exception.Message)) | Out-Null
    }

    if ($issues.Count -eq 0) { return $true }

    $issueText = [string]::Join(' | ', $issues)
    Write-RunLog -Level WARN -Message ("File metadata restore was only partially successful | File: {0} | Detail: {1}" -f $Path, $issueText)
    Write-VerboseUi -Message ("Metadata restore partial | File: {0} | Detail: {1}" -f $Path, (Format-ErrSnippet -Text $issueText -MaxLength 250))
    return $false
}

function Quote-WinArg {
    <#
      Windows CreateProcess-style quoting for Start-Process when UseShellExecute is false
      (for redirected stdout/stderr). Prevents path splitting for spaced filenames.
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

#endregion File Metadata

#region FLAC Operations

function Try-GetFlacMd5 {
    param([Parameter(Mandatory)][string]$Path)
    try {
        # End option parsing in case a path starts with '-'.
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
        # Preserve file modtime when rewriting metadata.
        $null = (& metaflac --preserve-modtime --set-md5sum=$ExpectedMd5 -- $Path 2>$null)
    }
    catch {
        return $false
    }

    $after = Try-GetFlacMd5 -Path $Path
    if ([string]::IsNullOrWhiteSpace($after)) { return $false }
    return ($after -eq $ExpectedMd5)
}

#endregion FLAC Operations

#region Tool Resolution

function Get-OptionalToolSearchDirectories {
    $dirs = [System.Collections.Generic.List[string]]::new()

    if ($script:IsWindowsHost) {
        $candidates = @(
            "$env:LOCALAPPDATA\Microsoft\WinGet\Links",
            "$env:LOCALAPPDATA\Microsoft\WindowsApps",
            'C:\libjpeg-turbo64\bin',
            'C:\libjpeg-turbo\bin',
            "$env:ProgramFiles\libjpeg-turbo\bin",
            "$env:ProgramFiles\libjpeg-turbo64\bin",
            "$env:ProgramFiles(x86)\libjpeg-turbo\bin"
        )
    }
    else {
        $candidates = @(
            '/usr/bin',
            '/usr/local/bin',
            "$HOME/.local/bin",
            '/snap/bin',
            '/usr/lib/libjpeg-turbo/bin'
        )
    }

    foreach ($candidate in $candidates) {
        if ([string]::IsNullOrWhiteSpace($candidate)) { continue }
        if (-not (Test-Path -LiteralPath $candidate)) { continue }
        if ($dirs -notcontains $candidate) {
            $dirs.Add($candidate) | Out-Null
        }
    }

    return @($dirs)
}

function Find-ToolInDirectory {
    param(
        [Parameter(Mandatory)][string[]]$Names,
        [Parameter(Mandatory)][string]$Directory
    )

    foreach ($name in $Names) {
        $candidate = Join-Path -Path $Directory -ChildPath $name
        if (Test-Path -LiteralPath $candidate) {
            try {
                $item = Get-Item -LiteralPath $candidate -Force -ErrorAction Stop
                if (-not $item.PSIsContainer) {
                    return [PSCustomObject]@{
                        Name   = [System.IO.Path]::GetFileNameWithoutExtension($item.Name)
                        Source = $item.FullName
                    }
                }
            }
            catch { }
        }
    }

    return $null
}

function Resolve-OptionalTool {
    param(
        [Parameter(Mandatory)][string[]]$Names,
        [string]$BaseDirectory
    )

    if (-not [string]::IsNullOrWhiteSpace($BaseDirectory)) {
        $found = Find-ToolInDirectory -Names $Names -Directory $BaseDirectory
        if ($null -ne $found) { return $found }
    }

    foreach ($searchDir in @(Get-OptionalToolSearchDirectories)) {
        $found = Find-ToolInDirectory -Names $Names -Directory $searchDir
        if ($null -ne $found) { return $found }
    }

    foreach ($name in $Names) {
        $commandName = [System.IO.Path]::GetFileNameWithoutExtension($name)
        $cmd = Get-Command $commandName -ErrorAction SilentlyContinue
        if ($cmd) {
            return $cmd
        }
    }

    return $null
}

function Resolve-RequiredTool {
    param(
        [Parameter(Mandatory)][string[]]$Names,
        [string]$BaseDirectory,
        [Parameter(Mandatory)][string]$DisplayName
    )

    $cmd = Resolve-OptionalTool -Names $Names -BaseDirectory $BaseDirectory
    if ($null -ne $cmd) {
        return $cmd
    }

    throw ("'{0}' not found. Add it to PATH or place it beside this script." -f $DisplayName)
}

function Refresh-ProcessPathFromRegistry {
    try {
        $machinePath = [Environment]::GetEnvironmentVariable('Path', 'Machine')
        $userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
        $combined = @($machinePath, $userPath) | Where-Object { -not [string]::IsNullOrWhiteSpace($_) }
        if ($combined.Count -gt 0) {
            $env:PATH = ($combined -join [System.IO.Path]::PathSeparator)
        }
    }
    catch { }
}

#endregion Tool Resolution

#region Dependency Bootstrap

function Get-SystemPackageManager {
    if ($script:IsWindowsHost) {
        $winget = Get-Command winget -ErrorAction SilentlyContinue
        if ($winget) { return 'winget' }
        $choco = Get-Command choco -ErrorAction SilentlyContinue
        if ($choco) { return 'choco' }
        return $null
    }

    $aptGet = Get-Command apt-get -ErrorAction SilentlyContinue
    if ($aptGet) { return 'apt-get' }
    $dnf = Get-Command dnf -ErrorAction SilentlyContinue
    if ($dnf) { return 'dnf' }
    $pacman = Get-Command pacman -ErrorAction SilentlyContinue
    if ($pacman) { return 'pacman' }
    return $null
}

function Install-RequiredDependencies {
    param([switch]$NonInteractive)

    $pkgMgr = Get-SystemPackageManager
    if ($null -eq $pkgMgr) {
        throw "No supported package manager found. Install flac and metaflac manually and add them to PATH."
    }

    if (-not $NonInteractive) {
        $reply = ''
        try {
            $reply = [string](Read-Host ("flac/metaflac not found. Attempt install via {0}? [y/N]" -f $pkgMgr))
        }
        catch { return }
        if ($reply.Trim().ToLowerInvariant() -notin @('y', 'yes')) { return }
    }

    Write-Host ("Installing FLAC tools via {0}..." -f $pkgMgr) -ForegroundColor Cyan
    switch ($pkgMgr) {
        'winget' {
            & winget install FLAC.FLAC -e --source winget --accept-source-agreements --accept-package-agreements 2>&1 | Out-Null
            Refresh-ProcessPathFromRegistry
        }
        'choco' {
            & choco install flac -y 2>&1 | Out-Null
            Refresh-ProcessPathFromRegistry
        }
        'apt-get' {
            & sudo apt-get install -y flac 2>&1 | Out-Null
        }
        'dnf' {
            & sudo dnf install -y flac 2>&1 | Out-Null
        }
        'pacman' {
            & sudo pacman -S --noconfirm flac 2>&1 | Out-Null
        }
    }
}

function Install-OptionalDependencies {
    param(
        [switch]$NeedPng,
        [switch]$NeedJpeg,
        [switch]$NonInteractive
    )

    $pkgMgr = Get-SystemPackageManager
    if ($null -eq $pkgMgr) { return }

    if (-not $NonInteractive) {
        Write-Host ""
        Write-Host "Optional album-art tools are missing." -ForegroundColor Yellow
        if ($NeedPng) {
            Write-Host "  PNG  : missing ('oxipng' preferred, or 'pngcrush')." -ForegroundColor DarkYellow
        }
        if ($NeedJpeg) {
            Write-Host "  JPEG : missing ('jpegtran')." -ForegroundColor DarkYellow
        }

        $reply = ''
        try {
            $reply = [string](Read-Host ("Attempt install via {0}? [y/N]" -f $pkgMgr))
        }
        catch { return }
        if ($reply.Trim().ToLowerInvariant() -notin @('y', 'yes')) { return }
    }

    switch ($pkgMgr) {
        'winget' {
            if ($NeedPng) {
                Write-Host "Installing PNG optimizer (oxipng)..." -ForegroundColor Cyan
                try { $null = & winget install Shssoichiro.Oxipng -e --source winget --accept-source-agreements --accept-package-agreements 2>&1 }
                catch { Write-Warning ("winget could not install oxipng: {0}" -f $_.Exception.Message) }
            }
            if ($NeedJpeg) {
                Write-Host "Installing JPEG optimizer (jpegtran)..." -ForegroundColor Cyan
                try { $null = & winget install --id libjpeg-turbo.libjpeg-turbo.VC -e --source winget --accept-source-agreements --accept-package-agreements 2>&1 }
                catch { Write-Warning ("winget could not install libjpeg-turbo: {0}" -f $_.Exception.Message) }
            }
            Refresh-ProcessPathFromRegistry
        }
        'choco' {
            if ($NeedPng) {
                Write-Host "Installing PNG optimizer (oxipng)..." -ForegroundColor Cyan
                try { $null = & choco install oxipng -y 2>&1 }
                catch { Write-Warning ("choco could not install oxipng: {0}" -f $_.Exception.Message) }
            }
            if ($NeedJpeg) {
                Write-Host "Installing JPEG optimizer (jpegtran)..." -ForegroundColor Cyan
                try { $null = & choco install libjpeg-turbo -y 2>&1 }
                catch { Write-Warning ("choco could not install libjpeg-turbo: {0}" -f $_.Exception.Message) }
            }
            Refresh-ProcessPathFromRegistry
        }
        'apt-get' {
            if ($NeedPng) {
                Write-Host "Installing PNG optimizer (oxipng)..." -ForegroundColor Cyan
                try { $null = & sudo apt-get install -y oxipng 2>&1 }
                catch { Write-Warning ("apt-get could not install oxipng: {0}" -f $_.Exception.Message) }
            }
            if ($NeedJpeg) {
                Write-Host "Installing JPEG optimizer (jpegtran)..." -ForegroundColor Cyan
                try { $null = & sudo apt-get install -y libjpeg-turbo-progs 2>&1 }
                catch { Write-Warning ("apt-get could not install libjpeg-turbo-progs: {0}" -f $_.Exception.Message) }
            }
        }
        'dnf' {
            if ($NeedPng) {
                try { $null = & sudo dnf install -y oxipng 2>&1 }
                catch { Write-Warning ("dnf could not install oxipng: {0}" -f $_.Exception.Message) }
            }
            if ($NeedJpeg) {
                try { $null = & sudo dnf install -y libjpeg-turbo-utils 2>&1 }
                catch { Write-Warning ("dnf could not install libjpeg-turbo-utils: {0}" -f $_.Exception.Message) }
            }
        }
        'pacman' {
            if ($NeedPng) {
                try { $null = & sudo pacman -S --noconfirm oxipng 2>&1 }
                catch { Write-Warning ("pacman could not install oxipng: {0}" -f $_.Exception.Message) }
            }
            if ($NeedJpeg) {
                try { $null = & sudo pacman -S --noconfirm libjpeg-turbo 2>&1 }
                catch { Write-Warning ("pacman could not install libjpeg-turbo: {0}" -f $_.Exception.Message) }
            }
        }
    }
}

function Test-FlacVersion {
    param([Parameter(Mandatory)][string]$FlacExePath)

    try {
        $versionOutput = & $FlacExePath --version 2>&1
        $versionText = [string]($versionOutput | Select-Object -First 1)
        if ($versionText -match '(\d+\.\d+\.\d+)') {
            $detected = [version]$Matches[1]
            $minimum = [version]$script:MinFlacVersion
            if ($detected -lt $minimum) {
                Write-Warning ("flac version {0} detected; minimum recommended is {1}. Some features may not work correctly." -f $detected, $minimum)
            }
            return $detected
        }
    }
    catch { }
    return $null
}

function Invoke-OptionalOptimizerInstallPrompt {
    param(
        [AllowNull()]$PngOptimizerCommand,
        [AllowNull()]$JpegOptimizerCommand,
        [string]$BaseDirectory,
        [switch]$Interactive
    )

    $result = [PSCustomObject]@{
        PngOptimizerCommand  = $PngOptimizerCommand
        JpegOptimizerCommand = $JpegOptimizerCommand
        Prompted             = $false
    }

    if (-not $Interactive) {
        return $result
    }

    $needPng = ($null -eq $PngOptimizerCommand)
    $needJpeg = ($null -eq $JpegOptimizerCommand)
    if (-not $needPng -and -not $needJpeg) {
        return $result
    }

    $wingetCmd = Get-Command winget -ErrorAction SilentlyContinue
    if (-not $wingetCmd) {
        return $result
    }

    Write-Host ""
    Write-Host "Optional album-art tools are missing." -ForegroundColor Yellow
    if ($needPng) {
        Write-Host "  PNG  : missing ('oxipng' preferred, or 'pngcrush')." -ForegroundColor DarkYellow
    }
    if ($needJpeg) {
        Write-Host "  JPEG : missing ('jpegtran')." -ForegroundColor DarkYellow
    }
    if (-not [string]::IsNullOrWhiteSpace($BaseDirectory)) {
        Write-Host ("You can also place the EXE(s) next to this script: {0}" -f $BaseDirectory) -ForegroundColor DarkGray
    }

    $reply = ''
    try {
        $reply = [string](Read-Host "Attempt best-effort install with winget now? [y/N]")
    }
    catch {
        return $result
    }

    if ($reply.Trim().ToLowerInvariant() -notin @('y', 'yes')) {
        return $result
    }

    $result.Prompted = $true

    if ($needPng) {
        Write-Host "Installing PNG optimizer with winget (oxipng)..." -ForegroundColor Cyan
        try {
            $null = & $wingetCmd.Source install --id Shssoichiro.Oxipng -e --source winget --accept-source-agreements --accept-package-agreements 2>&1
            if ($LASTEXITCODE -ne 0) {
                Write-Warning "winget could not install oxipng automatically."
            }
        }
        catch {
            Write-Warning ("winget could not install oxipng automatically: {0}" -f $_.Exception.Message)
        }
    }

    if ($needJpeg) {
        Write-Host "Installing JPEG optimizer with winget (libjpeg-turbo / jpegtran)..." -ForegroundColor Cyan
        try {
            $null = & $wingetCmd.Source install --id libjpeg-turbo.libjpeg-turbo.VC -e --source winget --accept-source-agreements --accept-package-agreements 2>&1
            if ($LASTEXITCODE -ne 0) {
                Write-Warning "winget could not install libjpeg-turbo (jpegtran) automatically."
            }
        }
        catch {
            Write-Warning ("winget could not install libjpeg-turbo (jpegtran) automatically: {0}" -f $_.Exception.Message)
        }
    }

    Refresh-ProcessPathFromRegistry
    $result.PngOptimizerCommand = Resolve-OptionalTool -Names @('oxipng.exe', 'pngcrush.exe') -BaseDirectory $BaseDirectory
    $result.JpegOptimizerCommand = Resolve-OptionalTool -Names @('jpegtran.exe') -BaseDirectory $BaseDirectory

    return $result
}

#endregion Dependency Bootstrap

#region Album Art Optimization

function Get-ImageSignatureKind {
    param([Parameter(Mandatory)][string]$Path)

    try {
        $fs = [System.IO.File]::OpenRead($Path)
        try {
            $buffer = New-Object byte[] 8
            $read = $fs.Read($buffer, 0, $buffer.Length)
        }
        finally {
            $fs.Dispose()
        }
    }
    catch {
        return $null
    }

    if ($read -ge 8 -and
        $buffer[0] -eq 0x89 -and
        $buffer[1] -eq 0x50 -and
        $buffer[2] -eq 0x4E -and
        $buffer[3] -eq 0x47 -and
        $buffer[4] -eq 0x0D -and
        $buffer[5] -eq 0x0A -and
        $buffer[6] -eq 0x1A -and
        $buffer[7] -eq 0x0A) {
        return 'PNG'
    }

    if ($read -ge 3 -and
        $buffer[0] -eq 0xFF -and
        $buffer[1] -eq 0xD8 -and
        $buffer[2] -eq 0xFF) {
        return 'JPEG'
    }

    return $null
}

function Get-FlacPictureBlocks {
    param(
        [Parameter(Mandatory)][string]$MetaflacExePath,
        [Parameter(Mandatory)][string]$Path
    )

    $items = [System.Collections.Generic.List[object]]::new()
    $lines = @()
    $listExitCode = 0
    try {
        $lines = @(& $MetaflacExePath --list --block-type=PICTURE -- $Path 2>$null)
        $listExitCode = $LASTEXITCODE
    }
    catch {
        return @()
    }

    if ($listExitCode -ne 0 -or $lines.Count -eq 0) {
        return @()
    }

    $current = $null
    foreach ($rawLine in $lines) {
        $line = [string]$rawLine
        $trimmed = $line.Trim()

        if ($trimmed -match '^METADATA block #(?<num>\d+)') {
            if ($null -ne $current) {
                $items.Add([PSCustomObject]$current) | Out-Null
            }

            $current = [ordered]@{
                BlockNumber = [int]$Matches['num']
                PictureType = 3
                MimeType    = ''
                Description = ''
                Width       = 0
                Height      = 0
                Depth       = 0
                Colors      = 0
            }
            continue
        }

        if ($null -eq $current) { continue }

        if ($trimmed -match '^type:\s*(?<num>\d+)\s+\((?<label>.+)\)$') {
            if ($Matches['label'] -ne 'PICTURE') {
                $current.PictureType = [int]$Matches['num']
            }
            continue
        }

        if ($trimmed -match '^MIME type:\s*(?<value>.*)$') {
            $current.MimeType = [string]$Matches['value']
            continue
        }

        if ($trimmed -match '^description:\s*(?<value>.*)$') {
            $current.Description = [string]$Matches['value']
            continue
        }

        if ($trimmed -match '^width:\s*(?<value>\d+)$') {
            $current.Width = [int]$Matches['value']
            continue
        }

        if ($trimmed -match '^height:\s*(?<value>\d+)$') {
            $current.Height = [int]$Matches['value']
            continue
        }

        if ($trimmed -match '^depth:\s*(?<value>\d+)$') {
            $current.Depth = [int]$Matches['value']
            continue
        }

        if ($trimmed -match '^colors:\s*(?<value>\d+)$') {
            $current.Colors = [int]$Matches['value']
            continue
        }
    }

    if ($null -ne $current) {
        $items.Add([PSCustomObject]$current) | Out-Null
    }

    return @($items)
}

function New-MetaflacPictureSpec {
    param(
        [Parameter(Mandatory)]$Block,
        [Parameter(Mandatory)][string]$FilePath
    )

    $mimeType = [string]$Block.MimeType
    if ([string]::IsNullOrWhiteSpace($mimeType) -or $mimeType -eq '-->') {
        return $null
    }

    $description = [string]$Block.Description
    if ($description -match '[\r\n|]') {
        return $null
    }

    $dimensionSpec = ''
    if ($Block.Width -gt 0 -and $Block.Height -gt 0 -and $Block.Depth -gt 0) {
        $dimensionSpec = "{0}x{1}x{2}" -f $Block.Width, $Block.Height, $Block.Depth
        if ($Block.Colors -gt 0) {
            $dimensionSpec = "{0}/{1}" -f $dimensionSpec, $Block.Colors
        }
    }

    return ("{0}|{1}|{2}|{3}|{4}" -f [int]$Block.PictureType, $mimeType, $description, $dimensionSpec, $FilePath)
}

function Optimize-FlacAlbumArt {
    param(
        [Parameter(Mandatory)][string]$Path,
        [Parameter(Mandatory)][string]$MetaflacExePath,
        [AllowNull()][string]$PngOptimizerPath,
        [AllowNull()][string]$JpegOptimizerPath,
        [Parameter(Mandatory)][string]$ScratchDir,
        [Parameter(Mandatory)][string]$JobId
    )

    $result = [PSCustomObject]@{
        Changed             = $false
        SavedBytes          = 0L
        RawSavedBytes       = 0L
        CandidateSavedBytes = 0L
        BlocksOptimized     = 0
        Summary             = ''
    }

    $pictureBlocks = @(Get-FlacPictureBlocks -MetaflacExePath $MetaflacExePath -Path $Path)

    $scanMessage = ("Album art scan | File: {0} | PictureBlocks: {1} | PNG Tool: {2} | JPEG Tool: {3}" -f $Path, $pictureBlocks.Count, $(if ([string]::IsNullOrWhiteSpace($PngOptimizerPath)) { 'OFF' } else { $PngOptimizerPath }), $(if ([string]::IsNullOrWhiteSpace($JpegOptimizerPath)) { 'OFF' } else { $JpegOptimizerPath }))
    Write-RunLog -Level INFO -Message $scanMessage
    Write-VerboseUi -Message $scanMessage

    $stagedPictures = [System.Collections.Generic.List[object]]::new()
    foreach ($block in $pictureBlocks) {
        $baseName = "{0}_b{1}" -f $JobId, $block.BlockNumber
        $picturePath = Join-Path -Path $ScratchDir -ChildPath ("{0}.picture.tmp" -f $baseName)
        $jpegOutputPath = Join-Path -Path $ScratchDir -ChildPath ("{0}.jpegopt.tmp" -f $baseName)

        Safe-RemoveFile -Path $picturePath
        Safe-RemoveFile -Path $jpegOutputPath

        $exportOutput = @()
        $exportExitCode = 0
        try {
            $exportOutput = @(& $MetaflacExePath "--block-number=$($block.BlockNumber)" "--export-picture-to=$picturePath" -- $Path 2>&1)
            $exportExitCode = $LASTEXITCODE
        }
        catch {
            Write-VerboseUi -Message ("metaflac export threw | File: {0} | Block: {1} | Detail: {2}" -f $Path, $block.BlockNumber, $_.Exception.Message)
            Safe-RemoveFile -Path $picturePath
            Safe-RemoveFile -Path $jpegOutputPath
            continue
        }

        if ($exportOutput.Count -gt 0) {
            Write-VerboseUi -Message ("metaflac export | File: {0} | Block: {1} | Output: {2}" -f $Path, $block.BlockNumber, (Format-ErrSnippet -Text ([string]::Join(' ', @($exportOutput | ForEach-Object { [string]$_ }))) -MaxLength 250))
        }

        if ($exportExitCode -ne 0 -or -not (Test-Path -LiteralPath $picturePath)) {
            Write-VerboseUi -Message ("Album art export failed | File: {0} | Block: {1} | Exit: {2}" -f $Path, $block.BlockNumber, $LASTEXITCODE)
            Safe-RemoveFile -Path $picturePath
            Safe-RemoveFile -Path $jpegOutputPath
            continue
        }

        $pictureKind = Get-ImageSignatureKind -Path $picturePath
        if ($null -eq $pictureKind) {
            $mimeType = [string]$block.MimeType
            if ($mimeType -match '(?i)image/png') {
                $pictureKind = 'PNG'
            }
            elseif ($mimeType -match '(?i)image/jpe?g') {
                $pictureKind = 'JPEG'
            }
        }

        $beforeSize = (Get-Item -LiteralPath $picturePath -Force).Length
        $afterSize = $beforeSize

        if ($pictureKind -eq 'PNG') {
            if ([string]::IsNullOrWhiteSpace($PngOptimizerPath)) {
                Write-RunLog -Level INFO -Message ("Album art block skipped | File: {0} | Block: {1} | Kind: PNG | Reason: PNG optimizer unavailable" -f $Path, $block.BlockNumber)
                Safe-RemoveFile -Path $picturePath
                Safe-RemoveFile -Path $jpegOutputPath
                continue
            }

            $pngTool = [System.IO.Path]::GetFileNameWithoutExtension($PngOptimizerPath).ToLowerInvariant()
            $pngOutput = @()
            $pngExitCode = 0
            try {
                if ($pngTool -eq 'pngcrush') {
                    $pngOutput = @(& $PngOptimizerPath '-q' '-ow' $picturePath 2>&1)
                }
                else {
                    $pngOutput = @(& $PngOptimizerPath '-o' '4' '-q' $picturePath 2>&1)
                }
                $pngExitCode = $LASTEXITCODE
            }
            catch {
                Write-RunLog -Level WARN -Message ("Album art PNG optimization failed | File: {0} | Block: {1} | Tool: {2} | Detail: {3}" -f $Path, $block.BlockNumber, $PngOptimizerPath, $_.Exception.Message)
                Safe-RemoveFile -Path $picturePath
                Safe-RemoveFile -Path $jpegOutputPath
                continue
            }

            if ($pngExitCode -ne 0 -or -not (Test-Path -LiteralPath $picturePath)) {
                Write-RunLog -Level WARN -Message ("Album art PNG optimization failed | File: {0} | Block: {1} | Tool: {2} | Exit: {3}" -f $Path, $block.BlockNumber, $PngOptimizerPath, $pngExitCode)
                Safe-RemoveFile -Path $picturePath
                Safe-RemoveFile -Path $jpegOutputPath
                continue
            }

            if ($pngOutput.Count -gt 0) {
                Write-VerboseUi -Message ("PNG optimizer | File: {0} | Block: {1} | Output: {2}" -f $Path, $block.BlockNumber, (Format-ErrSnippet -Text ([string]::Join(' ', @($pngOutput | ForEach-Object { [string]$_ }))) -MaxLength 250))
            }

            $afterSize = (Get-Item -LiteralPath $picturePath -Force).Length
        }
        elseif ($pictureKind -eq 'JPEG') {
            if ([string]::IsNullOrWhiteSpace($JpegOptimizerPath)) {
                Write-RunLog -Level INFO -Message ("Album art block skipped | File: {0} | Block: {1} | Kind: JPEG | Reason: JPEG optimizer unavailable" -f $Path, $block.BlockNumber)
                Safe-RemoveFile -Path $picturePath
                Safe-RemoveFile -Path $jpegOutputPath
                continue
            }

            $jpegOutput = @()
            $jpegExitCode = 0
            try {
                $jpegOutput = @(& $JpegOptimizerPath '-copy' 'all' '-optimize' '-outfile' $jpegOutputPath $picturePath 2>&1)
                $jpegExitCode = $LASTEXITCODE
            }
            catch {
                Write-RunLog -Level WARN -Message ("Album art JPEG optimization failed | File: {0} | Block: {1} | Tool: {2} | Detail: {3}" -f $Path, $block.BlockNumber, $JpegOptimizerPath, $_.Exception.Message)
                Safe-RemoveFile -Path $picturePath
                Safe-RemoveFile -Path $jpegOutputPath
                continue
            }

            if ($jpegOutput.Count -gt 0) {
                Write-VerboseUi -Message ("JPEG optimizer | File: {0} | Block: {1} | Output: {2}" -f $Path, $block.BlockNumber, (Format-ErrSnippet -Text ([string]::Join(' ', @($jpegOutput | ForEach-Object { [string]$_ }))) -MaxLength 250))
            }

            if ($jpegExitCode -ne 0 -or -not (Test-Path -LiteralPath $jpegOutputPath)) {
                Write-RunLog -Level WARN -Message ("Album art JPEG optimization failed | File: {0} | Block: {1} | Tool: {2} | Exit: {3}" -f $Path, $block.BlockNumber, $JpegOptimizerPath, $jpegExitCode)
                Safe-RemoveFile -Path $picturePath
                Safe-RemoveFile -Path $jpegOutputPath
                continue
            }

            $candidateSize = (Get-Item -LiteralPath $jpegOutputPath -Force).Length
            if ($candidateSize -lt $beforeSize) {
                try {
                    Move-Item -LiteralPath $jpegOutputPath -Destination $picturePath -Force
                }
                catch {
                    Write-RunLog -Level WARN -Message ("Album art JPEG optimization failed | File: {0} | Block: {1} | Detail: {2}" -f $Path, $block.BlockNumber, $_.Exception.Message)
                    Safe-RemoveFile -Path $picturePath
                    Safe-RemoveFile -Path $jpegOutputPath
                    continue
                }
                $afterSize = $candidateSize
            }
            else {
                Safe-RemoveFile -Path $jpegOutputPath
            }
        }
        else {
            Write-RunLog -Level INFO -Message ("Album art block skipped | File: {0} | Block: {1} | MIME: {2} | Reason: Unsupported/undetected image type" -f $Path, $block.BlockNumber, $block.MimeType)
            Safe-RemoveFile -Path $picturePath
            Safe-RemoveFile -Path $jpegOutputPath
            continue
        }

        $pictureSaved = $beforeSize - $afterSize
        if ($pictureSaved -le 0) {
            Write-RunLog -Level INFO -Message ("Album art block produced no gain | File: {0} | Block: {1} | Kind: {2} | Before: {3} | After: {4}" -f $Path, $block.BlockNumber, $pictureKind, (Format-Bytes $beforeSize), (Format-Bytes $afterSize))
            Safe-RemoveFile -Path $picturePath
            Safe-RemoveFile -Path $jpegOutputPath
            continue
        }

        $blockSavedMessage = ("Album art block optimized | File: {0} | Block: {1} | Kind: {2} | Before: {3} | After: {4} | Saved: {5}" -f $Path, $block.BlockNumber, $pictureKind, (Format-Bytes $beforeSize), (Format-Bytes $afterSize), (Format-Bytes $pictureSaved))
        Write-RunLog -Level INFO -Message $blockSavedMessage
        Write-VerboseUi -Message $blockSavedMessage

        $pictureSpec = New-MetaflacPictureSpec -Block $block -FilePath $picturePath
        if ([string]::IsNullOrWhiteSpace($pictureSpec)) {
            Write-RunLog -Level WARN -Message ("Album art optimization skipped due to unsupported picture metadata | File: {0} | Block: {1}" -f $Path, $block.BlockNumber)
            Safe-RemoveFile -Path $picturePath
            Safe-RemoveFile -Path $jpegOutputPath
            continue
        }

        $stagedPictures.Add([PSCustomObject]@{
                BlockNumber = $block.BlockNumber
                FilePath    = $picturePath
                SavedBytes  = $pictureSaved
                Spec        = $pictureSpec
            }) | Out-Null
    }

    $candidateSavingsMeasure = ($stagedPictures | Measure-Object -Property SavedBytes -Sum)
    $candidateSavedBytes = 0L
    if ($null -ne $candidateSavingsMeasure -and $candidateSavingsMeasure.PSObject.Properties.Name -contains 'Sum' -and $null -ne $candidateSavingsMeasure.Sum) {
        $candidateSavedBytes = [long]$candidateSavingsMeasure.Sum
    }
    $result.RawSavedBytes = $candidateSavedBytes
    $result.CandidateSavedBytes = $candidateSavedBytes
    if ($stagedPictures.Count -eq 0) {
        Write-RunLog -Level INFO -Message ("Metadata cleanup proceeding with padding-only pass | File: {0} | Reason: No smaller image payload was found" -f $Path)
    }

    $stagingMessage = ("Metadata cleanup staging | File: {0} | Blocks: {1} | Candidate Image Savings: {2}" -f $Path, $stagedPictures.Count, (Format-Bytes $candidateSavedBytes))
    Write-RunLog -Level INFO -Message $stagingMessage
    Write-VerboseUi -Message $stagingMessage

    $workingPath = "{0}.arttmp" -f $Path
    Safe-RemoveFile -Path $workingPath

    try {
        Copy-Item -LiteralPath $Path -Destination $workingPath -Force
    }
    catch {
        $result.Summary = ("MetadataCleanup skipped (stage copy failed; raw image {0})" -f (Format-Bytes $candidateSavedBytes))
        Write-RunLog -Level WARN -Message ("Album art optimization skipped; could not stage FLAC metadata rewrite | File: {0} | Detail: {1}" -f $Path, $_.Exception.Message)
        foreach ($staged in $stagedPictures) {
            Safe-RemoveFile -Path $staged.FilePath
        }
        return $result
    }

    $applyError = $null
    foreach ($staged in @($stagedPictures | Sort-Object -Property BlockNumber -Descending)) {
        $insertAfter = [Math]::Max(0, ([int]$staged.BlockNumber - 1))
        $removeExitCode = 0
        try {
            $removeOutput = @(& $MetaflacExePath --preserve-modtime --dont-use-padding "--block-number=$($staged.BlockNumber)" --remove -- $workingPath 2>&1)
            $removeExitCode = $LASTEXITCODE
        }
        catch {
            $applyError = "remove block $($staged.BlockNumber): $($_.Exception.Message)"
            break
        }

        if ($removeExitCode -ne 0) {
            $applyError = "remove block $($staged.BlockNumber): $(Format-ErrSnippet -Text ([string]::Join(' ', @($removeOutput | ForEach-Object { [string]$_ }))))"
            break
        }

        if ($removeOutput.Count -gt 0) {
            Write-VerboseUi -Message ("metaflac remove picture | File: {0} | Block: {1} | Output: {2}" -f $Path, $staged.BlockNumber, (Format-ErrSnippet -Text ([string]::Join(' ', @($removeOutput | ForEach-Object { [string]$_ }))) -MaxLength 250))
        }

        $importExitCode = 0
        try {
            $importOutput = @(& $MetaflacExePath --preserve-modtime --dont-use-padding "--block-number=$insertAfter" "--import-picture-from=$($staged.Spec)" -- $workingPath 2>&1)
            $importExitCode = $LASTEXITCODE
        }
        catch {
            $applyError = "import block $($staged.BlockNumber): $($_.Exception.Message)"
            break
        }

        if ($importExitCode -ne 0) {
            $applyError = "import block $($staged.BlockNumber): $(Format-ErrSnippet -Text ([string]::Join(' ', @($importOutput | ForEach-Object { [string]$_ }))))"
            break
        }

        if ($importOutput.Count -gt 0) {
            Write-VerboseUi -Message ("metaflac import picture | File: {0} | Block: {1} | InsertAfter: {2} | Output: {3}" -f $Path, $staged.BlockNumber, $insertAfter, (Format-ErrSnippet -Text ([string]::Join(' ', @($importOutput | ForEach-Object { [string]$_ }))) -MaxLength 250))
        }
    }

    if (-not [string]::IsNullOrWhiteSpace($applyError)) {
        $result.Summary = ("MetadataCleanup skipped (rewrite failed; raw image {0})" -f (Format-Bytes $candidateSavedBytes))
        Write-RunLog -Level WARN -Message ("Album art optimization skipped; could not rewrite picture blocks | File: {0} | Detail: {1}" -f $Path, $applyError)
        Safe-RemoveFile -Path $workingPath
        foreach ($staged in $stagedPictures) {
            Safe-RemoveFile -Path $staged.FilePath
        }
        return $result
    }

    $paddingOutput = @()
    $paddingWarning = $null
    $paddingExitCode = 0
    try {
        $paddingOutput = @(& $MetaflacExePath --dont-use-padding --remove --block-type=PADDING -- $workingPath 2>&1)
        $paddingExitCode = $LASTEXITCODE
    }
    catch {
        $paddingWarning = $_.Exception.Message
    }

    if ([string]::IsNullOrWhiteSpace($paddingWarning) -and $paddingExitCode -ne 0) {
        $paddingWarning = Format-ErrSnippet -Text ([string]::Join(' ', @($paddingOutput | ForEach-Object { [string]$_ })))
    }

    if (-not [string]::IsNullOrWhiteSpace($paddingWarning)) {
        Write-RunLog -Level WARN -Message ("Album art padding cleanup skipped after rewrite | File: {0} | Detail: {1}" -f $Path, $paddingWarning)
        Write-VerboseUi -Message ("metaflac strip padding skipped | File: {0} | Detail: {1}" -f $Path, (Format-ErrSnippet -Text $paddingWarning -MaxLength 250))
    }

    if ($paddingOutput.Count -gt 0) {
        Write-VerboseUi -Message ("metaflac strip padding | File: {0} | Output: {1}" -f $Path, (Format-ErrSnippet -Text ([string]::Join(' ', @($paddingOutput | ForEach-Object { [string]$_ }))) -MaxLength 250))
    }

    $originalSize = (Get-Item -LiteralPath $Path -Force).Length
    $optimizedSize = (Get-Item -LiteralPath $workingPath -Force).Length
    $netSaved = $originalSize - $optimizedSize

    $rewriteResultMessage = ("Metadata cleanup result | File: {0} | FLAC Before: {1} | FLAC After: {2} | Net Saved: {3} | Candidate Image Savings: {4}" -f $Path, (Format-Bytes $originalSize), (Format-Bytes $optimizedSize), (Format-Bytes $netSaved), (Format-Bytes $candidateSavedBytes))
    Write-RunLog -Level INFO -Message $rewriteResultMessage
    Write-VerboseUi -Message $rewriteResultMessage

    if ($netSaved -le 0) {
        if ($stagedPictures.Count -gt 0) {
            $result.Summary = ("MetadataCleanup {0} net ({1} raw image, discarded)" -f (Format-Bytes $netSaved), (Format-Bytes $candidateSavedBytes))
        }
        else {
            $result.Summary = ("MetadataCleanup {0} net (padding-only, discarded)" -f (Format-Bytes $netSaved))
        }
        Write-RunLog -Level INFO -Message ("Metadata cleanup discarded | File: {0} | Reason: Rewritten FLAC did not shrink after metadata rewrite" -f $Path)
        Safe-RemoveFile -Path $workingPath
        foreach ($staged in $stagedPictures) {
            Safe-RemoveFile -Path $staged.FilePath
        }
        return $result
    }

    try {
        Move-Item -LiteralPath $workingPath -Destination $Path -Force
    }
    catch {
        $result.Summary = ("MetadataCleanup skipped (swap failed; raw image {0})" -f (Format-Bytes $candidateSavedBytes))
        Write-RunLog -Level WARN -Message ("Album art optimization skipped; could not swap optimized temp FLAC | File: {0} | Detail: {1}" -f $Path, $_.Exception.Message)
        Safe-RemoveFile -Path $workingPath
        foreach ($staged in $stagedPictures) {
            Safe-RemoveFile -Path $staged.FilePath
        }
        return $result
    }

    foreach ($staged in $stagedPictures) {
        Safe-RemoveFile -Path $staged.FilePath
    }

    $result.Changed = $true
    $result.SavedBytes = [long]$netSaved
    $result.BlocksOptimized = $stagedPictures.Count
    if ($stagedPictures.Count -gt 0) {
        $result.Summary = ("MetadataCleanup {0} net ({1} raw image, {2} block{3})" -f (Format-Bytes $netSaved), (Format-Bytes $candidateSavedBytes), $stagedPictures.Count, $(if ($stagedPictures.Count -eq 1) { '' } else { 's' }))
    }
    else {
        $result.Summary = ("MetadataCleanup {0} net (padding-only)" -f (Format-Bytes $netSaved))
    }
    return $result
}

#endregion Album Art Optimization

#region Progress and Hash Display

function Read-FlacProgress {
    param(
        [Parameter(Mandatory)][string]$StdErrPath,
        [hashtable]$Cache
    )

    $default = @{ Pct = 0; Ratio = 'N/A' }
    if (-not (Test-Path -LiteralPath $StdErrPath)) { return $default }

    $cacheKey = $StdErrPath.ToLowerInvariant()
    $entry = $null
    if ($null -ne $Cache -and $Cache.ContainsKey($cacheKey)) { $entry = $Cache[$cacheKey] }
    $nowUtc = [DateTime]::UtcNow

    if ($null -ne $entry) {
        $hasNextRead = $entry.PSObject.Properties.Name -contains 'NextReadUtc'
        if ($hasNextRead -and ($entry.NextReadUtc -gt $nowUtc)) {
            return @{ Pct = $entry.Pct; Ratio = $entry.Ratio }
        }
    }

    try {
        $fi = Get-Item -LiteralPath $StdErrPath -ErrorAction Stop
        $length = [long]$fi.Length

        if ($null -ne $entry -and $entry.Length -eq $length) {
            if ($null -ne $Cache) {
                $Cache[$cacheKey] = [PSCustomObject]@{
                    Length      = $length
                    Pct         = $entry.Pct
                    Ratio       = $entry.Ratio
                    NextReadUtc = $nowUtc.AddMilliseconds(700)
                }
            }
            return @{ Pct = $entry.Pct; Ratio = $entry.Ratio }
        }

        $tailSize = 24576L
        $startOffset = [Math]::Max(0L, $length - $tailSize)
        $raw = ''

        $fs = [System.IO.FileStream]::new($StdErrPath, [System.IO.FileMode]::Open, [System.IO.FileAccess]::Read, [System.IO.FileShare]::ReadWrite)
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
                Length      = $length
                Pct         = $pct
                Ratio       = $ratio
                NextReadUtc = $nowUtc.AddMilliseconds(700)
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

function Format-HashForLog {
    param(
        [AllowNull()][string]$Hash,
        [Parameter(Mandatory)][string]$NullHash
    )

    if ([string]::IsNullOrWhiteSpace($Hash)) { return 'N/A' }
    if ($Hash -eq $NullHash) { return 'NULL-EMBEDDED' }
    return $Hash.ToLowerInvariant()
}

function Truncate-Text {
    param(
        [AllowNull()][string]$Text,
        [Parameter(Mandatory)][int]$Width
    )

    if ($Width -lt 1) { return '' }
    return (Fit-DisplayText -Text $Text -Width $Width -UseEllipsis).TrimEnd()
}

function Get-CodepointDisplayWidth {
    param([Parameter(Mandatory)][int]$CodePoint)

    if ($CodePoint -le 0) { return 0 }

    # Control characters and combining/formatting marks are zero-width in terminal cells.
    if (($CodePoint -lt 0x20) -or ($CodePoint -ge 0x7F -and $CodePoint -lt 0xA0)) { return 0 }
    $cpText = [char]::ConvertFromUtf32($CodePoint)
    $category = [char]::GetUnicodeCategory($cpText, 0)
    if ($category -eq [System.Globalization.UnicodeCategory]::NonSpacingMark -or
        $category -eq [System.Globalization.UnicodeCategory]::SpacingCombiningMark -or
        $category -eq [System.Globalization.UnicodeCategory]::EnclosingMark -or
        $category -eq [System.Globalization.UnicodeCategory]::Format -or
        $category -eq [System.Globalization.UnicodeCategory]::Control) {
        return 0
    }

    # East Asian wide/full-width ranges + emoji ranges.
    if (($CodePoint -ge 0x1100 -and $CodePoint -le 0x115F) -or
        ($CodePoint -ge 0x2329 -and $CodePoint -le 0x232A) -or
        ($CodePoint -ge 0x2E80 -and $CodePoint -le 0xA4CF) -or
        ($CodePoint -ge 0xAC00 -and $CodePoint -le 0xD7A3) -or
        ($CodePoint -ge 0xF900 -and $CodePoint -le 0xFAFF) -or
        ($CodePoint -ge 0xFE10 -and $CodePoint -le 0xFE19) -or
        ($CodePoint -ge 0xFE30 -and $CodePoint -le 0xFE6F) -or
        ($CodePoint -ge 0xFF00 -and $CodePoint -le 0xFF60) -or
        ($CodePoint -ge 0xFFE0 -and $CodePoint -le 0xFFE6) -or
        ($CodePoint -ge 0x1F300 -and $CodePoint -le 0x1FAFF) -or
        ($CodePoint -ge 0x20000 -and $CodePoint -le 0x2FFFD) -or
        ($CodePoint -ge 0x30000 -and $CodePoint -le 0x3FFFD)) {
        return 2
    }

    return 1
}

function Get-DisplayTextWidth {
    param([AllowNull()][string]$Text)

    if ([string]::IsNullOrEmpty($Text)) { return 0 }
    $width = 0
    $enumerator = [System.Globalization.StringInfo]::GetTextElementEnumerator($Text)
    while ($enumerator.MoveNext()) {
        $element = [string]$enumerator.Current
        $cp = [char]::ConvertToUtf32($element, 0)
        $width += Get-CodepointDisplayWidth -CodePoint $cp
    }
    return $width
}

function Fit-DisplayText {
    param(
        [AllowNull()][string]$Text,
        [Parameter(Mandatory)][int]$Width,
        [switch]$UseEllipsis
    )

    if ($Width -lt 1) { return '' }
    if ($null -eq $Text) { $Text = '' }
    elseif ($Text.Length -gt 0) {
        # Keep every rendered UI row physically single-line. Control characters are
        # zero-width for layout, but if emitted verbatim they still move the cursor
        # and can force the whole frame to scroll.
        $Text = [regex]::Replace($Text, '[\x00-\x1F\x7F-\x9F]+', ' ')
    }

    $currentWidth = Get-DisplayTextWidth -Text $Text
    if ($currentWidth -le $Width) {
        return ($Text + (' ' * ($Width - $currentWidth)))
    }

    $ellipsis = if ($UseEllipsis -and $Width -gt 3) { '...' } else { '' }
    $targetWidth = $Width - $ellipsis.Length
    if ($targetWidth -lt 0) { $targetWidth = 0 }

    $sb = [System.Text.StringBuilder]::new()
    $used = 0
    $enumerator = [System.Globalization.StringInfo]::GetTextElementEnumerator($Text)
    while ($enumerator.MoveNext()) {
        $element = [string]$enumerator.Current
        $cp = [char]::ConvertToUtf32($element, 0)
        $w = Get-CodepointDisplayWidth -CodePoint $cp
        if (($used + $w) -gt $targetWidth) { break }
        [void]$sb.Append($element)
        $used += $w
    }

    if ($ellipsis.Length -gt 0) {
        [void]$sb.Append($ellipsis)
        $used += $ellipsis.Length
    }

    if ($used -lt $Width) {
        [void]$sb.Append((' ' * ($Width - $used)))
    }

    return $sb.ToString()
}

#endregion Progress and Hash Display

#region UI Colors

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

    # Compression gradient: cool colors for low wins, warm/purple for major wins.
    if ($value -ge 20.0) { return 'Magenta' }        # Exceptional compression
    if ($value -ge 15.0) { return 'DarkMagenta' }    # Very high compression
    if ($value -ge 10.0) { return 'Yellow' }         # High compression
    if ($value -ge 6.0) { return 'DarkYellow' }      # Strong compression
    if ($value -ge 3.0) { return 'Cyan' }            # Moderate compression
    if ($value -gt 1.0) { return 'DarkCyan' }        # Light compression
    if ($value -gt 0.0) { return 'Blue' }            # Minimal compression
    if ($value -eq 0.0) { return 'Gray' }            # No change
    return 'DarkRed'                                 # Expansion / worse result
}

function Get-StatusColor {
    param([AllowNull()][string]$Status)
    switch ($Status) {
        'OK' { return 'Cyan' }
        'RETRY' { return 'Yellow' }
        'FAIL' { return 'Red' }
        'WAIT' { return 'DarkGray' }
        default { return 'White' }
    }
}

function Get-VerificationColor {
    param([AllowNull()][string]$Verification)

    if ([string]::IsNullOrWhiteSpace($Verification)) { return 'DarkGray' }
    if ($Verification -eq 'MATCH|NEW') { return 'Green' }
    if ($Verification -eq 'MATCH') { return 'Cyan' }
    if ($Verification -eq 'MISMATCH') { return 'Red' }
    return 'Yellow'
}

function Get-WorkerStateColor {
    param(
        [AllowNull()][string]$State,
        [AllowNull()][string]$Stage,
        [int]$Pct = 0
    )

    switch ($State) {
        'ART' { return 'Magenta' }
        'HASHIN' { return 'Yellow' }
        'HASHOUT' { return 'DarkYellow' }
        'HASHING' { return 'DarkYellow' }
        'FINAL' { return 'Cyan' }
        'ENCODE' {
            if ($Pct -ge 100) { return 'Green' }
            return 'White'
        }
        'IDLE' { return 'DarkGray' }
        default {
            if ($Stage -eq 'HASHING') { return 'DarkYellow' }
            if ($Stage -eq 'ARTWORK') { return 'Magenta' }
            return 'White'
        }
    }
}

function Format-VerificationText {
    param([AllowNull()][string]$Verification)

    if ([string]::IsNullOrWhiteSpace($Verification)) { return 'N/A' }
    return $Verification
}

#endregion UI Colors

#region UI Rendering

function Render-InteractiveUi {
    param(
        [Parameter(Mandatory)][string]$AlbumName,
        [Parameter(Mandatory)][DateTime]$RunStartedUtc,
        [Parameter(Mandatory)][int]$Processed,
        [Parameter(Mandatory)][int]$TotalFiles,
        [Parameter(Mandatory)][int]$Failed,
        [Parameter(Mandatory)][long]$TotalSavedBytes,
        [Parameter(Mandatory)][long]$TotalMetadataSavedBytes,
        [Parameter(Mandatory)][long]$TotalPaddingTrimSavedBytes,
        [Parameter(Mandatory)][int]$PaddingTrimFiles,
        [Parameter(Mandatory)][long]$TotalArtworkSavedBytes,
        [Parameter(Mandatory)][long]$TotalArtworkRawSavedBytes,
        [Parameter(Mandatory)][int]$ArtworkOptimizedFiles,
        [Parameter(Mandatory)][int]$ArtworkOptimizedBlocks,
        [Parameter(Mandatory)][int]$QueueCount,
        [Parameter(Mandatory)][int]$MaxAttemptsPerFile,
        [Parameter(Mandatory)][object[]]$Workers,
        [AllowNull()][System.Collections.Generic.List[object]]$RecentEvents,
        [AllowNull()][object[]]$TopCompression,
        [Parameter(Mandatory)][hashtable]$ProgressCache,
        [AllowNull()][System.Collections.Generic.List[string]]$VerboseMessages,
        [string]$PngToolStatus = 'OFF',
        [string]$JpegToolStatus = 'OFF',
        [int]$PreviousRows = 0,
        [switch]$ForceClear,
        [string]$Banner = ''
    )

    $frameWidth = 120
    $frameHeight = 30
    try {
        # Keep one spare column and row so Windows Terminal never auto-wraps at the
        # last cell and turns a repaint into a scroll.
        $frameWidth = [Math]::Max(40, [Console]::WindowWidth - 1)
        $frameHeight = [Math]::Max(12, [Console]::WindowHeight - 1)
    }
    catch { }

    $width = [Math]::Max(10, $frameWidth - 2)
    # Leave one extra guard row inside the viewport. In Windows Terminal, a single
    # unexpectedly wrapped row in the bottom section can still trigger a scroll if
    # the frame fully consumes the drawable height.
    $maxRows = [Math]::Max(2, $frameHeight - 3)

    if ($null -eq $RecentEvents) {
        $RecentEvents = [System.Collections.Generic.List[object]]::new()
    }

    $activeWorkers = @($Workers | Where-Object { $_.Job -ne $null })
    $activeCount = $activeWorkers.Count

    $rowsWritten = 0
    $renderLines = [System.Collections.Generic.List[object]]::new()

    function Get-AnsiForeground {
        param([ConsoleColor]$Color)

        switch ($Color) {
            ([ConsoleColor]::Black) { return '30' }
            ([ConsoleColor]::DarkRed) { return '31' }
            ([ConsoleColor]::DarkGreen) { return '32' }
            ([ConsoleColor]::DarkYellow) { return '33' }
            ([ConsoleColor]::DarkBlue) { return '34' }
            ([ConsoleColor]::DarkMagenta) { return '35' }
            ([ConsoleColor]::DarkCyan) { return '36' }
            ([ConsoleColor]::Gray) { return '37' }
            ([ConsoleColor]::DarkGray) { return '90' }
            ([ConsoleColor]::Red) { return '91' }
            ([ConsoleColor]::Green) { return '92' }
            ([ConsoleColor]::Yellow) { return '93' }
            ([ConsoleColor]::Blue) { return '94' }
            ([ConsoleColor]::Magenta) { return '95' }
            ([ConsoleColor]::Cyan) { return '96' }
            ([ConsoleColor]::White) { return '97' }
            default { return '39' }
        }
    }

    function Write-UiBorder {
        $renderLines.Add([PSCustomObject]@{
                Text  = ('+' + ('-' * $width) + '+')
                Color = [ConsoleColor]::DarkGray
            }) | Out-Null
    }

    function Write-UiLine {
        param(
            [AllowNull()][string]$Text = '',
            [ConsoleColor]$Color = [ConsoleColor]::Gray
        )
        $content = Fit-DisplayText -Text $Text -Width $width -UseEllipsis
        $renderLines.Add([PSCustomObject]@{
                Text  = ('|' + $content + '|')
                Color = $Color
            }) | Out-Null
        $script:__uiRowsWritten++
    }

    function Write-UiSegmentLine {
        param([Parameter(Mandatory)][object[]]$Segments)

        $fullSegments = [System.Collections.Generic.List[object]]::new()
        $fullSegments.Add([PSCustomObject]@{
                Text  = '|'
                Color = [ConsoleColor]::DarkGray
            }) | Out-Null

        foreach ($segment in $Segments) {
            $fullSegments.Add($segment) | Out-Null
        }

        $fullSegments.Add([PSCustomObject]@{
                Text  = '|'
                Color = [ConsoleColor]::DarkGray
            }) | Out-Null

        $renderLines.Add([PSCustomObject]@{
                Segments = @($fullSegments)
            }) | Out-Null
        $script:__uiRowsWritten++
    }

    function Write-UiTableRow {
        param([Parameter(Mandatory)][object[]]$Columns)
        $segments = [System.Collections.Generic.List[object]]::new()
        foreach ($col in $Columns) {
            if ($segments.Count -gt 0) {
                $segments.Add([PSCustomObject]@{ Text = '|'; Color = [ConsoleColor]::DarkGray }) | Out-Null
            }
            $fitted = Fit-DisplayText -Text ([string]$col.Text) -Width $col.Width -UseEllipsis:([bool]$col.Ellipsis)
            $segments.Add([PSCustomObject]@{ Text = $fitted; Color = [ConsoleColor]$col.Color }) | Out-Null
        }
        Write-UiSegmentLine -Segments @($segments)
    }

    function Format-UiTableHeader {
        param([Parameter(Mandatory)][object[]]$Columns)
        return ($Columns | ForEach-Object { ([string]$_.Label).PadRight($_.Width) }) -join '|'
    }

    function Format-HeaderMetricRow {
        param([Parameter(Mandatory)][string[]]$Metrics)

        $metricList = @($Metrics | Where-Object { -not [string]::IsNullOrWhiteSpace($_) })
        if ($metricList.Count -eq 0) { return '' }

        $separatorWidth = 3
        $availableWidth = [Math]::Max(1, ($width - (($metricList.Count - 1) * $separatorWidth)))
        $columnWidth = [Math]::Max(24, [int][Math]::Floor($availableWidth / $metricList.Count))
        $columns = foreach ($metric in $metricList) {
            Fit-DisplayText -Text $metric -Width $columnWidth
        }

        return [string]::Join(' | ', $columns)
    }

    $script:__uiRowsWritten = 0
    Write-UiBorder

    $elapsed = [DateTime]::UtcNow - $RunStartedUtc
    if ($elapsed.Ticks -lt 0) { $elapsed = [TimeSpan]::Zero }
    $elapsedText = Format-Elapsed -Elapsed $elapsed
    $metadataNetSavedBytes = $TotalMetadataSavedBytes
    $audioDeltaBytes = $TotalSavedBytes - $metadataNetSavedBytes
    if ($TotalSavedBytes -le 0 -and $audioDeltaBytes -lt 0) {
        $audioDeltaBytes = 0L
    }
    $successfulCount = [Math]::Max(0, ($Processed - $Failed))

    Write-UiLine -Text ("Exact Flac Cruncher | {0}" -f $AlbumName) -Color Cyan
    Write-UiLine -Text ("Progress {0}/{1} | Failed {2} | Elapsed {3} | Queue {4} | Active {5}/{6}" -f $Processed, $TotalFiles, $Failed, $elapsedText, $QueueCount, $activeCount, $Workers.Count) -Color White
    Write-UiLine -Text (Format-HeaderMetricRow -Metrics @(
            (Format-LabelValue -Label 'TOTAL SAVED' -Value (Format-Bytes $TotalSavedBytes) -LabelWidth 15),
            (Format-LabelValue -Label 'AUDIO DELTA' -Value (Format-Bytes $audioDeltaBytes -Signed) -LabelWidth 15),
            (Format-LabelValue -Label 'METADATA NET' -Value (Format-Bytes $metadataNetSavedBytes) -LabelWidth 15)
        )) -Color DarkCyan
    Write-UiLine -Text (Format-HeaderMetricRow -Metrics @(
            (Format-LabelValue -Label 'PADDING TRIM' -Value (Format-Bytes $TotalPaddingTrimSavedBytes) -LabelWidth 15),
            (Format-LabelValue -Label 'ARTWORK NET' -Value (Format-Bytes $TotalArtworkSavedBytes) -LabelWidth 15),
            (Format-LabelValue -Label 'ARTWORK RAW' -Value (Format-Bytes $TotalArtworkRawSavedBytes) -LabelWidth 15)
        )) -Color DarkGreen
    Write-UiLine -Text (Format-HeaderMetricRow -Metrics @(
            (Format-LabelValue -Label 'PADDING FILES' -Value (Format-HeaderCount -Value $PaddingTrimFiles) -LabelWidth 15),
            (Format-LabelValue -Label 'ARTWORK FILES' -Value (Format-HeaderCount -Value $ArtworkOptimizedFiles) -LabelWidth 15),
            (Format-LabelValue -Label 'ARTWORK BLOCKS' -Value (Format-HeaderCount -Value $ArtworkOptimizedBlocks) -LabelWidth 15)
        )) -Color DarkGreen
    Write-UiLine -Text ("ART TOOLS: PNG={0} | JPEG={1}" -f $PngToolStatus, $JpegToolStatus) -Color DarkGray
    if ([string]::IsNullOrWhiteSpace($Banner)) {
        Write-UiLine -Text "Ctrl+C: Cancel safely. Will clean up temp files and restore original files on exit." -Color DarkGray
    }
    else {
        Write-UiLine -Text $Banner -Color Yellow
    }
    Write-UiLine -Text ("-" * $width) -Color DarkGray
    Write-UiLine -Text "Top 3 Compression (live)" -Color Magenta
    if ($null -eq $TopCompression -or $TopCompression.Count -eq 0) {
        if ($successfulCount -gt 0) {
            Write-UiLine -Text "  (No net-positive file reductions yet)" -Color DarkGray
        }
        else {
            Write-UiLine -Text "  (No successful file conversions yet)" -Color DarkGray
        }
        for ($i = 1; $i -lt 3; $i++) {
            Write-UiLine -Text '' -Color DarkGray
        }
    }
    else {
        $rank = 0
        foreach ($entry in $TopCompression) {
            $rank++
            $entryColor = Get-CompressionColor -CompressionPct ("{0:N2}%" -f $entry.SavedPct)
            $line = Format-TopCompressionLine -Rank $rank -Entry $entry -LeafName
            Write-UiLine -Text $line -Color $entryColor
            if ($rank -ge 3) { break }
        }
        for ($i = $rank; $i -lt 3; $i++) {
            Write-UiLine -Text '' -Color DarkGray
        }
    }
    Write-UiLine -Text ("-" * $width) -Color DarkGray

    $showVerboseTrace = ($null -ne $VerboseMessages -and $VerboseMessages.Count -gt 0)
    $workerChromeRows = 3
    $eventChromeRows = 2
    $verboseChromeRows = if ($showVerboseTrace) { 2 } else { 0 }
    $desiredTraceRows = if ($showVerboseTrace) { 4 } else { 0 }
    $bodyRowsRemaining = [Math]::Max(0, $maxRows - $script:__uiRowsWritten)
    $workerRows = 0
    if ($bodyRowsRemaining -gt $workerChromeRows) {
        $workerRows = [Math]::Min($Workers.Count, $bodyRowsRemaining - $workerChromeRows)
    }
    $rowsAfterWorkers = [Math]::Max(0, $bodyRowsRemaining - ($workerChromeRows + $workerRows))
    $traceRows = 0
    $showVerboseSection = $false
    if ($showVerboseTrace -and $rowsAfterWorkers -ge ($eventChromeRows + 1 + $verboseChromeRows + 1)) {
        $maxTraceRows = $rowsAfterWorkers - ($eventChromeRows + 1 + $verboseChromeRows)
        if ($maxTraceRows -gt 0) {
            $showVerboseSection = $true
            $traceRows = [Math]::Min($desiredTraceRows, $maxTraceRows)
        }
    }
    $rowsForEvents = $rowsAfterWorkers - $(if ($showVerboseSection) { $verboseChromeRows + $traceRows } else { 0 })
    $showEventSection = ($rowsForEvents -ge ($eventChromeRows + 1))
    $eventRows = if ($showEventSection) { [Math]::Max(0, $rowsForEvents - $eventChromeRows) } else { 0 }

    $wCore = 6; $wState = 7; $wTry = 5; $wPct = 5; $wRatio = 8; $wBar = 22
    $workerFixed = $wCore + $wState + $wTry + $wPct + $wRatio + $wBar
    $workerSep = 6
    $wFile = [Math]::Max(18, $width - ($workerFixed + $workerSep))

    $workerColDefs = @(
        @{ Label = 'Core'; Width = $wCore }, @{ Label = 'State'; Width = $wState },
        @{ Label = 'Try'; Width = $wTry }, @{ Label = 'Pct'; Width = $wPct },
        @{ Label = 'Ratio'; Width = $wRatio }, @{ Label = 'Progress'; Width = $wBar },
        @{ Label = 'File'; Width = $wFile }
    )
    Write-UiLine -Text "Workers" -Color Cyan
    Write-UiLine -Text (Format-UiTableHeader -Columns $workerColDefs) -Color DarkCyan

    for ($i = 0; $i -lt $workerRows; $i++) {
        if ($i -ge $Workers.Count) {
            Write-UiLine -Text '' -Color DarkGray
            continue
        }

        $w = $Workers[$i]
        $coreLabel = "C{0:D2}" -f $w.Id

        if ($null -eq $w.Job) {
            Write-UiTableRow -Columns @(
                @{ Text = $coreLabel; Width = $wCore; Color = 'DarkGray' },
                @{ Text = 'IDLE'; Width = $wState; Color = 'DarkGray' },
                @{ Text = '-'; Width = $wTry; Color = 'DarkGray' },
                @{ Text = '-'; Width = $wPct; Color = 'DarkGray' },
                @{ Text = '-'; Width = $wRatio; Color = 'DarkGray' },
                @{ Text = ''; Width = $wBar; Color = 'DarkGray' },
                @{ Text = ''; Width = $wFile; Color = 'DarkGray' }
            )
            continue
        }

        $stage = [string]$w.Job.Stage
        if ([string]::IsNullOrWhiteSpace($stage)) { $stage = 'CONVERTING' }

        $pct = 0; $ratio = '-'; $stateText = 'ENCODE'; $fileSuffix = ''
        switch ($stage) {
            'ARTWORK'    { $stateText = 'ART';     $fileSuffix = ' [artwork]' }
            'HASHING'    {
                $stateText = 'HASHING'
                $phase = [string]$w.Job.HashPhase
                if ($phase -eq 'SOURCE')    { $stateText = 'HASHIN';  $fileSuffix = ' [hash:in]' }
                elseif ($phase -eq 'OUTPUT') { $stateText = 'HASHOUT'; $fileSuffix = ' [hash:out]' }
                else { $fileSuffix = ' [hash]' }
            }
            'FINALIZING' { $stateText = 'FINAL';   $fileSuffix = ' [finalize]' }
            default {
                $stateText = 'ENCODE'
                $p = Read-FlacProgress -StdErrPath $w.Job.ErrLog -Cache $ProgressCache
                $pct = [int]$p.Pct; $ratio = [string]$p.Ratio
                if ($ratio -eq 'N/A') { $ratio = '-' }
            }
        }

        $attempt = "{0}/{1}" -f $w.Job.Attempt, $MaxAttemptsPerFile
        $barLen = [Math]::Max(8, $wBar - 2)
        $fill = if ($stage -eq 'HASHING' -or $stage -eq 'ARTWORK') { [int](($script:__uiRowsWritten + $i) % ($barLen + 1)) } else { [int][Math]::Floor(($pct / 100.0) * $barLen) }
        if ($fill -lt 0) { $fill = 0 }
        if ($fill -gt $barLen) { $fill = $barLen }
        $bar = '[' + ('#' * $fill).PadRight($barLen, '.') + ']'
        $stateColor = Get-WorkerStateColor -State $stateText -Stage $stage -Pct $pct

        Write-UiTableRow -Columns @(
            @{ Text = $coreLabel; Width = $wCore; Color = 'DarkGray' },
            @{ Text = $stateText; Width = $wState; Color = $stateColor },
            @{ Text = $attempt; Width = $wTry; Color = 'Gray' },
            @{ Text = ("{0,3}%" -f $pct); Width = $wPct; Color = $stateColor },
            @{ Text = $ratio; Width = $wRatio; Color = 'Gray' },
            @{ Text = $bar; Width = $wBar; Color = $stateColor },
            @{ Text = ($w.Job.Name + $fileSuffix); Width = $wFile; Color = 'Gray'; Ellipsis = $true }
        )
    }

    if ($Workers.Count -gt $workerRows) {
        Write-UiLine -Text ("... {0} more workers not shown (expand to show more; resize taller to view)." -f ($Workers.Count - $workerRows)) -Color DarkGray
    }
    else {
        Write-UiLine -Text ("-" * $width) -Color DarkGray
    }

    if ($showVerboseSection) {
        Write-UiLine -Text "Verbose Trace (-Verbose)" -Color Yellow
        $traceCount = [Math]::Min($traceRows, $VerboseMessages.Count)
        for ($i = 0; $i -lt $traceCount; $i++) {
            Write-UiLine -Text ([string]$VerboseMessages[$i]) -Color DarkYellow
        }
        for ($i = $traceCount; $i -lt $traceRows; $i++) {
            Write-UiLine -Text '' -Color DarkGray
        }
        Write-UiLine -Text ("-" * $width) -Color DarkGray
    }

    if ($showEventSection) {
        # Core columns always shown: Time, Stat, Comp%, Verify, File
        # File absorbs remaining width. 4 fixed cols + File = 5 cols = 4 separators.
        $eTime = 8; $eStat = 5; $eComp = 8; $eHash = 7
        $coreFixed = $eTime + $eStat + $eComp + $eHash  # 28
        $coreSeps = 4                                     # 4 separators between 5 cols (including File)

        # Optional columns added when width permits (priority order)
        $eSaved = 9; $eTry = 7
        $minFile = 10

        $remaining = $width - $coreFixed - $coreSeps
        $showSaved = ($remaining -ge ($eSaved + 1 + $minFile))
        if ($showSaved) { $remaining -= ($eSaved + 1) }
        $showAttempt = ($remaining -ge ($eTry + 1 + $minFile))
        if ($showAttempt) { $remaining -= ($eTry + 1) }
        $showDetail = ($remaining -ge (18 + 1 + 18))
        $eFile = 0; $eDetail = 0
        if ($showDetail) {
            $eFile = [int][Math]::Floor($remaining * 0.52)
            if ($eFile -lt 18) { $eFile = 18 }
            if ($eFile -gt ($remaining - 18)) { $eFile = $remaining - 18 }
            $eDetail = $remaining - $eFile - 1
        }
        else {
            $eFile = $remaining
        }

        # Build column definitions for header and rows
        $eventColDefs = [System.Collections.Generic.List[object]]::new()
        $eventColDefs.Add(@{ Label = 'Time';   Width = $eTime }) | Out-Null
        $eventColDefs.Add(@{ Label = 'Stat';   Width = $eStat }) | Out-Null
        if ($showAttempt) { $eventColDefs.Add(@{ Label = 'Attempt'; Width = $eTry }) | Out-Null }
        $eventColDefs.Add(@{ Label = 'Comp%';  Width = $eComp }) | Out-Null
        if ($showSaved) { $eventColDefs.Add(@{ Label = 'Saved';  Width = $eSaved }) | Out-Null }
        $eventColDefs.Add(@{ Label = 'Verify'; Width = $eHash }) | Out-Null
        $eventColDefs.Add(@{ Label = 'File';   Width = $eFile }) | Out-Null
        if ($showDetail) { $eventColDefs.Add(@{ Label = 'Detail'; Width = $eDetail }) | Out-Null }

        Write-UiLine -Text "Recent Results (latest first)" -Color Cyan
        Write-UiLine -Text (Format-UiTableHeader -Columns @($eventColDefs)) -Color DarkCyan

        $rowsToPrint = [Math]::Min($eventRows, $RecentEvents.Count)
        for ($i = 0; $i -lt $rowsToPrint; $i++) {
            $row = $RecentEvents[$i]
            $statusColor = Get-StatusColor -Status $row.Status
            $cmpColor = Get-CompressionColor -CompressionPct $row.CompressionPct
            $verificationDisplay = Format-VerificationText -Verification ([string]$row.Verification)
            $verificationColor = Get-VerificationColor -Verification $row.Verification

            $cols = [System.Collections.Generic.List[object]]::new()
            $cols.Add(@{ Text = $row.Time;           Width = $eTime; Color = 'DarkGray' }) | Out-Null
            $cols.Add(@{ Text = $row.Status;         Width = $eStat; Color = $statusColor }) | Out-Null
            if ($showAttempt) { $cols.Add(@{ Text = $row.Attempt; Width = $eTry; Color = 'Gray' }) | Out-Null }
            $cols.Add(@{ Text = $row.CompressionPct; Width = $eComp; Color = $cmpColor }) | Out-Null
            if ($showSaved) { $cols.Add(@{ Text = $row.Saved;    Width = $eSaved; Color = $cmpColor }) | Out-Null }
            $cols.Add(@{ Text = $verificationDisplay; Width = $eHash; Color = $verificationColor; Ellipsis = $true }) | Out-Null
            $cols.Add(@{ Text = $row.File;           Width = $eFile; Color = 'Gray'; Ellipsis = $true }) | Out-Null
            if ($showDetail) { $cols.Add(@{ Text = $row.Detail; Width = $eDetail; Color = 'DarkGray'; Ellipsis = $true }) | Out-Null }

            Write-UiTableRow -Columns @($cols)
        }

        for ($i = $rowsToPrint; $i -lt $eventRows; $i++) {
            Write-UiLine -Text '' -Color DarkGray
        }
    }

    $rowsWritten = $script:__uiRowsWritten
    for ($i = $rowsWritten; $i -lt $maxRows; $i++) {
        Write-UiLine -Text '' -Color DarkGray
    }

    Write-UiBorder

    $esc = [char]27
    $buffer = [System.Text.StringBuilder]::new()
    [void]$buffer.Append($esc).Append('[H')
    if ($ForceClear) {
        [void]$buffer.Append($esc).Append('[J')
    }
    for ($i = 0; $i -lt $renderLines.Count; $i++) {
        $line = $renderLines[$i]
        if (($line.PSObject.Properties.Name -contains 'Segments') -and $null -ne $line.Segments) {
            foreach ($segment in $line.Segments) {
                $ansiColor = Get-AnsiForeground -Color $segment.Color
                [void]$buffer.Append($esc).Append('[').Append($ansiColor).Append('m')
                [void]$buffer.Append([string]$segment.Text)
            }
            [void]$buffer.Append($esc).Append('[0m')
        }
        else {
            $ansiColor = Get-AnsiForeground -Color $line.Color
            [void]$buffer.Append($esc).Append('[').Append($ansiColor).Append('m')
            [void]$buffer.Append([string]$line.Text)
            [void]$buffer.Append($esc).Append('[0m')
        }
        if ($i -lt ($renderLines.Count - 1)) {
            [void]$buffer.Append("`r`n")
        }
    }

    [Console]::Write($buffer.ToString())
    return $maxRows
}

function Sync-ConsoleBufferToWindow {
    try {
        $windowWidth = [Console]::WindowWidth
        $windowHeight = [Console]::WindowHeight
        if ($windowWidth -lt 1 -or $windowHeight -lt 1) { return }

        if ([Console]::BufferWidth -lt $windowWidth) {
            [Console]::BufferWidth = $windowWidth
        }
        if ([Console]::BufferHeight -lt $windowHeight) {
            [Console]::BufferHeight = $windowHeight
        }

        if ([Console]::BufferHeight -ne $windowHeight) {
            [Console]::BufferHeight = $windowHeight
        }
        if ([Console]::BufferWidth -ne $windowWidth) {
            [Console]::BufferWidth = $windowWidth
        }
    }
    catch { }
}

#endregion UI Rendering

#region Event Tracking

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

    if ($List.Count -gt 250) { $List.RemoveRange(250, $List.Count - 250) }
}

function Add-FinalLogEvent {
    param(
        [Parameter(Mandatory)][object]$List,
        [Parameter(Mandatory)][ValidateSet('OK', 'RETRY', 'FAIL', 'CANCELED')][string]$EventType,
        [Parameter(Mandatory)][AllowEmptyString()][string]$File,
        [Parameter(Mandatory)][AllowEmptyString()][string]$FullPath,
        [Parameter(Mandatory)][string]$Attempt,
        [string]$Verification = 'N/A',
        [string]$EmbeddedHash = 'N/A',
        [string]$CalcPreHash = 'N/A',
        [string]$CalcPostHash = 'N/A',
        [string]$OrigBytes = 'N/A',
        [string]$NewBytes = 'N/A',
        [string]$SavedBytes = 'N/A',
        [string]$SavedPct = 'N/A',
        [string]$AudioSavedBytes = 'N/A',
        [string]$MetadataSummary = '',
        [string]$FailureReason = ''
    )

    if ($null -eq $List) { throw 'Add-FinalLogEvent: event list is null.' }
    if (-not ($List -is [System.Collections.Generic.List[object]])) {
        throw ("Add-FinalLogEvent: expected List[object], got {0}" -f $List.GetType().FullName)
    }

    $List.Add([PSCustomObject]@{
            Timestamp       = Get-Date
            File            = $File
            FullPath        = $FullPath
            Attempt         = $Attempt
            EventType       = $EventType
            Verification    = $Verification
            EmbeddedHash    = $EmbeddedHash
            CalcPreHash     = $CalcPreHash
            CalcPostHash    = $CalcPostHash
            OrigBytes       = $OrigBytes
            NewBytes        = $NewBytes
            SavedBytes      = $SavedBytes
            SavedPct        = $SavedPct
            AudioSavedBytes = $AudioSavedBytes
            MetadataSummary = $MetadataSummary
            FailureReason   = $FailureReason
        }) | Out-Null
}

#endregion Event Tracking

#region Reporting

function New-EfcStatusReportLines {
    param(
        [Parameter(Mandatory)][int]$Successful,
        [Parameter(Mandatory)][int]$Failed,
        [Parameter(Mandatory)][int]$Pending,
        [Parameter(Mandatory)][bool]$RunCanceled
    )

    $lines = [System.Collections.Generic.List[string]]::new()
    if ($Successful -gt 0) {
        $lines.Add((" {0} file(s) processed successfully" -f $Successful)) | Out-Null
    }
    if ($Failed -gt 0) {
        $lines.Add((" {0} file(s) failed" -f $Failed)) | Out-Null
    }
    if ($Pending -gt 0) {
        $lines.Add((" {0} file(s) pending" -f $Pending)) | Out-Null
    }
    if ($lines.Count -eq 0) {
        $lines.Add(' 0 file(s) processed') | Out-Null
    }

    $lines.Add('') | Out-Null
    if ($RunCanceled -and $Pending -gt 0) {
        $lines.Add('Processing canceled by user') | Out-Null
    }
    elseif ($Failed -gt 0) {
        $lines.Add('Some files could not be verified') | Out-Null
    }
    elseif ($Successful -gt 0 -and $Pending -eq 0) {
        $lines.Add('All files processed successfully') | Out-Null
    }
    else {
        $lines.Add('Processing complete') | Out-Null
    }

    $lines.Add('') | Out-Null
    if ($Failed -gt 0 -or ($RunCanceled -and $Pending -gt 0)) {
        $lines.Add('There were errors') | Out-Null
    }
    else {
        $lines.Add('No errors occurred') | Out-Null
    }
    $lines.Add('') | Out-Null
    $lines.Add('End of status report') | Out-Null

    return $lines
}

function New-EfcFinalLogText {
    param(
        [Parameter(Mandatory)][string]$AlbumName,
        [Parameter(Mandatory)][string]$RootFolder,
        [Parameter(Mandatory)][DateTime]$RunStartedLocal,
        [Parameter(Mandatory)][DateTime]$FinishedLocal,
        [Parameter(Mandatory)][int]$MaxWorkers,
        [Parameter(Mandatory)][int]$MaxAttemptsPerFile,
        [Parameter(Mandatory)][int]$TotalFiles,
        [Parameter(Mandatory)][int]$Processed,
        [Parameter(Mandatory)][int]$Successful,
        [Parameter(Mandatory)][int]$Failed,
        [Parameter(Mandatory)][int]$Pending,
        [Parameter(Mandatory)][bool]$RunCanceled,
        [Parameter(Mandatory)][AllowEmptyCollection()][object[]]$Events,
        [Parameter(Mandatory)][AllowEmptyCollection()][object[]]$TopCompression
    )

    $lines = [System.Collections.Generic.List[string]]::new()
    $lines.Add('Exact Flac Cruncher') | Out-Null
    $lines.Add('') | Out-Null
    $lines.Add(("EFC processing logfile from {0}" -f (Format-EacLogDateTime -Value $FinishedLocal))) | Out-Null
    $lines.Add('') | Out-Null
    $lines.Add($AlbumName) | Out-Null
    $lines.Add('') | Out-Null
    $lines.Add((Format-EacValueLine -Label 'Source folder' -Value $RootFolder)) | Out-Null
    $lines.Add((Format-EacValueLine -Label 'Run started' -Value (Format-EacLogDateTime -Value $RunStartedLocal))) | Out-Null
    $lines.Add((Format-EacValueLine -Label 'Run finished' -Value (Format-EacLogDateTime -Value $FinishedLocal))) | Out-Null
    $lines.Add((Format-EacValueLine -Label 'Worker threads' -Value ("{0}" -f $MaxWorkers))) | Out-Null
    $lines.Add((Format-EacValueLine -Label 'Retry limit' -Value ("{0}" -f $MaxAttemptsPerFile))) | Out-Null
    $lines.Add((Format-EacValueLine -Label 'Files discovered' -Value ("{0}" -f $TotalFiles))) | Out-Null
    $lines.Add('') | Out-Null

    foreach ($event in $Events) {
        $lines.Add('File') | Out-Null
        $lines.Add('') | Out-Null
        $lines.Add(("     Filename {0}" -f $event.FullPath)) | Out-Null
        $lines.Add('') | Out-Null
        $lines.Add((Format-EacValueLine -Label 'Logged at' -Value (Format-EacLogDateTime -Value $event.Timestamp))) | Out-Null
        $lines.Add((Format-EacValueLine -Label 'Attempt' -Value $event.Attempt)) | Out-Null
        $lines.Add((Format-EacValueLine -Label 'Verification' -Value $event.Verification)) | Out-Null

        if ($event.EventType -eq 'OK') {
            $lines.Add((Format-EacValueLine -Label 'Original size' -Value $event.OrigBytes)) | Out-Null
            $lines.Add((Format-EacValueLine -Label 'Compressed size' -Value $event.NewBytes)) | Out-Null
            $lines.Add((Format-EacValueLine -Label 'Net saved' -Value ("{0} ({1})" -f $event.SavedBytes, $event.SavedPct))) | Out-Null
            $lines.Add((Format-EacValueLine -Label 'Audio delta' -Value $event.AudioSavedBytes)) | Out-Null
            $lines.Add((Format-EacValueLine -Label 'Embedded MD5' -Value $event.EmbeddedHash)) | Out-Null
            $lines.Add((Format-EacValueLine -Label 'Calculated pre MD5' -Value $event.CalcPreHash)) | Out-Null
            $lines.Add((Format-EacValueLine -Label 'Calculated post MD5' -Value $event.CalcPostHash)) | Out-Null
            if (-not [string]::IsNullOrWhiteSpace($event.MetadataSummary) -and $event.MetadataSummary -ne 'none') {
                $lines.Add((Format-EacValueLine -Label 'Metadata cleanup' -Value $event.MetadataSummary)) | Out-Null
            }
            $lines.Add('     Copy OK') | Out-Null
        }
        elseif ($event.EventType -eq 'RETRY') {
            $lines.Add('     Copy aborted') | Out-Null
            $lines.Add('     Retry scheduled') | Out-Null
            $lines.Add((Format-EacValueLine -Label 'Reason' -Value $event.FailureReason)) | Out-Null
        }
        elseif ($event.EventType -eq 'CANCELED') {
            $lines.Add('     Copy aborted') | Out-Null
            $lines.Add((Format-EacValueLine -Label 'Reason' -Value $event.FailureReason)) | Out-Null
        }
        else {
            $lines.Add('     Copy failed') | Out-Null
            $lines.Add((Format-EacValueLine -Label 'Reason' -Value $event.FailureReason)) | Out-Null
        }

        $lines.Add('') | Out-Null
    }

    foreach ($statusLine in (New-EfcStatusReportLines -Successful $Successful -Failed $Failed -Pending $Pending -RunCanceled:$RunCanceled)) {
        $lines.Add($statusLine) | Out-Null
    }

    if ($TopCompression.Count -gt 0) {
        $lines.Add('') | Out-Null
        $lines.Add('---- EFC Compression Notes') | Out-Null
        $lines.Add('') | Out-Null
        $rank = 0
        foreach ($entry in $TopCompression) {
            $rank++
            $lines.Add((Format-TopCompressionLine -Rank $rank -Entry $entry)) | Out-Null
        }
    }

    $body = [string]::Join([Environment]::NewLine, $lines)
    $checksumLine = "==== Log checksum {0} ====" -f (Get-TextSha256 -Text $body)
    return ($body + [Environment]::NewLine + [Environment]::NewLine + $checksumLine)
}

#endregion Reporting

#region Job Management

function Stop-ActiveJobsAndCleanup {
    param([Parameter(Mandatory)][object[]]$Workers)

    [int]$killed = 0
    [int]$tmpDeleted = 0

    foreach ($w in $Workers) {
        if ($null -eq $w.Job) { continue }
        $job = $w.Job

        try {
            if ($null -ne $job.Proc) {
                $stillRunning = $false
                try { $stillRunning = -not $job.Proc.HasExited } catch { $stillRunning = $false }

                if ($stillRunning) {
                    $stopIssued = $false

                    try {
                        $job.Proc.Kill($true)
                        $stopIssued = $true
                    }
                    catch { }

                    if ($stopIssued) {
                        try { $null = $job.Proc.WaitForExit(2000) } catch { }
                    }

                    $stillRunning = $false
                    try {
                        $job.Proc.Refresh()
                        $stillRunning = -not $job.Proc.HasExited
                    }
                    catch { $stillRunning = $false }

                    if ($stillRunning) {
                        try {
                            Stop-Process -Id $job.Proc.Id -Force -ErrorAction Stop
                            $stopIssued = $true
                        }
                        catch { }

                        try { $null = $job.Proc.WaitForExit(2000) } catch { }
                    }

                    if ($stopIssued) { $killed++ }
                }
            }
        }
        catch { }

        try {
            if ($null -ne $job.HashJob) {
                if ($job.HashJob.State -eq 'Running' -or $job.HashJob.State -eq 'NotStarted') {
                    Stop-Job -Job $job.HashJob -Force -ErrorAction SilentlyContinue | Out-Null
                }
                Remove-Job -Job $job.HashJob -Force -ErrorAction SilentlyContinue | Out-Null
            }
        }
        catch { }

        # Originals are replaced only after verification. On cancel, remove temp files.
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

#endregion Job Management

#region Test Runner

# Handle -RunTests early exit
if ($RunTests) {
    $pesterModule = Get-Module Pester -ListAvailable | Sort-Object Version -Descending | Select-Object -First 1
    if ($null -eq $pesterModule -or $pesterModule.Version.Major -lt 5) {
        Write-Host "Installing Pester 5+..." -ForegroundColor Cyan
        Install-Module Pester -Force -Scope CurrentUser -MinimumVersion 5.0.0 -SkipPublisherCheck
    }
    Import-Module Pester -MinimumVersion 5.0.0
    $testFile = Join-Path $PSScriptRoot 'Tests' 'Start-ExactFlacCrunch.Tests.ps1'
    if (-not (Test-Path -LiteralPath $testFile)) {
        throw "Test file not found: $testFile"
    }
    Invoke-Pester $testFile -CI
    return
}

#endregion Test Runner

# Guard: when EFC_LOAD_FUNCTIONS_ONLY is '1', export all function definitions but skip main orchestration.
if ($env:EFC_LOAD_FUNCTIONS_ONLY -eq '1') { return }

#region Main Orchestration

# Preconditions

# Collect all root folders: $RootFolder is [string[]] with ValueFromRemainingArguments, so all positional args land here
$RootFolders = [System.Collections.Generic.List[string]]::new()
foreach ($a in @($RootFolder)) {
    $trimmed = ([string]$a).TrimEnd('\', '/')
    if (-not [string]::IsNullOrWhiteSpace($trimmed)) {
        $RootFolders.Add($trimmed) | Out-Null
    }
}

if ($RootFolders.Count -eq 0) {
    throw "RootFolder is required. Provide a folder path as the first argument."
}

foreach ($folder in $RootFolders) {
    if (-not (Test-Path -LiteralPath $folder)) { throw "RootFolder does not exist: $folder" }
    $folderItem = Get-Item -LiteralPath $folder -Force
    if (-not $folderItem.PSIsContainer) { throw "RootFolder is not a directory: $folder" }
}

$isMultiFolder = $RootFolders.Count -gt 1
$RootFolder = $RootFolders[0]
$rootItem = Get-Item -LiteralPath $RootFolder -Force

$toolBaseDirectory = $PSScriptRoot
if ([string]::IsNullOrWhiteSpace($toolBaseDirectory)) {
    try {
        $toolBaseDirectory = Split-Path -Path $PSCommandPath -Parent
    }
    catch {
        $toolBaseDirectory = (Get-Location).Path
    }
}

if ($script:IsWindowsHost) {
    Refresh-ProcessPathFromRegistry
}

# Attempt to resolve required tools; if missing and -InstallDeps is set, auto-install them
$flacCmd = Resolve-OptionalTool -Names @('flac', 'flac.exe') -BaseDirectory $toolBaseDirectory
$metaflacCmd = Resolve-OptionalTool -Names @('metaflac', 'metaflac.exe') -BaseDirectory $toolBaseDirectory

if ($null -eq $flacCmd -or $null -eq $metaflacCmd) {
    if ($InstallDeps) {
        Write-Host 'Required tools not found. Installing dependencies...' -ForegroundColor Yellow
        Install-RequiredDependencies
        if ($script:IsWindowsHost) { Refresh-ProcessPathFromRegistry }
        # Retry resolution
        $flacCmd = Resolve-RequiredTool -Names @('flac', 'flac.exe') -BaseDirectory $toolBaseDirectory -DisplayName 'flac'
        $metaflacCmd = Resolve-RequiredTool -Names @('metaflac', 'metaflac.exe') -BaseDirectory $toolBaseDirectory -DisplayName 'metaflac'
    }
    else {
        $missing = @()
        if ($null -eq $flacCmd) { $missing += 'flac' }
        if ($null -eq $metaflacCmd) { $missing += 'metaflac' }
        throw ("Required tool(s) not found: {0}. Add them to PATH, place beside this script, or rerun with -InstallDeps." -f ($missing -join ', '))
    }
}

# Validate flac version
$flacVersion = Test-FlacVersion -FlacExePath $flacCmd.Source
if ($null -ne $flacVersion) {
    Write-Host ("flac version: {0}" -f $flacVersion) -ForegroundColor DarkGray
}

$pngOptimizerCmd = Resolve-OptionalTool -Names @('oxipng', 'oxipng.exe', 'pngcrush', 'pngcrush.exe') -BaseDirectory $toolBaseDirectory
$jpegOptimizerCmd = Resolve-OptionalTool -Names @('jpegtran', 'jpegtran.exe') -BaseDirectory $toolBaseDirectory

if ($isMultiFolder) {
    $albumName = '[multifolder]'
    $safeAlbumName = '_multifolder_'
} else {
    $albumName = Get-RootDisplayName -Path $RootFolder -Item $rootItem
    $safeAlbumName = Get-SafeName -Value $albumName
}
$targetDisplay = if ($isMultiFolder) { "[multifolder] ({0} folders)" -f $RootFolders.Count } else { $RootFolder }
$runStartedLocal = Get-Date
$runStartedUtc = $runStartedLocal.ToUniversalTime()
$runStamp = $runStartedLocal.ToString('yyyyMMdd-HHmmss-fff')
if ([string]::IsNullOrWhiteSpace($LogFolder)) {
    $LogFolder = Get-DefaultLogFolder
}

New-Item -ItemType Directory -Path $LogFolder -Force | Out-Null
$runLogDir = Join-Path -Path $LogFolder -ChildPath ("{0}_{1}" -f $safeAlbumName, $runStamp)
New-Item -ItemType Directory -Path $runLogDir -Force | Out-Null

$runtimeCaptureDir = Join-Path -Path $runLogDir -ChildPath '.runtime'
New-Item -ItemType Directory -Path $runtimeCaptureDir -Force | Out-Null

$jobLogDir = Join-Path -Path $runLogDir -ChildPath 'job-logs'
New-Item -ItemType Directory -Path $jobLogDir -Force | Out-Null

$logFile = Join-Path -Path $runLogDir -ChildPath ("{0}_{1}.log" -f $safeAlbumName, $runStamp)
$script:LogFile = $logFile
$verboseLogFile = $null
$script:VerboseLogFile = $null
if (Test-VerboseUiEnabled) {
    $verboseLogFile = Join-Path -Path $runLogDir -ChildPath ("verbose-trace_{0}.log" -f $runStamp)
    $script:VerboseLogFile = $verboseLogFile
    @"
Exact Flac Cruncher Verbose Trace
Target: $targetDisplay
Run Logs: $runLogDir
Started: $($runStartedLocal.ToString('o'))
===================================================================
"@ | Out-File -LiteralPath $verboseLogFile -Encoding UTF8
}

@"
Exact Flac Cruncher v$($script:Version)
Target: $targetDisplay
Log Root: $LogFolder
Run Logs: $runLogDir
Started: $($runStartedLocal.ToString('o'))
===================================================================
"@ | Out-File -LiteralPath $logFile -Encoding UTF8

# Affinity support: classic processor mask handles up to 64 logical processors.
$cpuCount = [Environment]::ProcessorCount
$affinityEnabled = $false
if ($script:IsWindowsHost -and $cpuCount -le 64) {
    $affinityEnabled = $true
}
elseif ($script:IsWindowsHost -and $cpuCount -gt 64) {
    Write-RunLog -Level WARN -Message ("Host has {0} logical processors; processor groups not handled. Affinity disabled." -f $cpuCount)
}

function Set-SingleCoreAffinity {
    param(
        [Parameter(Mandatory)][System.Diagnostics.Process]$Process,
        [Parameter(Mandatory)][int]$CoreIndexZeroBased
    )

    # ProcessorAffinity is only supported on Windows.
    if (-not $script:IsWindowsHost) { return }
    if (-not $affinityEnabled) { return }

    if ($CoreIndexZeroBased -lt 0 -or $CoreIndexZeroBased -gt 63) {
        Write-RunLog -Level WARN -Message ("Core index {0} out of mask range; affinity skipped for PID {1}." -f $CoreIndexZeroBased, $Process.Id)
        return
    }

    try {
        # Use signed Int64 shifting so bit 63 remains valid.
        $mask = [IntPtr](1L -shl $CoreIndexZeroBased)
        $Process.ProcessorAffinity = $mask
    }
    catch {
        Write-RunLog -Level WARN -Message ("Failed to set affinity | PID {0} | {1}" -f $Process.Id, $_.Exception.Message)
    }
}

function Start-DecodedAudioHashJob {
    param(
        [Parameter(Mandatory)][string]$FlacExePath,
        [Parameter(Mandatory)][string]$Path
    )

    $jobScript = {
        param(
            [string]$ExePath,
            [string]$FilePath
        )

        function Invoke-FlacHashFromStdout {
            param(
                [Parameter(Mandatory)][string]$InnerExePath,
                [Parameter(Mandatory)][string]$Args
            )

            $psi = [System.Diagnostics.ProcessStartInfo]::new()
            $psi.FileName = $InnerExePath
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
                $null = $proc.StandardError.ReadToEnd()
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
            $quotedPath = if ($FilePath -notmatch '[\s"]') { $FilePath } else { '"' + ($FilePath -replace '"', '\"') + '"' }
            $rawArgVariants = @(
                ("-d -c -s --force-raw-format --endian=little --sign=signed -- " + $quotedPath),
                ("-d -c -s --force-raw-format -- " + $quotedPath)
            )

            foreach ($args in $rawArgVariants) {
                $hash = Invoke-FlacHashFromStdout -InnerExePath $ExePath -Args $args
                if (-not [string]::IsNullOrWhiteSpace($hash)) { return $hash }
            }
        }
        catch { }

        return $null
    }

    try {
        return Start-Job -ScriptBlock $jobScript -ArgumentList $FlacExePath, $Path
    }
    catch {
        return $null
    }
}

# Check root folder writability. Subfolders may vary, so permission issues below are handled per item.
$nonWritableFolders = @($RootFolders | Where-Object { -not (Test-DirectoryWriteAccess -Path $_) })
if ($nonWritableFolders.Count -gt 0) {
    $warningText = "One or more root folders failed a temp write test. The run may still work for readable/writable subfolders, but permission-related failures will be reported clearly."
    Write-Host $warningText -ForegroundColor Yellow
    foreach ($nwf in $nonWritableFolders) {
        Write-RunLog -Level WARN -Message ("{0} | Path: {1}" -f $warningText, $nwf)
    }

    $continueAnswer = Read-Host "Continue anyway? [y/N]"
    if ($continueAnswer -notmatch '^(?i)y(?:es)?$') {
        Write-RunLog -Level WARN -Message "Run cancelled during root-folder write preflight."
        Flush-RunLog -Force
        Write-Host "Cancelled before starting conversions."
        return
    }
}

# Cleanup stale .tmp files (conservative):
# Delete *.tmp only when same-base *.flac exists (track.tmp <-> track.flac).

$cleanupScanErrors = @()
foreach ($folder in $RootFolders) {
    Get-ChildItem -LiteralPath $folder -Recurse -File -Force -ErrorAction SilentlyContinue -ErrorVariable +cleanupScanErrors |
    Where-Object { $_.Extension -ieq '.tmp' } |
    ForEach-Object {
        $maybeFlac = [System.IO.Path]::ChangeExtension($_.FullName, '.flac')
        if (Test-Path -LiteralPath $maybeFlac) {
            Safe-RemoveFile -Path $_.FullName
        }
    }
}

# Collect FLAC files

$fileScanErrors = @()
$files = @(
    $RootFolders | ForEach-Object {
        Get-ChildItem -LiteralPath $_ -Recurse -File -Force -ErrorAction SilentlyContinue -ErrorVariable +fileScanErrors -Filter *.flac
    } |
    Sort-Object -Property @{ Expression = 'Length'; Descending = $true }, @{ Expression = 'FullName'; Descending = $false }
)

$permissionScanPaths = @(
    @($cleanupScanErrors) + @($fileScanErrors) |
    Where-Object { Get-FriendlyPermissionMessage -Operation 'scanning folders' -Path $_.TargetObject -Exception $_.Exception -Details $_.ToString() } |
    ForEach-Object {
        if (-not [string]::IsNullOrWhiteSpace([string]$_.TargetObject)) {
            [string]$_.TargetObject
        }
    } |
    Sort-Object -Unique
)

if ($permissionScanPaths.Count -gt 0) {
    $warningLines = [System.Collections.Generic.List[string]]::new()
    $warningLines.Add(("Some folders/files were skipped during scan due to permissions: {0}" -f $permissionScanPaths.Count)) | Out-Null
    foreach ($path in ($permissionScanPaths | Select-Object -First 3)) {
        $warningLines.Add(("  SKIPPED: {0}" -f $path)) | Out-Null
    }
    if ($permissionScanPaths.Count -gt 3) {
        $warningLines.Add(("  ... plus {0} more" -f ($permissionScanPaths.Count - 3))) | Out-Null
    }

    $warningText = [string]::Join([Environment]::NewLine, $warningLines)
    Write-Host $warningText -ForegroundColor Yellow
    Write-RunLog -Level WARN -Message ([regex]::Replace($warningText, '\r?\n', ' | '))
}

$totalFiles = $files.Count
if ($totalFiles -eq 0) {
    $noFlacMessage = if ($isMultiFolder) { "No FLAC files found across {0} folders" -f $RootFolders.Count } else { "No FLAC files found under: $RootFolder" }
    Write-RunLog -Level WARN -Message $noFlacMessage
    Flush-RunLog -Force
    Write-Host ("ERROR: {0}" -f $noFlacMessage) -ForegroundColor Red
    return
}

# Default worker count: reserve one logical processor for the main thread/UI unless overridden.
$availableWorkerSlots = if ($PSBoundParameters.ContainsKey('Threads')) {
    $Threads
}
else {
    $defaultWorkerSlots = [Environment]::ProcessorCount
    if ($defaultWorkerSlots -gt 1) {
        $defaultWorkerSlots--
    }

    $defaultWorkerSlots
}
$maxWorkers = [Math]::Min($availableWorkerSlots, $totalFiles)
if ($maxWorkers -lt 1) { $maxWorkers = 1 }

$maxAttemptsPerFile = $script:MaxAttemptsPerFile
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

$nullHash = $script:NullHash

[long]$totalOriginalBytes = 0
[long]$totalNewBytes = 0
[long]$totalSavedBytes = 0
[long]$totalMetadataSavedBytes = 0
[long]$totalPaddingTrimSavedBytes = 0
[int]$paddingTrimFiles = 0
[long]$totalArtworkSavedBytes = 0
[long]$totalArtworkRawSavedBytes = 0
[int]$processed = 0
[int]$failed = 0
[int]$conversionAttempts = 0
[int]$artworkOptimizedFiles = 0
[int]$artworkOptimizedBlocks = 0

$compressionResults = [System.Collections.Generic.List[object]]::new()
$failedResults = [System.Collections.Generic.List[object]]::new()
$finalLogEvents = [System.Collections.Generic.List[object]]::new()

$recentEvents = [System.Collections.Generic.List[object]]::new()
for ($i = 1; $i -le 25; $i++) {
    $recentEvents.Add([PSCustomObject]@{
            Time           = '--:--:--'
            Status         = 'WAIT'
            File           = '(waiting for first result)'
            Attempt        = '-'
            EmbeddedHash   = 'N/A'
            CalcBeforeHash = 'N/A'
            CalcAfterHash  = 'N/A'
            Verification   = 'N/A'
            BeforeAfter    = 'N/A'
            Saved          = 'N/A'
            CompressionPct = 'N/A'
            Detail         = 'Pending'
        }) | Out-Null
}

$workers = for ($i = 0; $i -lt $maxWorkers; $i++) {
    [PSCustomObject]@{
        Id      = $i + 1
        CoreIdx = $i
        Job     = $null
    }
}

$interactive = ($Host.Name -eq 'ConsoleHost')
if ($interactive) {
    try {
        [Console]::CursorVisible = $false
        Clear-Host
        Sync-ConsoleBufferToWindow
    }
    catch { $interactive = $false }
}
$script:UiInteractiveMode = $interactive

$installPromptResult = Invoke-OptionalOptimizerInstallPrompt `
    -PngOptimizerCommand $pngOptimizerCmd `
    -JpegOptimizerCommand $jpegOptimizerCmd `
    -BaseDirectory $toolBaseDirectory `
    -Interactive:$interactive
$pngOptimizerCmd = $installPromptResult.PngOptimizerCommand
$jpegOptimizerCmd = $installPromptResult.JpegOptimizerCommand

$pngToolStatus = if ($pngOptimizerCmd) { [System.IO.Path]::GetFileNameWithoutExtension([string]$pngOptimizerCmd.Source) } else { 'OFF' }
$jpegToolStatus = if ($jpegOptimizerCmd) { [System.IO.Path]::GetFileNameWithoutExtension([string]$jpegOptimizerCmd.Source) } else { 'OFF' }

if ($installPromptResult.Prompted) {
    if ($pngOptimizerCmd) {
        Write-Host ("PNG optimizer ready: {0}" -f $pngOptimizerCmd.Source) -ForegroundColor Green
    }
    else {
        Write-Host "PNG optimizer still unavailable; continuing without PNG album-art optimization." -ForegroundColor Yellow
    }

    if ($jpegOptimizerCmd) {
        Write-Host ("JPEG optimizer ready: {0}" -f $jpegOptimizerCmd.Source) -ForegroundColor Green
    }
    else {
        Write-Host "JPEG optimizer still unavailable; continuing without JPEG album-art optimization." -ForegroundColor Yellow
    }
}

if ($pngOptimizerCmd) {
    Write-RunLog -Level INFO -Message ("Optional PNG album-art optimizer enabled: {0}" -f $pngOptimizerCmd.Source)
}
else {
    $pngWarn = "Optional PNG album-art optimization disabled; add 'oxipng' (preferred) or 'pngcrush' to PATH or place the EXE beside this script."
    Write-Warning $pngWarn
    Write-RunLog -Level WARN -Message $pngWarn
}

if ($jpegOptimizerCmd) {
    Write-RunLog -Level INFO -Message ("Optional JPEG album-art optimizer enabled: {0}" -f $jpegOptimizerCmd.Source)
}
else {
    $jpegWarn = "Optional JPEG album-art optimization disabled; add 'jpegtran' to PATH or place the EXE beside this script."
    Write-Warning $jpegWarn
    Write-RunLog -Level WARN -Message $jpegWarn
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

# Throttled status output for non-interactive hosts.
$lastStatusUtc = [DateTime]::UtcNow
$statusInterval = [TimeSpan]::FromSeconds(10)
$runCanceled = $false
$progressCache = @{}
$lastUiFrameUtc = [DateTime]::MinValue
$uiFrameInterval = [TimeSpan]::FromMilliseconds(250)
$lastUiRenderRows = 0
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

        if ($script:CancelRequested) {
            $runCanceled = $true
            $uiBanner = "Cancellation requested... stopping active jobs and cleaning temp files."
            $uiDirty = $true
            if ($interactive) {
                $lastUiRenderRows = Render-InteractiveUi `
                    -AlbumName $albumName `
                    -RunStartedUtc $runStartedUtc `
                    -Processed $processed `
                    -TotalFiles $totalFiles `
                    -Failed $failed `
                    -TotalSavedBytes $totalSavedBytes `
                    -TotalMetadataSavedBytes $totalMetadataSavedBytes `
                    -TotalPaddingTrimSavedBytes $totalPaddingTrimSavedBytes `
                    -PaddingTrimFiles $paddingTrimFiles `
                    -TotalArtworkSavedBytes $totalArtworkSavedBytes `
                    -TotalArtworkRawSavedBytes $totalArtworkRawSavedBytes `
                    -ArtworkOptimizedFiles $artworkOptimizedFiles `
                    -ArtworkOptimizedBlocks $artworkOptimizedBlocks `
                    -QueueCount $queue.Count `
                    -MaxAttemptsPerFile $maxAttemptsPerFile `
                    -Workers @($workers) `
                    -RecentEvents $recentEvents `
                    -TopCompression @(
                    $compressionResults |
                    Where-Object { $_.SavedBytes -gt 0 } |
                    Sort-Object -Property SavedBytes -Descending |
                    Select-Object -First 3
                ) `
                    -ProgressCache $progressCache `
                    -VerboseMessages $script:VerboseUiMessages `
                    -PngToolStatus $pngToolStatus `
                    -JpegToolStatus $jpegToolStatus `
                    -PreviousRows $lastUiRenderRows `
                    -Banner $uiBanner
            }
            Write-RunLog -Level WARN -Message "Cancellation requested by user (Ctrl+C)."

            $cancelResult = Stop-ActiveJobsAndCleanup -Workers @($workers)
            $queue.Clear()

            Write-RunLog -Level WARN -Message ("Cancellation cleanup complete | ProcessesStopped: {0} | TempFilesDeleted: {1}" -f $cancelResult.Killed, $cancelResult.TempDeleted)
            break
        }

        if ($interactive) {
            $nowUtc = [DateTime]::UtcNow
            if ($uiDirty -or (($nowUtc - $lastUiFrameUtc) -ge $uiFrameInterval)) {
                $lastUiRenderRows = Render-InteractiveUi `
                    -AlbumName $albumName `
                    -RunStartedUtc $runStartedUtc `
                    -Processed $processed `
                    -TotalFiles $totalFiles `
                    -Failed $failed `
                    -TotalSavedBytes $totalSavedBytes `
                    -TotalMetadataSavedBytes $totalMetadataSavedBytes `
                    -TotalPaddingTrimSavedBytes $totalPaddingTrimSavedBytes `
                    -PaddingTrimFiles $paddingTrimFiles `
                    -TotalArtworkSavedBytes $totalArtworkSavedBytes `
                    -TotalArtworkRawSavedBytes $totalArtworkRawSavedBytes `
                    -ArtworkOptimizedFiles $artworkOptimizedFiles `
                    -ArtworkOptimizedBlocks $artworkOptimizedBlocks `
                    -QueueCount $queue.Count `
                    -MaxAttemptsPerFile $maxAttemptsPerFile `
                    -Workers @($workers) `
                    -RecentEvents $recentEvents `
                    -TopCompression @(
                    $compressionResults |
                    Where-Object { $_.SavedBytes -gt 0 } |
                    Sort-Object -Property SavedBytes -Descending |
                    Select-Object -First 3
                ) `
                    -ProgressCache $progressCache `
                    -VerboseMessages $script:VerboseUiMessages `
                    -PngToolStatus $pngToolStatus `
                    -JpegToolStatus $jpegToolStatus `
                    -PreviousRows $lastUiRenderRows `
                    -Banner $uiBanner
                $lastUiFrameUtc = $nowUtc
                $uiDirty = $false
            }
        }

        foreach ($w in $workers) {

            # Finalize completed job.
            if ($null -ne $w.Job) {
                $job = $w.Job
                if ([string]::IsNullOrWhiteSpace($job.Stage)) { $job.Stage = 'CONVERTING' }

                if ($job.Stage -eq 'HASHING') {
                    if ($null -eq $job.HashJob) {
                        $job.FailureReason = "Decoded-audio hash worker unavailable"
                        $job.Stage = 'FINALIZING'
                        $uiDirty = $true
                    }
                    else {
                        $hashState = $job.HashJob.State
                        if ($hashState -eq 'Completed' -or $hashState -eq 'Failed' -or $hashState -eq 'Stopped') {
                            $hashOutput = @()
                            try { $hashOutput = @(Receive-Job -Job $job.HashJob -ErrorAction SilentlyContinue) } catch { }
                            try { Remove-Job -Job $job.HashJob -Force -ErrorAction SilentlyContinue | Out-Null } catch { }
                            $job.HashJob = $null

                            $hashValue = @($hashOutput | ForEach-Object { [string]$_ } | Where-Object { -not [string]::IsNullOrWhiteSpace($_) } | Select-Object -Last 1)
                            $decodedHash = if ($hashValue.Count -gt 0) { $hashValue[0].Trim() } else { $null }

                            if ($job.HashPhase -eq 'SOURCE') {
                                $job.PreCalcHash = $decodedHash
                                if ([string]::IsNullOrWhiteSpace($job.PreCalcHash)) {
                                    $job.FailureReason = "Could not calculate decoded-audio hash (pre)"
                                    $job.Stage = 'FINALIZING'
                                }
                                else {
                                    $job.HashPhase = 'OUTPUT'
                                    $job.HashJob = Start-DecodedAudioHashJob -FlacExePath $flacCmd.Source -Path $job.Temp
                                    if ($null -eq $job.HashJob) {
                                        $job.FailureReason = "Could not start decoded-audio hash worker (output)"
                                        $job.Stage = 'FINALIZING'
                                    }
                                }
                            }
                            elseif ($job.HashPhase -eq 'OUTPUT') {
                                $job.PostCalcHash = $decodedHash
                                if ([string]::IsNullOrWhiteSpace($job.PostCalcHash)) {
                                    $job.FailureReason = "Could not calculate decoded-audio hash (post)"
                                    $job.Stage = 'FINALIZING'
                                }
                                else {
                                    $job.Stage = 'ARTWORK'
                                }
                            }
                            else {
                                $job.FailureReason = "Unknown hash phase state"
                                $job.Stage = 'FINALIZING'
                            }
                            $uiDirty = $true
                        }
                    }
                    if ($job.Stage -eq 'ARTWORK') { continue }
                    if ($job.Stage -ne 'FINALIZING') { continue }
                }

                if ($job.Stage -eq 'CONVERTING') {
                    if ($null -eq $job.Proc -or -not $job.Proc.HasExited) { continue }

                    $job.ConvertExitCode = $job.Proc.ExitCode
                    try { $job.Proc.Dispose() } catch { }
                    $job.Proc = $null

                    $errText = ""
                    if (Test-Path -LiteralPath $job.ErrLog) {
                        try { $errText = (Get-Content -LiteralPath $job.ErrLog -Raw -ErrorAction SilentlyContinue).Trim() } catch { }
                    }
                    $job.ErrText = $errText
                    if (-not [string]::IsNullOrWhiteSpace($errText)) {
                        Write-VerboseUi -Message ("flac stderr | File: {0} | Exit: {1} | Output: {2}" -f $job.Original, $job.ConvertExitCode, (Format-ErrSnippet -Text $errText -MaxLength 250))
                    }

                    if ($job.ConvertExitCode -ne 0) {
                        $job.FailureReason = "flac exit $($job.ConvertExitCode)"
                        $job.Stage = 'FINALIZING'
                        $uiDirty = $true
                    }
                    elseif (-not (Test-Path -LiteralPath $job.Temp)) {
                        $job.FailureReason = "flac produced no temp output"
                        $job.Stage = 'FINALIZING'
                        $uiDirty = $true
                    }
                    else {
                        $embeddedPresent = ($job.EmbeddedHash -ne $nullHash)
                        if ([string]::IsNullOrWhiteSpace($job.PreCalcHash) -and $embeddedPresent) {
                            # Embedded stream MD5 already reflects decoded source audio.
                            $job.PreCalcHash = $job.EmbeddedHash
                        }

                        if ([string]::IsNullOrWhiteSpace($job.PreCalcHash)) {
                            $job.HashPhase = 'SOURCE'
                            $job.HashJob = Start-DecodedAudioHashJob -FlacExePath $flacCmd.Source -Path $job.Original
                            if ($null -eq $job.HashJob) {
                                $job.FailureReason = "Could not start decoded-audio hash worker (source)"
                                $job.Stage = 'FINALIZING'
                            }
                            else {
                                $job.Stage = 'HASHING'
                            }
                        }
                        else {
                            $job.HashPhase = 'OUTPUT'
                            $job.HashJob = Start-DecodedAudioHashJob -FlacExePath $flacCmd.Source -Path $job.Temp
                            if ($null -eq $job.HashJob) {
                                $job.FailureReason = "Could not start decoded-audio hash worker (output)"
                                $job.Stage = 'FINALIZING'
                            }
                            else {
                                $job.Stage = 'HASHING'
                            }
                        }
                        $uiDirty = $true
                    }
                    if ($job.Stage -ne 'FINALIZING') { continue }
                }

                if ($job.Stage -eq 'ARTWORK') {
                    $failureReason = $job.FailureReason
                    $embeddedPresent = ($job.EmbeddedHash -ne $nullHash)
                    if ([string]::IsNullOrWhiteSpace($job.PreCalcHash) -and $embeddedPresent) {
                        # Embedded stream MD5 already reflects decoded source audio.
                        $job.PreCalcHash = $job.EmbeddedHash
                    }

                    $postCalcHash = $null
                    if (-not [string]::IsNullOrWhiteSpace($job.PostCalcHash)) { $postCalcHash = [string]$job.PostCalcHash }

                    $hashOK = $false
                    $calcBeforeOk = -not [string]::IsNullOrWhiteSpace($job.PreCalcHash)
                    $calcAfterOk = -not [string]::IsNullOrWhiteSpace($postCalcHash)
                    $calcMatch = $calcBeforeOk -and $calcAfterOk -and ($job.PreCalcHash -eq $postCalcHash)

                    if ($calcMatch) {
                        $hashOK = $true
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
                        $job.ArtworkResult = Optimize-FlacAlbumArt `
                            -Path $job.Temp `
                            -MetaflacExePath $metaflacCmd.Source `
                            -PngOptimizerPath $(if ($pngOptimizerCmd) { $pngOptimizerCmd.Source } else { $null }) `
                            -JpegOptimizerPath $(if ($jpegOptimizerCmd) { $jpegOptimizerCmd.Source } else { $null }) `
                            -ScratchDir $runtimeCaptureDir `
                            -JobId $job.JobId
                    }
                    else {
                        if (-not $calcBeforeOk -or -not $calcAfterOk) {
                            $failureReason = "Could not calculate decoded-audio hash (pre or post)"
                        }
                        else {
                            $failureReason = "Decoded-audio hash mismatch (original vs converted)"
                        }
                    }

                    $job.FailureReason = $failureReason
                    $job.Stage = 'FINALIZING'
                    $uiDirty = $true
                    continue
                }

                if ($job.Stage -eq 'FINALIZING') {
                    $exitCode = if ($null -ne $job.ConvertExitCode) { [int]$job.ConvertExitCode } else { 1 }
                    $errText = [string]$job.ErrText

                    $postCalcHash = $null
                    if (-not [string]::IsNullOrWhiteSpace($job.PostCalcHash)) { $postCalcHash = [string]$job.PostCalcHash }
                    $finalized = $false
                    $failureReason = $job.FailureReason
                    $newSize = 0
                    $saved = 0
                    $embeddedPresent = ($job.EmbeddedHash -ne $nullHash)
                    $artResult = $job.ArtworkResult

                    if ($exitCode -eq 0 -and (Test-Path -LiteralPath $job.Temp) -and [string]::IsNullOrWhiteSpace($failureReason)) {
                        $newSize = (Get-Item -LiteralPath $job.Temp -Force).Length
                        $saved = $job.OrigSize - $newSize

                        # Replace original file, retrying with exponential backoff to tolerate transient locks.
                        $replaced = $false
                        $lastMoveException = $null
                        for ($attempt = 1; $attempt -le $script:MoveItemMaxRetries -and -not $replaced; $attempt++) {
                            try {
                                Move-Item -LiteralPath $job.Temp -Destination $job.Original -Force
                                $metadataRestored = Restore-FileMetadata -Path $job.Original -Snapshot $job.SourceMetadata
                                if (-not $metadataRestored) {
                                    Write-RunLog -Level WARN -Message ("File metadata restoration was only partially successful | File: {0}" -f $job.Original)
                                }
                                $replaced = $true
                            }
                            catch {
                                $lastMoveException = $_.Exception
                                if ($attempt -lt $script:MoveItemMaxRetries) {
                                    Start-Sleep -Milliseconds ($script:MoveItemRetryBaseMs * $attempt)
                                }
                            }
                        }

                        if ($replaced) {
                            $processed++
                            $totalOriginalBytes += $job.OrigSize
                            $totalNewBytes += $newSize
                            $totalSavedBytes += $saved
                            $audioNetSaved = [long]$saved
                            $metadataNetSaved = 0L
                            if ($null -ne $artResult -and $artResult.Changed -and $artResult.SavedBytes -gt 0) {
                                $metadataNetSaved = [long]$artResult.SavedBytes
                                $audioNetSaved = [long]$saved - $metadataNetSaved
                            }

                            if ($metadataNetSaved -gt 0) {
                                $totalMetadataSavedBytes += $metadataNetSaved
                            }
                            if ($null -ne $artResult -and $artResult.RawSavedBytes -gt 0) {
                                $totalArtworkRawSavedBytes += $artResult.RawSavedBytes
                            }
                            if ($null -ne $artResult -and $artResult.Changed -and $metadataNetSaved -gt 0) {
                                if ($artResult.BlocksOptimized -gt 0) {
                                    $totalArtworkSavedBytes += $metadataNetSaved
                                    $artworkOptimizedFiles++
                                    $artworkOptimizedBlocks += $artResult.BlocksOptimized
                                }
                                else {
                                    $totalPaddingTrimSavedBytes += $metadataNetSaved
                                    $paddingTrimFiles++
                                }
                            }

                            $artSummary = 'none'
                            $artDetailText = 'none'
                            if ($null -ne $artResult) {
                                if ($metadataNetSaved -gt 0) {
                                    if ($artResult.BlocksOptimized -gt 0) {
                                        $artSummary = ("MetadataCleanup {0} net ({1} raw image, {2} block{3})" -f (Format-Bytes $metadataNetSaved), (Format-Bytes $artResult.RawSavedBytes), $artResult.BlocksOptimized, $(if ($artResult.BlocksOptimized -eq 1) { '' } else { 's' }))
                                        $artDetailText = ("Meta {0} ({1} raw, {2} blk{3})" -f (Format-Bytes $metadataNetSaved), (Format-Bytes $artResult.RawSavedBytes), $artResult.BlocksOptimized, $(if ($artResult.BlocksOptimized -eq 1) { '' } else { 's' }))
                                    }
                                    else {
                                        $artSummary = ("MetadataCleanup {0} net (padding-only)" -f (Format-Bytes $metadataNetSaved))
                                        $artDetailText = ("Meta {0} (pad)" -f (Format-Bytes $metadataNetSaved))
                                    }
                                }
                                elseif ($artResult.Changed) {
                                    if ($artResult.BlocksOptimized -gt 0) {
                                        $artSummary = ("MetadataCleanup 00.00 B net ({0} raw image, no final-size change)" -f (Format-Bytes $artResult.RawSavedBytes))
                                        $artDetailText = ("Meta 00.00 B ({0} raw)" -f (Format-Bytes $artResult.RawSavedBytes))
                                    }
                                    else {
                                        $artSummary = "MetadataCleanup 00.00 B net (padding-only, no final-size change)"
                                        $artDetailText = "Meta 00.00 B (pad)"
                                    }
                                }
                                elseif (-not [string]::IsNullOrWhiteSpace($artResult.Summary)) {
                                    $artSummary = $artResult.Summary
                                    $artDetailText = $artResult.Summary
                                }
                            }

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

                            $verification = if ($embeddedPresent) { 'MATCH' } else { 'MATCH|NEW' }
                            $audioSavedPct = 0.0
                            if ($job.OrigSize -gt 0) {
                                $audioSavedPct = [Math]::Round((($audioNetSaved / [double]$job.OrigSize) * 100.0), 2)
                            }
                            $detailParts = [System.Collections.Generic.List[string]]::new()
                            $detailParts.Add(("Audio {0} ({1:N2}%)" -f (Format-Bytes $audioNetSaved -Signed), $audioSavedPct)) | Out-Null
                            if ($null -ne $artResult -and $artResult.Changed) {
                                $detailParts.Add($artDetailText) | Out-Null
                            }
                            elseif ($null -ne $artResult -and $artDetailText -ne 'none') {
                                $detailParts.Add($artDetailText) | Out-Null
                            }
                            $detailText = [string]::Join(' | ', $detailParts)
                            Add-FinalLogEvent -List $finalLogEvents `
                                -EventType 'OK' `
                                -File $job.Name `
                                -FullPath $job.Original `
                                -Attempt ("{0}/{1}" -f $job.Attempt, $maxAttemptsPerFile) `
                                -Verification $verification `
                                -EmbeddedHash (Format-HashForLog -Hash $job.EmbeddedHash -NullHash $nullHash) `
                                -CalcPreHash (Format-HashForLog -Hash $job.PreCalcHash -NullHash $nullHash) `
                                -CalcPostHash (Format-HashForLog -Hash $postCalcHash -NullHash $nullHash) `
                                -OrigBytes (Format-Bytes $job.OrigSize) `
                                -NewBytes (Format-Bytes $newSize) `
                                -SavedBytes (Format-Bytes $saved) `
                                -SavedPct ("{0:N2}%" -f $savedPct) `
                                -AudioSavedBytes (Format-Bytes $audioNetSaved -Signed) `
                                -MetadataSummary $artSummary
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
                                -Detail $detailText

                            Write-RunLog -Level SUCCESS -Message ("File: {0} | Attempt: {1}/{2} | Embedded: {3} | CalcPre: {4} | CalcPost: {5} | Verification: {6} | Size: {7}->{8} | Saved: {9} ({10:N2}%) | MetadataCleanup: {11}" -f $job.Original, $job.Attempt, $maxAttemptsPerFile, $job.EmbeddedHash, $job.PreCalcHash, $postCalcHash, $verification, (Format-Bytes $job.OrigSize), (Format-Bytes $newSize), (Format-Bytes $saved), $savedPct, $artSummary)
                            Write-VerboseUi -Message ("Finalized | File: {0} | Total Saved: {1} | Audio+Container Before/After: {2}->{3} | MetadataCleanup: {4}" -f $job.Original, (Format-Bytes $saved), (Format-Bytes $job.OrigSize), (Format-Bytes $newSize), $artSummary)
                            $finalized = $true
                        }
                        else {
                            $permissionFailure = Get-FriendlyPermissionMessage -Operation 'replacing original file' -Path $job.Original -Exception $lastMoveException
                            if (-not [string]::IsNullOrWhiteSpace($permissionFailure)) {
                                $failureReason = $permissionFailure
                            }
                            else {
                                $failureReason = "Could not replace original after retries (file may be locked)"
                            }
                        }
                    }
                    else {
                        if ([string]::IsNullOrWhiteSpace($failureReason)) {
                            if ($exitCode -ne 0) {
                                $failureReason = "flac exit $exitCode"
                            }
                            else {
                                $failureReason = "flac produced no temp output"
                            }
                        }
                    }

                    $permissionFailure = Get-FriendlyPermissionMessage -Operation 'converting file' -Path $job.Original -Details $errText
                    if (-not [string]::IsNullOrWhiteSpace($permissionFailure)) {
                        $failureReason = $permissionFailure
                    }

                    if (-not $finalized) {
                        Safe-RemoveFile -Path $job.Temp

                        $nonRetryableDecodeError = $false
                        if (-not [string]::IsNullOrWhiteSpace($errText)) {
                            if ($errText -match 'FLAC__STREAM_DECODER_ERROR_STATUS_LOST_SYNC' -or
                                $errText -match 'while decoding FLAC input, state = FLAC__STREAM_DECODER_ABORTED') {
                                $nonRetryableDecodeError = $true
                            }
                        }

                        if ($nonRetryableDecodeError -and -not [string]::IsNullOrWhiteSpace($failureReason)) {
                            $failureReason = "{0} (non-retryable decode corruption)" -f $failureReason
                        }

                        $shouldRetry = (($job.Attempt -lt $maxAttemptsPerFile) -and (-not $nonRetryableDecodeError))
                        if ($shouldRetry) {
                            $nextAttempt = $job.Attempt + 1
                            $queue.Enqueue([PSCustomObject]@{
                                    FileId   = $job.FileId
                                    Path     = $job.Original
                                    Name     = $job.Name
                                    Attempts = $nextAttempt
                                })

                            $verification = if (-not [string]::IsNullOrWhiteSpace($job.PreCalcHash) -and
                                -not [string]::IsNullOrWhiteSpace($postCalcHash) -and
                                ($job.PreCalcHash -eq $postCalcHash)) {
                                if ($embeddedPresent) { 'MATCH' } else { 'MATCH|NEW' }
                            }
                            else { 'MISMATCH' }
                            $beforeAfter = if ($newSize -gt 0) { ("{0} -> {1}" -f (Format-Bytes $job.OrigSize), (Format-Bytes $newSize)) } else { ("{0} -> N/A" -f (Format-Bytes $job.OrigSize)) }
                            Add-FinalLogEvent -List $finalLogEvents `
                                -EventType 'RETRY' `
                                -File $job.Name `
                                -FullPath $job.Original `
                                -Attempt ("{0}/{1}" -f $job.Attempt, $maxAttemptsPerFile) `
                                -Verification $verification `
                                -EmbeddedHash (Format-HashForLog -Hash $job.EmbeddedHash -NullHash $nullHash) `
                                -CalcPreHash (Format-HashForLog -Hash $job.PreCalcHash -NullHash $nullHash) `
                                -CalcPostHash (Format-HashForLog -Hash $postCalcHash -NullHash $nullHash) `
                                -OrigBytes (Format-Bytes $job.OrigSize) `
                                -NewBytes $(if ($newSize -gt 0) { Format-Bytes $newSize } else { 'N/A' }) `
                                -FailureReason $failureReason
                            Push-RecentEvent -List $recentEvents `
                                -Status 'RETRY' `
                                -File $job.Name `
                                -Attempt ("{0}/{1}" -f $nextAttempt, $maxAttemptsPerFile) `
                                -EmbeddedHash (Format-HashForUi -Hash $job.EmbeddedHash -NullHash $nullHash) `
                                -CalculatedBeforeHash (Format-HashForUi -Hash $job.PreCalcHash -NullHash $nullHash) `
                                -CalculatedAfterHash (Format-HashForUi -Hash $postCalcHash -NullHash $nullHash) `
                                -Verification $verification `
                                -BeforeAfter $beforeAfter `
                                -Saved 'N/A' `
                                -CompressionPct 'N/A' `
                                -Detail $failureReason
                            Write-RunLog -Level WARN -Message ("Retrying | File: {0} | NextAttempt: {1}/{2} | Reason: {3} | Embedded: {4} | CalcPre: {5} | CalcPost: {6} | Verification: {7} | STDERR: {8} | ErrLog: {9} | OutLog: {10}" -f $job.Original, $nextAttempt, $maxAttemptsPerFile, $failureReason, $job.EmbeddedHash, $job.PreCalcHash, $postCalcHash, $verification, (Format-ErrSnippet -Text $errText), $job.ErrLog, $job.OutLog)
                        }
                        else {
                            $failed++
                            $processed++
                            $verification = if (-not [string]::IsNullOrWhiteSpace($job.PreCalcHash) -and
                                -not [string]::IsNullOrWhiteSpace($postCalcHash) -and
                                ($job.PreCalcHash -eq $postCalcHash)) {
                                if ($embeddedPresent) { 'MATCH' } else { 'MATCH|NEW' }
                            }
                            else { 'MISMATCH' }
                            $beforeAfter = if ($newSize -gt 0) { ("{0} -> {1}" -f (Format-Bytes $job.OrigSize), (Format-Bytes $newSize)) } else { ("{0} -> N/A" -f (Format-Bytes $job.OrigSize)) }
                            Add-FinalLogEvent -List $finalLogEvents `
                                -EventType 'FAIL' `
                                -File $job.Name `
                                -FullPath $job.Original `
                                -Attempt ("{0}/{1}" -f $job.Attempt, $maxAttemptsPerFile) `
                                -Verification $verification `
                                -EmbeddedHash (Format-HashForLog -Hash $job.EmbeddedHash -NullHash $nullHash) `
                                -CalcPreHash (Format-HashForLog -Hash $job.PreCalcHash -NullHash $nullHash) `
                                -CalcPostHash (Format-HashForLog -Hash $postCalcHash -NullHash $nullHash) `
                                -OrigBytes (Format-Bytes $job.OrigSize) `
                                -NewBytes $(if ($newSize -gt 0) { Format-Bytes $newSize } else { 'N/A' }) `
                                -FailureReason $failureReason
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
                            $failedResults.Add([PSCustomObject]@{
                                    Path         = $job.Original
                                    Name         = $job.Name
                                    Attempt      = ("{0}/{1}" -f $job.Attempt, $maxAttemptsPerFile)
                                    Reason       = $failureReason
                                    Verification = $verification
                                    EmbeddedMd5  = $job.EmbeddedHash
                                    CalcPreMd5   = $job.PreCalcHash
                                    CalcPostMd5  = $postCalcHash
                                    StdErr       = (Format-ErrSnippet -Text $errText)
                                    ErrLog       = $job.ErrLog
                                    OutLog       = $job.OutLog
                                }) | Out-Null
                            Write-RunLog -Level ERROR -Message ("Failed permanently | File: {0} | Attempts: {1}/{2} | Reason: {3} | Embedded: {4} | CalcPre: {5} | CalcPost: {6} | Verification: {7} | STDERR: {8} | ErrLog: {9} | OutLog: {10}" -f $job.Original, $job.Attempt, $maxAttemptsPerFile, $failureReason, $job.EmbeddedHash, $job.PreCalcHash, $postCalcHash, $verification, (Format-ErrSnippet -Text $errText), $job.ErrLog, $job.OutLog)
                        }
                    }

                    $w.Job = $null
                    $uiDirty = $true
                }
            }

            # Assign new job.
            if ($null -eq $w.Job -and $queue.Count -gt 0) {
                $queueItem = $queue.Dequeue()
                $original = $queueItem.Path

                # Temp file must be track.tmp (no ".flac" substring).
                $temp = [System.IO.Path]::ChangeExtension($original, '.tmp')

                $jobId = "{0:D5}_{1}_a{2}" -f $queueItem.FileId, ([guid]::NewGuid().ToString('N').Substring(0, 8)), $queueItem.Attempts
                $errLog = Join-Path -Path $jobLogDir -ChildPath "$jobId.err.log"
                $outLog = Join-Path -Path $jobLogDir -ChildPath "$jobId.out.log"
                $errLogRedirect = Escape-WildcardPath -Path $errLog
                $outLogRedirect = Escape-WildcardPath -Path $outLog

                Safe-RemoveFile -Path $temp
                Safe-RemoveFile -Path $errLog
                Safe-RemoveFile -Path $outLog

                try {
                    # Snapshot source metadata needed for verification and accounting.
                    $origItem = Get-Item -LiteralPath $original -Force
                    $sourceMetadata = Get-FileMetadataSnapshot -Path $original -Item $origItem
                    $embeddedHash = Try-GetFlacMd5 -Path $original
                    if ($null -eq $embeddedHash) { $embeddedHash = $nullHash }
                    $preCalcHash = $null

                    # Build one properly quoted ArgumentList; include "--" to end options.
                    $argString =
                    "-8 -e -p -V -f -o " +
                    (Quote-WinArg $temp) +
                    " -- " +
                    (Quote-WinArg $original)

                    $conversionAttempts++
                    Write-VerboseUi -Message ("Starting flac | File: {0} | Attempt: {1}/{2} | Command: {3} {4}" -f $original, $queueItem.Attempts, $maxAttemptsPerFile, $flacCmd.Source, $argString)
                    $proc = Start-Process -FilePath $flacCmd.Source `
                        -ArgumentList $argString `
                        -NoNewWindow `
                        -PassThru `
                        -RedirectStandardError $errLogRedirect `
                        -RedirectStandardOutput $outLogRedirect

                    try {
                        $proc.PriorityClass = [System.Diagnostics.ProcessPriorityClass]::BelowNormal
                    }
                    catch { }

                    Set-SingleCoreAffinity -Process $proc -CoreIndexZeroBased $w.CoreIdx

                    $w.Job = [PSCustomObject]@{
                        Proc            = $proc
                        Original        = $original
                        Temp            = $temp
                        JobId           = $jobId
                        ErrLog          = $errLog
                        OutLog          = $outLog
                        FileId          = $queueItem.FileId
                        Name            = $queueItem.Name
                        Attempt         = $queueItem.Attempts
                        EmbeddedHash    = $embeddedHash
                        PreCalcHash     = $preCalcHash
                        OrigSize        = $origItem.Length
                        SourceMetadata  = $sourceMetadata
                        Stage           = 'CONVERTING'
                        HashPhase       = ''
                        HashJob         = $null
                        PostCalcHash    = $null
                        ConvertExitCode = $null
                        ErrText         = ''
                        FailureReason   = $null
                        ArtworkResult   = $null
                    }
                    $uiDirty = $true
                }
                catch {
                    Safe-RemoveFile -Path $temp
                    Safe-RemoveFile -Path $errLog
                    Safe-RemoveFile -Path $outLog

                    $failed++
                    $processed++
                    $failureReason = Get-FriendlyPermissionMessage -Operation 'starting conversion' -Path $original -Exception $_.Exception
                    if ([string]::IsNullOrWhiteSpace($failureReason)) {
                        $failureReason = "Could not start conversion | Path: {0} | Detail: {1}" -f $original, $_.Exception.Message
                    }

                    Add-FinalLogEvent -List $finalLogEvents `
                        -EventType 'FAIL' `
                        -File $queueItem.Name `
                        -FullPath $original `
                        -Attempt ("{0}/{1}" -f $queueItem.Attempts, $maxAttemptsPerFile) `
                        -Verification 'MISMATCH' `
                        -EmbeddedHash (Format-HashForLog -Hash $nullHash -NullHash $nullHash) `
                        -CalcPreHash (Format-HashForLog -Hash $null -NullHash $nullHash) `
                        -CalcPostHash (Format-HashForLog -Hash $null -NullHash $nullHash) `
                        -FailureReason $failureReason
                    Push-RecentEvent -List $recentEvents `
                        -Status 'FAIL' `
                        -File $queueItem.Name `
                        -Attempt ("{0}/{1}" -f $queueItem.Attempts, $maxAttemptsPerFile) `
                        -EmbeddedHash (Format-HashForUi -Hash $nullHash -NullHash $nullHash) `
                        -CalculatedBeforeHash (Format-HashForUi -Hash $null -NullHash $nullHash) `
                        -CalculatedAfterHash (Format-HashForUi -Hash $null -NullHash $nullHash) `
                        -Verification 'MISMATCH' `
                        -BeforeAfter 'N/A -> N/A' `
                        -Saved 'N/A' `
                        -CompressionPct 'N/A' `
                        -Detail $failureReason

                    $failedResults.Add([PSCustomObject]@{
                            Path         = $original
                            Name         = $queueItem.Name
                            Attempt      = ("{0}/{1}" -f $queueItem.Attempts, $maxAttemptsPerFile)
                            Reason       = $failureReason
                            Verification = 'MISMATCH'
                            EmbeddedMd5  = $nullHash
                            CalcPreMd5   = $null
                            CalcPostMd5  = $null
                            StdErr       = '(none)'
                            ErrLog       = '(none)'
                            OutLog       = '(none)'
                        }) | Out-Null

                    Write-RunLog -Level ERROR -Message ("Failed before conversion start | File: {0} | Attempt: {1}/{2} | Reason: {3}" -f $original, $queueItem.Attempts, $maxAttemptsPerFile, $failureReason)
                    $uiDirty = $true
                }
            }
        }

        if (-not $interactive) {
            $nowUtc = [DateTime]::UtcNow
            if (($nowUtc - $lastStatusUtc) -ge $statusInterval) {
                $lastStatusUtc = $nowUtc
                $elapsedText = Format-Elapsed -Elapsed ($nowUtc - $runStartedUtc)
                $metadataSavedStatus = $totalMetadataSavedBytes
                $audioSavedStatus = $totalSavedBytes - $metadataSavedStatus
                if ($totalSavedBytes -le 0 -and $audioSavedStatus -lt 0) {
                    $audioSavedStatus = 0L
                }
                Write-Host ("Progress: {0}/{1}  Failed: {2}  Elapsed: {3}  Saved(All): {4}  Audio Delta: {5}  Pad: {6}  Art(Net): {7}  Art(Raw): {8}" -f $processed, $totalFiles, $failed, $elapsedText, (Format-Bytes $totalSavedBytes), (Format-Bytes $audioSavedStatus -Signed), (Format-Bytes $totalPaddingTrimSavedBytes), (Format-Bytes $totalArtworkSavedBytes), (Format-Bytes $totalArtworkRawSavedBytes))
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
    try { Remove-Item -LiteralPath $runtimeCaptureDir -Recurse -Force -ErrorAction SilentlyContinue } catch { }
    Write-RunLog -Level INFO -Message ("Run logs kept at: {0}" -f $runLogDir)
    Flush-RunLog -Force
}

$topCompression = @(
    $compressionResults |
    Where-Object { $_.SavedBytes -gt 0 } |
    Sort-Object -Property SavedBytes -Descending |
    Select-Object -First 3
)

$successful = [Math]::Max(0, ($processed - $failed))
$pending = [Math]::Max(0, ($totalFiles - $processed))
$failedListPath = $null
$efcFinalLogPath = Join-Path -Path $runLogDir -ChildPath ("efc-final_{0}.log" -f $runStamp)
$finishedLocal = Get-Date
$totalElapsed = $finishedLocal.ToUniversalTime() - $runStartedUtc
if ($totalElapsed.Ticks -lt 0) { $totalElapsed = [TimeSpan]::Zero }
$totalElapsedText = Format-Elapsed -Elapsed $totalElapsed
$overallReductionPct = if ($totalOriginalBytes -gt 0) { [Math]::Round((($totalSavedBytes / [double]$totalOriginalBytes) * 100.0), 2) } else { 0.0 }
$successRatePct = if ($totalFiles -gt 0) { [Math]::Round((($successful / [double]$totalFiles) * 100.0), 2) } else { 0.0 }
$avgSavedPerSuccessBytes = if ($successful -gt 0) { [long][Math]::Round(($totalSavedBytes / [double]$successful), 0) } else { 0L }
$metadataNetSavedBytes = $totalMetadataSavedBytes
$audioSavedBytes = $totalSavedBytes - $metadataNetSavedBytes
if ($totalSavedBytes -le 0 -and $audioSavedBytes -lt 0) {
    $audioSavedBytes = 0L
}
$audioReductionPct = if ($totalOriginalBytes -gt 0) { [Math]::Round((($audioSavedBytes / [double]$totalOriginalBytes) * 100.0), 2) } else { 0.0 }

$failedLines = [System.Collections.Generic.List[string]]::new()
$failedLines.Add("Exact Flac Cruncher - Failed Files") | Out-Null
$failedLines.Add(("Album    : {0}" -f $albumName)) | Out-Null
$failedLines.Add(("Finished : {0}" -f (Get-Date -Format o))) | Out-Null
$failedLines.Add(("Failed   : {0}" -f $failedResults.Count)) | Out-Null
$failedLines.Add("===================================================================") | Out-Null
if ($failedResults.Count -gt 0) {
    $failedListPath = Join-Path -Path $runLogDir -ChildPath ("failed-files_{0}.log" -f $runStamp)
    $index = 0
    foreach ($entry in $failedResults) {
        $index++
        $failedLines.Add(("{0}. {1}" -f $index, $entry.Path)) | Out-Null
        $failedLines.Add(("   Attempt      : {0}" -f $entry.Attempt)) | Out-Null
        $failedLines.Add(("   Reason       : {0}" -f $entry.Reason)) | Out-Null
        $failedLines.Add(("   Verification : {0}" -f $entry.Verification)) | Out-Null
        $failedLines.Add(("   Embedded MD5 : {0}" -f $entry.EmbeddedMd5)) | Out-Null
        $failedLines.Add(("   CalcPre MD5  : {0}" -f $entry.CalcPreMd5)) | Out-Null
        $failedLines.Add(("   CalcPost MD5 : {0}" -f $entry.CalcPostMd5)) | Out-Null
        $failedLines.Add(("   STDERR       : {0}" -f $entry.StdErr)) | Out-Null
        $failedLines.Add(("   ErrLog       : {0}" -f $entry.ErrLog)) | Out-Null
        $failedLines.Add(("   OutLog       : {0}" -f $entry.OutLog)) | Out-Null
        $failedLines.Add("") | Out-Null
    }
    [string]::Join([Environment]::NewLine, $failedLines) | Out-File -LiteralPath $failedListPath -Encoding UTF8
}

if ($runCanceled -and $pending -gt 0) {
    Add-FinalLogEvent -List $finalLogEvents `
        -EventType 'CANCELED' `
        -File '(run canceled)' `
        -FullPath $targetDisplay `
        -Attempt '-' `
        -Verification 'N/A' `
        -FailureReason ("Run canceled with {0} pending file(s)" -f $pending)
}

$summaryText = New-EfcFinalLogText `
    -AlbumName $albumName `
    -RootFolder $targetDisplay `
    -RunStartedLocal $runStartedLocal `
    -FinishedLocal $finishedLocal `
    -MaxWorkers $maxWorkers `
    -MaxAttemptsPerFile $maxAttemptsPerFile `
    -TotalFiles $totalFiles `
    -Processed $processed `
    -Successful $successful `
    -Failed $failed `
    -Pending $pending `
    -RunCanceled:$runCanceled `
    -Events $finalLogEvents.ToArray() `
    -TopCompression $topCompression
Flush-RunLog -Force
([Environment]::NewLine + $summaryText) | Out-File -LiteralPath $logFile -Append -Encoding UTF8
$summaryText | Out-File -LiteralPath $efcFinalLogPath -Encoding UTF8

Write-Host ""
if ($runCanceled) {
    Write-Host "JOB CANCELED" -ForegroundColor Yellow
}
else {
    Write-Host "JOB COMPLETE"
}
$processedColor = if ($failed -gt 0) { 'Red' } elseif ($pending -gt 0) { 'Yellow' } else { 'Green' }
$successColor = if ($successful -gt 0 -and $failed -eq 0) { 'Green' } elseif ($successful -gt 0) { 'Yellow' } else { 'Gray' }
$failedColor = if ($failed -gt 0) { 'Red' } else { 'Green' }
$pendingColor = if ($pending -gt 0) { 'Yellow' } else { 'Green' }
$audioDeltaColor = if ($audioSavedBytes -lt 0) { 'Red' } elseif ($audioSavedBytes -gt 0) { 'Cyan' } else { 'Gray' }
$metadataColor = if ($metadataNetSavedBytes -gt 0) { 'DarkGreen' } else { 'Gray' }
$paddingColor = if ($totalPaddingTrimSavedBytes -gt 0 -or $paddingTrimFiles -gt 0) { 'DarkGreen' } else { 'Gray' }
$artworkColor = if ($totalArtworkSavedBytes -gt 0 -or $artworkOptimizedFiles -gt 0) { 'DarkGreen' } else { 'Gray' }

$processedText = ("{0}/{1}" -f $processed, $totalFiles)
Write-SummaryLine -Label 'Processed' -Value $processedText -ValueColor $processedColor
Write-SummaryLine -Label 'Succeeded' -Value ("{0}" -f $successful) -ValueColor $successColor
Write-SummaryLine -Label 'Failed' -Value ("{0}" -f $failed) -ValueColor $failedColor
Write-SummaryLine -Label 'Pending' -Value ("{0}" -f $pending) -ValueColor $pendingColor
Write-SummaryLine -Label 'Elapsed' -Value $totalElapsedText -ValueColor Gray
Write-SummaryLine -Label 'Total Saved' -Value ("{0} | {1} of original" -f (Format-Bytes $totalSavedBytes), (Format-Percent -Value $overallReductionPct)) -ValueColor Green
Write-SummaryLine -Label 'Audio Delta' -Value ("{0} | {1} of original" -f (Format-Bytes $audioSavedBytes -Signed), (Format-Percent -Value $audioReductionPct)) -ValueColor $audioDeltaColor
Write-SummaryLine -Label 'Metadata Net' -Value (Format-Bytes $metadataNetSavedBytes) -ValueColor $metadataColor
Write-SummaryLine -Label 'Padding Trim' -Value (Format-Bytes $totalPaddingTrimSavedBytes) -ValueColor $paddingColor
Write-SummaryLine -Label 'Padding Files' -Value ("{0}" -f $paddingTrimFiles) -ValueColor $paddingColor
Write-SummaryLine -Label 'Artwork Net/Raw' -Value ("{0} / {1}" -f (Format-Bytes $totalArtworkSavedBytes), (Format-Bytes $totalArtworkRawSavedBytes)) -ValueColor $artworkColor
Write-SummaryLine -Label 'Artwork Files/Blk' -Value ("{0}/{1}" -f $artworkOptimizedFiles, $artworkOptimizedBlocks) -ValueColor $artworkColor
Write-SummaryLine -Label 'Success / Avg' -Value ("{0} | {1}" -f (Format-Percent -Value $successRatePct), (Format-Bytes $avgSavedPerSuccessBytes)) -ValueColor Gray
Write-Host "Top 3 Compression:"
if ($topCompression.Count -eq 0) {
    if ($successful -gt 0) {
        Write-Host "  (No net-positive file reductions)"
    }
    else {
        Write-Host "  (No successful file conversions)"
    }
}
else {
    $rank = 0
    foreach ($entry in $topCompression) {
        $rank++
        $entryColor = Get-CompressionColor -CompressionPct ("{0:N2}%" -f $entry.SavedPct)
        Write-Host (Format-TopCompressionLine -Rank $rank -Entry $entry) -ForegroundColor $entryColor
    }
}
Write-Host ""
foreach ($statusLine in (New-EfcStatusReportLines -Successful $successful -Failed $failed -Pending $pending -RunCanceled:$runCanceled)) {
    if ([string]::IsNullOrWhiteSpace($statusLine)) {
        Write-Host ""
        continue
    }

    if ($statusLine -eq 'There were errors') {
        Write-Host $statusLine -ForegroundColor Yellow
        continue
    }

    if ($statusLine -eq 'No errors occurred') {
        Write-Host $statusLine -ForegroundColor Green
        continue
    }

    if ($statusLine -eq 'All files processed successfully') {
        Write-Host $statusLine -ForegroundColor Green
        continue
    }

    if ($statusLine -eq 'Processing canceled by user' -or $statusLine -eq 'Some files could not be verified') {
        Write-Host $statusLine -ForegroundColor Yellow
        continue
    }

    Write-Host $statusLine
}
if ($failedListPath) {
    Write-SummaryLine -Label 'Failed List' -Value $failedListPath -ValueColor Gray
}
else {
    Write-SummaryLine -Label 'Failed List' -Value '(none)' -ValueColor Gray
}
Write-SummaryLine -Label 'EFC Final Log' -Value $efcFinalLogPath -ValueColor Gray
if ($verboseLogFile) {
    Write-SummaryLine -Label 'Verbose Trace' -Value $verboseLogFile -ValueColor Gray
}
else {
    Write-SummaryLine -Label 'Verbose Trace' -Value '(disabled)' -ValueColor Gray
}
Write-SummaryLine -Label 'Logs' -Value $runLogDir -ValueColor Gray
Write-SummaryLine -Label 'Log' -Value $logFile -ValueColor Gray

#endregion Main Orchestration
