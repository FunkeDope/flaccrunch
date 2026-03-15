#!/usr/bin/env pwsh
<#
.SYNOPSIS
    Build script for Exact FLAC Cruncher - runs Pester tests and PSScriptAnalyzer.
.DESCRIPTION
    Installs Pester 5+ if needed, runs all tests in ./Tests, and optionally
    runs PSScriptAnalyzer for lint checks.
.PARAMETER NoBuild
    Skip PSScriptAnalyzer (only run Pester tests).
#>
[CmdletBinding()]
param(
    [switch]$NoBuild
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

# Ensure Pester 5+ is available
$pester = Get-Module -Name Pester -ListAvailable | Where-Object { $_.Version.Major -ge 5 } | Select-Object -First 1
if (-not $pester) {
    Write-Host 'Installing Pester 5+...' -ForegroundColor Yellow
    Install-Module -Name Pester -MinimumVersion 5.0.0 -Scope CurrentUser -Force -SkipPublisherCheck
}

Import-Module Pester -MinimumVersion 5.0 -ErrorAction Stop

# Run Pester tests
Write-Host "`nRunning Pester tests..." -ForegroundColor Cyan
$config = [PesterConfiguration]::Default
$config.Run.Path = Join-Path $PSScriptRoot 'Tests'
$config.Run.Exit = $false
$config.Run.PassThru = $true
$config.Output.Verbosity = 'Detailed'
$config.TestResult.Enabled = $true
$config.TestResult.OutputPath = Join-Path $PSScriptRoot 'TestResults.xml'
$config.TestResult.OutputFormat = 'NUnitXml'

$result = Invoke-Pester -Configuration $config

# PSScriptAnalyzer (optional)
if (-not $NoBuild) {
    $analyzer = Get-Module -Name PSScriptAnalyzer -ListAvailable | Select-Object -First 1
    if ($analyzer) {
        Write-Host "`nRunning PSScriptAnalyzer..." -ForegroundColor Cyan
        $issues = Invoke-ScriptAnalyzer -Path (Join-Path $PSScriptRoot 'Start-ExactFlacCrunch.ps1') -Severity Warning, Error
        if ($issues) {
            Write-Host "`nPSScriptAnalyzer found $($issues.Count) issue(s):" -ForegroundColor Yellow
            $issues | Format-Table -AutoSize
        }
        else {
            Write-Host 'PSScriptAnalyzer: no warnings or errors.' -ForegroundColor Green
        }
    }
    else {
        Write-Host "`nPSScriptAnalyzer not installed, skipping lint. Install with: Install-Module PSScriptAnalyzer" -ForegroundColor DarkGray
    }
}

if ($result.FailedCount -gt 0) {
    Write-Host "`nBuild FAILED: $($result.FailedCount) test(s) failed." -ForegroundColor Red
    exit 1
}

Write-Host "`nBuild PASSED: $($result.PassedCount) test(s) passed." -ForegroundColor Green
exit 0
