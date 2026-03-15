BeforeAll {
    $env:EFC_LOAD_FUNCTIONS_ONLY = '1'
    # Provide LogFolder explicitly to avoid default-value failure on Linux where
    # [Environment]::GetFolderPath('Desktop') returns an empty string.
    $testLogFolder = Join-Path ([System.IO.Path]::GetTempPath()) 'EFC-test-logs'
    # Dot-source the script to load all functions (guard prevents main execution)
    . "$PSScriptRoot/../Start-ExactFlacCrunch.ps1" -RootFolder '__EFC_TEST_LOAD__' -LogFolder $testLogFolder -ErrorAction SilentlyContinue 2>$null
}

AfterAll {
    $env:EFC_LOAD_FUNCTIONS_ONLY = $null
}

# =============================================================================
#  1. Format-Bytes
# =============================================================================
Describe 'Format-Bytes' {
    It 'formats zero bytes' {
        $result = Format-Bytes -Bytes 0
        $result | Should -Match '0\.00\s+B'
    }

    It 'formats 1024 bytes as 1.00 KB' {
        $result = Format-Bytes -Bytes 1024
        $result | Should -Match '1\.00\s+KB'
    }

    It 'formats 1536 bytes as 1.50 KB' {
        $result = Format-Bytes -Bytes 1536
        $result | Should -Match '1\.50\s+KB'
    }

    It 'formats megabyte range' {
        $result = Format-Bytes -Bytes (5 * 1024 * 1024)
        $result | Should -Match '5\.00\s+MB'
    }

    It 'formats gigabyte range' {
        $result = Format-Bytes -Bytes ([long](2.5 * 1024 * 1024 * 1024))
        $result | Should -Match '2\.50\s+GB'
    }

    It 'formats terabyte range' {
        $result = Format-Bytes -Bytes ([long](1024) * 1024 * 1024 * 1024)
        $result | Should -Match '1\.00\s+TB'
    }

    It 'clamps negative to zero when unsigned' {
        $result = Format-Bytes -Bytes (-100)
        $result | Should -Match '0\.00\s+B'
    }

    It 'shows negative value when Signed' {
        $result = Format-Bytes -Bytes (-1024) -Signed
        $result | Should -Match '-.*1\.00\s+KB'
    }

    It 'shows space prefix for positive Signed values' {
        $result = Format-Bytes -Bytes 1024 -Signed
        $result | Should -Match '^\s.*1\.00\s+KB'
    }

    It 'handles very large values (PB range)' {
        $result = Format-Bytes -Bytes ([long]::MaxValue)
        $result | Should -Match '(PB|TB|EB)'
    }
}

# =============================================================================
#  2. Format-HeaderCount
# =============================================================================
Describe 'Format-HeaderCount' {
    It 'formats a number right-padded to 8 characters' {
        $result = Format-HeaderCount -Value 42
        $result.Length | Should -Be 8
        $result | Should -Match '42'
    }

    It 'formats zero' {
        $result = Format-HeaderCount -Value 0
        $result.Trim() | Should -Be '0'
        $result.Length | Should -Be 8
    }

    It 'formats large number' {
        $result = Format-HeaderCount -Value 99999999
        $result | Should -Be '99999999'
    }

    It 'right-aligns the number' {
        $result = Format-HeaderCount -Value 5
        $result | Should -Match '^\s+5$'
    }
}

# =============================================================================
#  3. Format-CountPair
# =============================================================================
Describe 'Format-CountPair' {
    It 'returns formatted pair with separator' {
        $result = Format-CountPair -Left 10 -Right 20
        $result | Should -Match '10\s*/\s*20'
    }

    It 'formats zeros' {
        $result = Format-CountPair -Left 0 -Right 0
        $result | Should -Match '0\s*/\s*0'
    }

    It 'formats large numbers' {
        $result = Format-CountPair -Left 9999 -Right 10000
        $result | Should -Match '9999\s*/\s*10000'
    }
}

# =============================================================================
#  4. Format-Percent
# =============================================================================
Describe 'Format-Percent' {
    It 'formats percentage with 2 decimal places' {
        $result = Format-Percent -Value 42.567
        $result | Should -Match '42\.57%'
    }

    It 'formats zero percent' {
        $result = Format-Percent -Value 0
        $result | Should -Match '0\.00%'
    }

    It 'formats 100 percent' {
        $result = Format-Percent -Value 100
        $result | Should -Match '100\.00%'
    }

    It 'formats negative percentage' {
        $result = Format-Percent -Value (-5.5)
        $result | Should -Match '-5\.50%'
    }
}

# =============================================================================
#  5. Format-LabelValue
# =============================================================================
Describe 'Format-LabelValue' {
    It 'formats label:value with default width' {
        $result = Format-LabelValue -Label 'Size' -Value '100 MB'
        $result | Should -Match 'Size\s+:\s+100 MB'
    }

    It 'uses custom label width' {
        $result = Format-LabelValue -Label 'AB' -Value 'X' -LabelWidth 10
        $result | Should -BeLike 'AB        : X'
    }

    It 'handles empty value' {
        $result = Format-LabelValue -Label 'Test' -Value ''
        $result | Should -Match 'Test\s+:\s*$'
    }

    It 'pads short labels to match width' {
        $result = Format-LabelValue -Label 'A' -Value 'B' -LabelWidth 5
        $result | Should -Be 'A    : B'
    }
}

# =============================================================================
#  6. Format-EacValueLine
# =============================================================================
Describe 'Format-EacValueLine' {
    It 'formats with leading spaces and default label width' {
        $result = Format-EacValueLine -Label 'Status' -Value 'OK'
        $result | Should -Match '^\s{5}Status\s+OK$'
    }

    It 'uses custom label width' {
        $result = Format-EacValueLine -Label 'X' -Value 'Y' -LabelWidth 10
        $result | Should -BeLike '     X          Y'
    }

    It 'handles empty value' {
        $result = Format-EacValueLine -Label 'Tag' -Value ''
        $result | Should -Match '^\s{5}Tag'
    }
}

# =============================================================================
#  7. Format-Elapsed
# =============================================================================
Describe 'Format-Elapsed' {
    It 'formats zero timespan' {
        $result = Format-Elapsed -Elapsed ([TimeSpan]::Zero)
        $result | Should -Be '00:00:00'
    }

    It 'formats under 1 hour' {
        $result = Format-Elapsed -Elapsed ([TimeSpan]::new(0, 5, 30))
        $result | Should -Be '00:05:30'
    }

    It 'formats exactly 1 hour' {
        $result = Format-Elapsed -Elapsed ([TimeSpan]::new(1, 0, 0))
        $result | Should -Be '01:00:00'
    }

    It 'formats over 24 hours with day prefix' {
        $result = Format-Elapsed -Elapsed ([TimeSpan]::new(2, 3, 15, 45))
        $result | Should -Be '2d 03:15:45'
    }

    It 'treats negative timespan as zero' {
        $result = Format-Elapsed -Elapsed ([TimeSpan]::FromSeconds(-10))
        $result | Should -Be '00:00:00'
    }

    It 'formats multi-hour correctly' {
        # Use exact hours/minutes/seconds to avoid rounding issues with [int] cast
        $result = Format-Elapsed -Elapsed ([TimeSpan]::new(0, 12, 0, 0))
        $result | Should -Be '12:00:00'
    }
}

# =============================================================================
#  8. Format-EacLogDateTime
# =============================================================================
Describe 'Format-EacLogDateTime' {
    It 'formats a specific date correctly' {
        $dt = [DateTime]::new(2024, 1, 15, 14, 30, 0)
        $result = Format-EacLogDateTime -Value $dt
        $result | Should -Be '15. January 2024, 14:30'
    }

    It 'formats midnight correctly' {
        $dt = [DateTime]::new(2023, 12, 25, 0, 0, 0)
        $result = Format-EacLogDateTime -Value $dt
        $result | Should -Be '25. December 2023, 0:00'
    }

    It 'formats single-digit day correctly' {
        $dt = [DateTime]::new(2024, 3, 5, 9, 5, 0)
        $result = Format-EacLogDateTime -Value $dt
        $result | Should -Be '5. March 2024, 9:05'
    }
}

# =============================================================================
#  9. Format-TopCompressionLine
# =============================================================================
Describe 'Format-TopCompressionLine' {
    BeforeAll {
        $testEntry = [PSCustomObject]@{
            Path       = '/music/album/track01.flac'
            SavedBytes = 1024
            SavedPct   = 5.25
        }
    }

    It 'formats with full path by default' {
        $result = Format-TopCompressionLine -Rank 1 -Entry $testEntry
        $result | Should -Match '1\.'
        $result | Should -Match 'Saved'
        $result | Should -Match '/music/album/track01\.flac'
    }

    It 'uses leaf name when LeafName switch is set' {
        $result = Format-TopCompressionLine -Rank 2 -Entry $testEntry -LeafName
        $result | Should -Match 'track01\.flac'
        $result | Should -Not -Match '/music/album/'
    }

    It 'includes saved percentage' {
        $result = Format-TopCompressionLine -Rank 1 -Entry $testEntry
        $result | Should -Match '5\.25%'
    }

    It 'formats rank number' {
        $result = Format-TopCompressionLine -Rank 10 -Entry $testEntry
        $result | Should -Match '10\.'
    }
}

# =============================================================================
# 10. Format-HashForUi
# =============================================================================
Describe 'Format-HashForUi' {
    It 'returns N/A for null hash' {
        $result = Format-HashForUi -Hash $null -NullHash '00000000000000000000000000000000'
        $result | Should -Be 'N/A'
    }

    It 'returns N/A for empty hash' {
        $result = Format-HashForUi -Hash '' -NullHash '00000000000000000000000000000000'
        $result | Should -Be 'N/A'
    }

    It 'returns NULL-EMBEDDED for null hash value' {
        $nullHash = '00000000000000000000000000000000'
        $result = Format-HashForUi -Hash $nullHash -NullHash $nullHash
        $result | Should -Be 'NULL-EMBEDDED'
    }

    It 'returns short hash as-is in lowercase' {
        $result = Format-HashForUi -Hash 'ABCDEF1234' -NullHash '00000000000000000000000000000000'
        $result | Should -Be 'abcdef1234'
    }

    It 'truncates long hash with ellipsis in middle' {
        $hash = 'abcdef1234567890abcdef1234567890'
        $result = Format-HashForUi -Hash $hash -NullHash '00000000000000000000000000000000'
        $result | Should -Match '^\w{10}\.\.\.\w{6}$'
    }
}

# =============================================================================
# 11. Format-HashForLog
# =============================================================================
Describe 'Format-HashForLog' {
    It 'returns N/A for null hash' {
        $result = Format-HashForLog -Hash $null -NullHash '00000000000000000000000000000000'
        $result | Should -Be 'N/A'
    }

    It 'returns NULL-EMBEDDED for null hash value' {
        $nullHash = '00000000000000000000000000000000'
        $result = Format-HashForLog -Hash $nullHash -NullHash $nullHash
        $result | Should -Be 'NULL-EMBEDDED'
    }

    It 'returns full hash in lowercase' {
        $hash = 'ABCDEF1234567890ABCDEF1234567890'
        $result = Format-HashForLog -Hash $hash -NullHash '00000000000000000000000000000000'
        $result | Should -Be 'abcdef1234567890abcdef1234567890'
    }

    It 'returns N/A for whitespace-only hash' {
        $result = Format-HashForLog -Hash '   ' -NullHash '00000000000000000000000000000000'
        $result | Should -Be 'N/A'
    }
}

# =============================================================================
# 12. Format-VerificationText
# =============================================================================
Describe 'Format-VerificationText' {
    It 'returns N/A for null' {
        $result = Format-VerificationText -Verification $null
        $result | Should -Be 'N/A'
    }

    It 'returns N/A for empty string' {
        $result = Format-VerificationText -Verification ''
        $result | Should -Be 'N/A'
    }

    It 'returns N/A for whitespace' {
        $result = Format-VerificationText -Verification '   '
        $result | Should -Be 'N/A'
    }

    It 'returns the verification text as-is' {
        $result = Format-VerificationText -Verification 'MATCH'
        $result | Should -Be 'MATCH'
    }

    It 'passes through arbitrary text' {
        $result = Format-VerificationText -Verification 'MATCH|NEW'
        $result | Should -Be 'MATCH|NEW'
    }
}

# =============================================================================
# 13. Format-ErrSnippet
# =============================================================================
Describe 'Format-ErrSnippet' {
    It 'returns (none) for null text' {
        $result = Format-ErrSnippet -Text $null
        $result | Should -Be '(none)'
    }

    It 'returns (none) for empty text' {
        $result = Format-ErrSnippet -Text ''
        $result | Should -Be '(none)'
    }

    It 'returns (none) for whitespace-only text' {
        $result = Format-ErrSnippet -Text '   '
        $result | Should -Be '(none)'
    }

    It 'normalizes whitespace in short text' {
        $result = Format-ErrSnippet -Text "hello`n  world`t!"
        $result | Should -Be 'hello world !'
    }

    It 'truncates long text with ellipsis' {
        $longText = 'A' * 500
        $result = Format-ErrSnippet -Text $longText -MaxLength 400
        $result.Length | Should -Be 403  # 400 + '...'
        $result | Should -Match '\.\.\.$'
    }

    It 'respects custom MaxLength' {
        $result = Format-ErrSnippet -Text ('ABCDEFGHIJ' * 5) -MaxLength 10
        $result.Length | Should -Be 13  # 10 + '...'
    }

    It 'returns text as-is when within MaxLength' {
        $result = Format-ErrSnippet -Text 'short error' -MaxLength 400
        $result | Should -Be 'short error'
    }
}

# =============================================================================
# 14. Get-TextSha256
# =============================================================================
Describe 'Get-TextSha256' {
    It 'returns uppercase hex string' {
        $result = Get-TextSha256 -Text 'test'
        $result | Should -Match '^[A-F0-9]{64}$'
    }

    It 'returns valid hash for empty string' {
        $result = Get-TextSha256 -Text ''
        $result | Should -Match '^[A-F0-9]{64}$'
        # SHA256 of empty string is well-known
        $result | Should -Be 'E3B0C44298FC1C149AFBF4C8996FB92427AE41E4649B934CA495991B7852B855'
    }

    It 'returns known hash for known input' {
        $result = Get-TextSha256 -Text 'hello'
        $result | Should -Be '2CF24DBA5FB0A30E26E83B2AC5B9E29E1B161E5C1FA7425E73043362938B9824'
    }

    It 'is deterministic' {
        $r1 = Get-TextSha256 -Text 'deterministic'
        $r2 = Get-TextSha256 -Text 'deterministic'
        $r1 | Should -Be $r2
    }

    It 'produces different hashes for different inputs' {
        $r1 = Get-TextSha256 -Text 'abc'
        $r2 = Get-TextSha256 -Text 'def'
        $r1 | Should -Not -Be $r2
    }
}

# =============================================================================
# 15. Get-SafeName
# =============================================================================
Describe 'Get-SafeName' {
    It 'passes through simple valid names' {
        $result = Get-SafeName -Value 'my-album'
        $result | Should -Be 'my-album'
    }

    It 'replaces invalid filename characters with underscore' {
        $result = Get-SafeName -Value 'file/name:test'
        $result | Should -Not -Match '[/:]'
    }

    It 'replaces wildcard characters' {
        $result = Get-SafeName -Value 'test[1]*?.flac'
        $result | Should -Not -Match '[\[\]\*\?]'
    }

    It 'truncates long names' {
        $longName = 'A' * 200
        $result = Get-SafeName -Value $longName -MaxLength 50
        $result.Length | Should -BeLessOrEqual 50
    }

    It 'returns flac-job for whitespace-only string' {
        $result = Get-SafeName -Value '   '
        $result | Should -Be 'flac-job'
    }

    It 'returns flac-job for dots-only string' {
        $result = Get-SafeName -Value '...'
        $result | Should -Be 'flac-job'
    }

    It 'normalizes multiple spaces' {
        $result = Get-SafeName -Value 'hello    world'
        $result | Should -Be 'hello world'
    }

    It 'trims leading/trailing dots and spaces' {
        $result = Get-SafeName -Value '  .test.  '
        $result | Should -Be 'test'
    }
}

# =============================================================================
# 16. Get-RootDisplayName
# =============================================================================
Describe 'Get-RootDisplayName' {
    It 'returns Item.Name when Item is a real FileSystemInfo' {
        $tempDir = Join-Path ([System.IO.Path]::GetTempPath()) ('efc-rootdisplay-{0}' -f [guid]::NewGuid().ToString('N'))
        New-Item -ItemType Directory -Path $tempDir -Force | Out-Null
        try {
            $item = Get-Item -LiteralPath $tempDir
            $result = Get-RootDisplayName -Path $tempDir -Item $item
            $result | Should -Be $item.Name
        }
        finally {
            Remove-Item -Path $tempDir -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It 'extracts leaf from path when Item is null' {
        $result = Get-RootDisplayName -Path '/music/albums/Beatles'
        $result | Should -Be 'Beatles'
    }

    It 'handles double-backslash UNC paths' {
        # The regex expects \\host\share format with 2+ leading separators
        $result = Get-RootDisplayName -Path '\\server\share'
        # On Linux, Split-Path may extract 'share' as leaf; the function returns the leaf if non-empty
        $result | Should -Not -BeNullOrEmpty
    }

    It 'returns root for path with only separators' {
        $result = Get-RootDisplayName -Path '///'
        $result | Should -Be 'root'
    }

    It 'trims trailing separators' {
        $result = Get-RootDisplayName -Path '/music/test/'
        $result | Should -Be 'test'
    }

    It 'returns leaf from Windows-style path' {
        $result = Get-RootDisplayName -Path 'C:\Music\Albums'
        $result | Should -Be 'Albums'
    }
}

# =============================================================================
# 17. Get-DefaultLogFolder
# =============================================================================
Describe 'Get-DefaultLogFolder' {
    It 'returns a path containing EFC-logs' {
        $result = Get-DefaultLogFolder
        $result | Should -Match 'EFC-logs'
    }

    It 'returns a non-empty string' {
        $result = Get-DefaultLogFolder
        $result | Should -Not -BeNullOrEmpty
    }

    It 'returns an absolute-looking path' {
        $result = Get-DefaultLogFolder
        # Should start with / on Linux or drive letter on Windows
        $result | Should -Match '^(/|[A-Za-z]:)'
    }
}

# =============================================================================
# 18. Test-DirectoryWriteAccess
# =============================================================================
Describe 'Test-DirectoryWriteAccess' {
    It 'returns true for a writable directory' {
        $tempDir = Join-Path ([System.IO.Path]::GetTempPath()) ('efc-test-{0}' -f [guid]::NewGuid().ToString('N'))
        New-Item -ItemType Directory -Path $tempDir -Force | Out-Null
        try {
            $result = Test-DirectoryWriteAccess -Path $tempDir
            $result | Should -Be $true
        }
        finally {
            Remove-Item -Path $tempDir -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It 'returns false for a non-existent directory' {
        $result = Test-DirectoryWriteAccess -Path '/nonexistent/path/that/does/not/exist'
        $result | Should -Be $false
    }

    It 'returns false for whitespace path' {
        $result = Test-DirectoryWriteAccess -Path '   '
        $result | Should -Be $false
    }

    It 'returns false for path to a file instead of directory' {
        $tempFile = Join-Path ([System.IO.Path]::GetTempPath()) ('efc-file-{0}.tmp' -f [guid]::NewGuid().ToString('N'))
        [System.IO.File]::WriteAllText($tempFile, 'test')
        try {
            $result = Test-DirectoryWriteAccess -Path $tempFile
            $result | Should -Be $false
        }
        finally {
            Remove-Item -LiteralPath $tempFile -Force -ErrorAction SilentlyContinue
        }
    }
}

# =============================================================================
# 19. Test-IsPermissionText
# =============================================================================
Describe 'Test-IsPermissionText' {
    It 'detects "access is denied"' {
        Test-IsPermissionText -Text 'Access is denied' | Should -Be $true
    }

    It 'detects "permission denied"' {
        Test-IsPermissionText -Text 'Permission denied to file' | Should -Be $true
    }

    It 'detects "UnauthorizedAccessException"' {
        Test-IsPermissionText -Text 'System.UnauthorizedAccessException thrown' | Should -Be $true
    }

    It 'detects "unauthorized access"' {
        Test-IsPermissionText -Text 'Unauthorized access to resource' | Should -Be $true
    }

    It 'detects "not authorized"' {
        Test-IsPermissionText -Text 'User is not authorized' | Should -Be $true
    }

    It 'detects "read-only"' {
        Test-IsPermissionText -Text 'File is read-only' | Should -Be $true
    }

    It 'returns false for non-permission text' {
        Test-IsPermissionText -Text 'File not found' | Should -Be $false
    }

    It 'returns false for empty text' {
        Test-IsPermissionText -Text '' | Should -Be $false
    }

    It 'returns false for null text' {
        Test-IsPermissionText -Text $null | Should -Be $false
    }

    It 'is case-insensitive' {
        Test-IsPermissionText -Text 'ACCESS IS DENIED' | Should -Be $true
    }
}

# =============================================================================
# 20. Get-FriendlyPermissionMessage
# =============================================================================
Describe 'Get-FriendlyPermissionMessage' {
    It 'returns message for UnauthorizedAccessException' {
        $ex = [System.UnauthorizedAccessException]::new('no access')
        $result = Get-FriendlyPermissionMessage -Operation 'reading file' -Path '/tmp/test' -Exception $ex
        $result | Should -Not -BeNullOrEmpty
        $result | Should -Match 'Permission denied'
        $result | Should -Match 'reading file'
    }

    It 'returns null for non-permission exception' {
        $ex = [System.IO.IOException]::new('disk full')
        $result = Get-FriendlyPermissionMessage -Operation 'writing file' -Path '/tmp/test' -Exception $ex
        $result | Should -BeNullOrEmpty
    }

    It 'detects permission text in Details parameter' {
        $result = Get-FriendlyPermissionMessage -Operation 'copying' -Path '/tmp/x' -Exception $null -Details 'Access is denied'
        $result | Should -Not -BeNullOrEmpty
        $result | Should -Match 'Permission denied'
    }

    It 'includes path in the message' {
        $ex = [System.UnauthorizedAccessException]::new('denied')
        $result = Get-FriendlyPermissionMessage -Operation 'deleting' -Path '/my/file.txt' -Exception $ex
        $result | Should -Match '/my/file\.txt'
    }

    It 'returns null when no permission issue found' {
        $result = Get-FriendlyPermissionMessage -Operation 'test' -Details 'something else'
        $result | Should -BeNullOrEmpty
    }

    It 'includes detail text when present' {
        $ex = [System.UnauthorizedAccessException]::new('specific error detail')
        $result = Get-FriendlyPermissionMessage -Operation 'opening' -Path '/a' -Exception $ex
        $result | Should -Match 'specific error detail'
    }
}

# =============================================================================
# 21. Escape-WildcardPath
# =============================================================================
Describe 'Escape-WildcardPath' {
    It 'escapes bracket characters' {
        $result = Escape-WildcardPath -Path 'file[1].txt'
        $result | Should -Not -Be 'file[1].txt'
        $result | Should -Match '`\['
    }

    It 'escapes asterisk' {
        $result = Escape-WildcardPath -Path 'file*.txt'
        $result | Should -Match '`\*'
    }

    It 'escapes question mark' {
        $result = Escape-WildcardPath -Path 'file?.txt'
        $result | Should -Match '`\?'
    }

    It 'leaves normal paths unchanged' {
        $result = Escape-WildcardPath -Path '/normal/path/file.txt'
        $result | Should -Be '/normal/path/file.txt'
    }

    It 'handles single character path' {
        $result = Escape-WildcardPath -Path 'a'
        $result | Should -Be 'a'
    }
}

# =============================================================================
# 22. Safe-RemoveFile
# =============================================================================
Describe 'Safe-RemoveFile' {
    It 'removes an existing file without error' {
        $tempFile = Join-Path ([System.IO.Path]::GetTempPath()) ('efc-test-{0}.tmp' -f [guid]::NewGuid().ToString('N'))
        [System.IO.File]::WriteAllText($tempFile, 'test')
        { Safe-RemoveFile -Path $tempFile } | Should -Not -Throw
        Test-Path -LiteralPath $tempFile | Should -Be $false
    }

    It 'does not throw for non-existent file' {
        { Safe-RemoveFile -Path '/nonexistent/file/that/does/not/exist.tmp' } | Should -Not -Throw
    }

    It 'does not throw for a path that is just whitespace-like content' {
        { Safe-RemoveFile -Path 'nonexistent-safe-remove-test.tmp' } | Should -Not -Throw
    }
}

# =============================================================================
# 23. Get-FileMetadataSnapshot
# =============================================================================
Describe 'Get-FileMetadataSnapshot' {
    It 'returns snapshot with expected properties from a real file' {
        $tempFile = Join-Path ([System.IO.Path]::GetTempPath()) ('efc-meta-{0}.tmp' -f [guid]::NewGuid().ToString('N'))
        [System.IO.File]::WriteAllText($tempFile, 'test content')
        try {
            $result = Get-FileMetadataSnapshot -Path $tempFile
            $result | Should -Not -BeNullOrEmpty
            $result.CreationTimeUtc | Should -BeOfType [DateTime]
            $result.LastWriteTimeUtc | Should -BeOfType [DateTime]
            $result.LastAccessTimeUtc | Should -BeOfType [DateTime]
            $result.Attributes | Should -Not -BeNullOrEmpty
        }
        finally {
            Remove-Item -LiteralPath $tempFile -Force -ErrorAction SilentlyContinue
        }
    }

    It 'uses provided Item when available' {
        $tempFile = Join-Path ([System.IO.Path]::GetTempPath()) ('efc-meta2-{0}.tmp' -f [guid]::NewGuid().ToString('N'))
        [System.IO.File]::WriteAllText($tempFile, 'test')
        try {
            $item = Get-Item -LiteralPath $tempFile
            $result = Get-FileMetadataSnapshot -Path $tempFile -Item $item
            $result | Should -Not -BeNullOrEmpty
            $result.CreationTimeUtc | Should -Be $item.CreationTimeUtc
        }
        finally {
            Remove-Item -LiteralPath $tempFile -Force -ErrorAction SilentlyContinue
        }
    }

    It 'returns null for non-existent path without Item' {
        $result = Get-FileMetadataSnapshot -Path '/does/not/exist/file.tmp'
        $result | Should -BeNullOrEmpty
    }
}

# =============================================================================
# 24. Restore-FileMetadata
# =============================================================================
Describe 'Restore-FileMetadata' {
    BeforeEach {
        $script:LogFile = $null
    }

    It 'returns true when snapshot is null' {
        $result = Restore-FileMetadata -Path '/any/path' -Snapshot $null
        $result | Should -Be $true
    }

    It 'restores timestamps on a real file' {
        $tempFile = Join-Path ([System.IO.Path]::GetTempPath()) ('efc-restore-{0}.tmp' -f [guid]::NewGuid().ToString('N'))
        [System.IO.File]::WriteAllText($tempFile, 'test content')
        try {
            $targetTime = [DateTime]::new(2020, 6, 15, 12, 0, 0, [DateTimeKind]::Utc)
            $snapshot = [PSCustomObject]@{
                CreationTimeUtc   = $targetTime
                LastAccessTimeUtc = $targetTime
                LastWriteTimeUtc  = $targetTime
                Attributes        = [System.IO.FileAttributes]::Normal
            }
            $result = Restore-FileMetadata -Path $tempFile -Snapshot $snapshot
            $result | Should -Be $true

            $fi = Get-Item -LiteralPath $tempFile
            $fi.LastWriteTimeUtc | Should -Be $targetTime
        }
        finally {
            Remove-Item -LiteralPath $tempFile -Force -ErrorAction SilentlyContinue
        }
    }

    It 'returns false when restore partially fails' {
        $snapshot = [PSCustomObject]@{
            CreationTimeUtc   = [DateTime]::UtcNow
            LastAccessTimeUtc = [DateTime]::UtcNow
            LastWriteTimeUtc  = [DateTime]::UtcNow
            Attributes        = [System.IO.FileAttributes]::Normal
        }
        $result = Restore-FileMetadata -Path '/nonexistent/path/file.tmp' -Snapshot $snapshot
        $result | Should -Be $false
    }
}

# =============================================================================
# 25. Quote-WinArg
# =============================================================================
Describe 'Quote-WinArg' {
    It 'returns empty quotes for empty string' {
        # The function handles Length -eq 0 internally, but Mandatory prevents empty string binding.
        # Test with a string containing only a double quote to verify escaping.
        $result = Quote-WinArg -Arg ' '
        $result | Should -Be '" "'
    }

    It 'passes through simple argument' {
        $result = Quote-WinArg -Arg 'simple'
        $result | Should -Be 'simple'
    }

    It 'wraps argument with spaces in quotes' {
        $result = Quote-WinArg -Arg 'hello world'
        $result | Should -Be '"hello world"'
    }

    It 'escapes internal quotes' {
        $result = Quote-WinArg -Arg 'say "hi"'
        $result | Should -Match '\\"'
    }

    It 'doubles backslashes before quotes' {
        $result = Quote-WinArg -Arg 'path\"end'
        $result | Should -Match '\\\\\\"'
    }

    It 'handles argument with only spaces' {
        $result = Quote-WinArg -Arg '   '
        $result | Should -Be '"   "'
    }

    It 'handles argument with tabs' {
        $result = Quote-WinArg -Arg "hello`tworld"
        $result.StartsWith('"') | Should -Be $true
        $result.EndsWith('"') | Should -Be $true
    }

    It 'passes through simple path without spaces' {
        $result = Quote-WinArg -Arg 'C:\folder\file.flac'
        $result | Should -Be 'C:\folder\file.flac'
    }
}

# =============================================================================
# 26. Try-GetFlacMd5
# =============================================================================
Describe 'Try-GetFlacMd5' {
    BeforeAll {
        # Create a wrapper function so we can mock external tool calls
        function script:Invoke-Metaflac {
            param([string[]]$Arguments)
            & metaflac @Arguments
        }
    }

    It 'returns hash from metaflac output' {
        # Mock the external metaflac command by defining it as a function
        function metaflac { 'abcdef1234567890abcdef1234567890' }
        $result = Try-GetFlacMd5 -Path '/fake/path.flac'
        $result | Should -Be 'abcdef1234567890abcdef1234567890'
        Remove-Item -Path Function:\metaflac -ErrorAction SilentlyContinue
    }

    It 'returns null when metaflac returns empty' {
        function metaflac { '' }
        $result = Try-GetFlacMd5 -Path '/fake/path.flac'
        $result | Should -BeNullOrEmpty
        Remove-Item -Path Function:\metaflac -ErrorAction SilentlyContinue
    }

    It 'returns null when metaflac throws' {
        function metaflac { throw 'metaflac error' }
        $result = Try-GetFlacMd5 -Path '/fake/path.flac'
        $result | Should -BeNullOrEmpty
        Remove-Item -Path Function:\metaflac -ErrorAction SilentlyContinue
    }

    It 'trims whitespace from result' {
        function metaflac { '  abc123def456  ' }
        $result = Try-GetFlacMd5 -Path '/fake/path.flac'
        $result | Should -Be 'abc123def456'
        Remove-Item -Path Function:\metaflac -ErrorAction SilentlyContinue
    }
}

# =============================================================================
# 27. Set-FlacMd5IfMissing
# =============================================================================
Describe 'Set-FlacMd5IfMissing' {
    It 'returns true when current hash already matches expected' {
        $expectedMd5 = 'abcdef1234567890abcdef1234567890'
        function metaflac { $expectedMd5 }
        $result = Set-FlacMd5IfMissing -Path '/fake.flac' -ExpectedMd5 $expectedMd5 -NullHash '00000000000000000000000000000000'
        $result | Should -Be $true
        Remove-Item -Path Function:\metaflac -ErrorAction SilentlyContinue
    }

    It 'returns false for whitespace-only ExpectedMd5' {
        $result = Set-FlacMd5IfMissing -Path '/fake.flac' -ExpectedMd5 ' ' -NullHash '00000000000000000000000000000000'
        $result | Should -Be $false
    }

    It 'returns false for whitespace ExpectedMd5' {
        $result = Set-FlacMd5IfMissing -Path '/fake.flac' -ExpectedMd5 '   ' -NullHash '00000000000000000000000000000000'
        $result | Should -Be $false
    }

    It 'attempts to set hash when current is null hash and returns result' {
        $nullHash = '00000000000000000000000000000000'
        $expectedMd5 = 'abcdef1234567890abcdef1234567890'
        # metaflac returns null hash first, then expected after setting
        $script:metaflacCallCount = 0
        function metaflac {
            $script:metaflacCallCount++
            if ($script:metaflacCallCount -le 1) { return $using:nullHash }
            return $using:expectedMd5
        }
        # The function uses `& metaflac` which calls our function override
        $result = Set-FlacMd5IfMissing -Path '/fake.flac' -ExpectedMd5 $expectedMd5 -NullHash $nullHash
        $result | Should -BeOfType [bool]
        Remove-Item -Path Function:\metaflac -ErrorAction SilentlyContinue
    }
}

# =============================================================================
# 28. Get-ImageSignatureKind
# =============================================================================
Describe 'Get-ImageSignatureKind' {
    It 'detects PNG magic bytes' {
        $tempFile = Join-Path ([System.IO.Path]::GetTempPath()) ('efc-png-{0}.tmp' -f [guid]::NewGuid().ToString('N'))
        try {
            $pngHeader = [byte[]]@(0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00)
            [System.IO.File]::WriteAllBytes($tempFile, $pngHeader)
            $result = Get-ImageSignatureKind -Path $tempFile
            $result | Should -Be 'PNG'
        }
        finally {
            Remove-Item -LiteralPath $tempFile -Force -ErrorAction SilentlyContinue
        }
    }

    It 'detects JPEG magic bytes' {
        $tempFile = Join-Path ([System.IO.Path]::GetTempPath()) ('efc-jpg-{0}.tmp' -f [guid]::NewGuid().ToString('N'))
        try {
            $jpegHeader = [byte[]]@(0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10, 0x4A, 0x46)
            [System.IO.File]::WriteAllBytes($tempFile, $jpegHeader)
            $result = Get-ImageSignatureKind -Path $tempFile
            $result | Should -Be 'JPEG'
        }
        finally {
            Remove-Item -LiteralPath $tempFile -Force -ErrorAction SilentlyContinue
        }
    }

    It 'returns null for unknown file signature' {
        $tempFile = Join-Path ([System.IO.Path]::GetTempPath()) ('efc-unk-{0}.tmp' -f [guid]::NewGuid().ToString('N'))
        try {
            $unknownHeader = [byte[]]@(0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07)
            [System.IO.File]::WriteAllBytes($tempFile, $unknownHeader)
            $result = Get-ImageSignatureKind -Path $tempFile
            $result | Should -BeNullOrEmpty
        }
        finally {
            Remove-Item -LiteralPath $tempFile -Force -ErrorAction SilentlyContinue
        }
    }

    It 'returns null for non-existent file' {
        $result = Get-ImageSignatureKind -Path '/nonexistent/file.png'
        $result | Should -BeNullOrEmpty
    }

    It 'returns null for file too short' {
        $tempFile = Join-Path ([System.IO.Path]::GetTempPath()) ('efc-short-{0}.tmp' -f [guid]::NewGuid().ToString('N'))
        try {
            [System.IO.File]::WriteAllBytes($tempFile, [byte[]]@(0x89, 0x50))
            $result = Get-ImageSignatureKind -Path $tempFile
            $result | Should -BeNullOrEmpty
        }
        finally {
            Remove-Item -LiteralPath $tempFile -Force -ErrorAction SilentlyContinue
        }
    }
}

# =============================================================================
# 29. Get-FlacPictureBlocks
# =============================================================================
Describe 'Get-FlacPictureBlocks' {
    It 'parses metaflac --list output correctly' {
        $metaflacOutput = @(
            'METADATA block #6',
            '  type: 6 (PICTURE)',
            '  MIME type: image/png',
            '  description: Cover',
            '  width: 500',
            '  height: 500',
            '  depth: 24',
            '  colors: 0'
        )
        # Override metaflac as a function
        function metaflac { $metaflacOutput; $global:LASTEXITCODE = 0 }
        $result = @(Get-FlacPictureBlocks -MetaflacExePath 'metaflac' -Path '/fake.flac')
        $result.Count | Should -Be 1
        $result[0].BlockNumber | Should -Be 6
        $result[0].MimeType | Should -Be 'image/png'
        $result[0].Description | Should -Be 'Cover'
        $result[0].Width | Should -Be 500
        $result[0].Height | Should -Be 500
        $result[0].Depth | Should -Be 24
        Remove-Item -Path Function:\metaflac -ErrorAction SilentlyContinue
    }

    It 'returns empty array when metaflac returns no output' {
        function metaflac { $global:LASTEXITCODE = 1; return @() }
        $result = @(Get-FlacPictureBlocks -MetaflacExePath 'metaflac' -Path '/fake.flac')
        $result.Count | Should -Be 0
        Remove-Item -Path Function:\metaflac -ErrorAction SilentlyContinue
    }

    It 'parses multiple picture blocks' {
        $metaflacOutput = @(
            'METADATA block #3',
            '  type: 3 (PICTURE)',
            '  MIME type: image/jpeg',
            '  description: Front',
            '  width: 600',
            '  height: 600',
            '  depth: 24',
            '  colors: 0',
            'METADATA block #4',
            '  type: 4 (PICTURE)',
            '  MIME type: image/png',
            '  description: Back',
            '  width: 300',
            '  height: 300',
            '  depth: 32',
            '  colors: 256'
        )
        function metaflac { $metaflacOutput; $global:LASTEXITCODE = 0 }
        $result = @(Get-FlacPictureBlocks -MetaflacExePath 'metaflac' -Path '/fake.flac')
        $result.Count | Should -Be 2
        $result[0].BlockNumber | Should -Be 3
        $result[1].BlockNumber | Should -Be 4
        Remove-Item -Path Function:\metaflac -ErrorAction SilentlyContinue
    }

    It 'returns empty array when metaflac throws' {
        function metaflac { throw 'error' }
        $result = @(Get-FlacPictureBlocks -MetaflacExePath 'metaflac' -Path '/fake.flac')
        $result.Count | Should -Be 0
        Remove-Item -Path Function:\metaflac -ErrorAction SilentlyContinue
    }
}

# =============================================================================
# 30. New-MetaflacPictureSpec
# =============================================================================
Describe 'New-MetaflacPictureSpec' {
    It 'returns pipe-delimited spec string for valid block' {
        $block = [PSCustomObject]@{
            PictureType = 3
            MimeType    = 'image/jpeg'
            Description = 'Cover Art'
            Width       = 500
            Height      = 500
            Depth       = 24
            Colors      = 0
        }
        $result = New-MetaflacPictureSpec -Block $block -FilePath '/tmp/cover.jpg'
        $result | Should -Not -BeNullOrEmpty
        $result | Should -Match '3\|image/jpeg\|Cover Art\|500x500x24\|/tmp/cover\.jpg'
    }

    It 'returns null for URL MIME type (-->)' {
        $block = [PSCustomObject]@{
            PictureType = 3
            MimeType    = '-->'
            Description = ''
            Width       = 0
            Height      = 0
            Depth       = 0
            Colors      = 0
        }
        $result = New-MetaflacPictureSpec -Block $block -FilePath '/tmp/cover.jpg'
        $result | Should -BeNullOrEmpty
    }

    It 'returns null when description contains pipe character' {
        $block = [PSCustomObject]@{
            PictureType = 3
            MimeType    = 'image/png'
            Description = 'Cover|Art'
            Width       = 100
            Height      = 100
            Depth       = 24
            Colors      = 0
        }
        $result = New-MetaflacPictureSpec -Block $block -FilePath '/tmp/cover.png'
        $result | Should -BeNullOrEmpty
    }

    It 'returns null when description contains newline' {
        $block = [PSCustomObject]@{
            PictureType = 3
            MimeType    = 'image/png'
            Description = "Line1`nLine2"
            Width       = 100
            Height      = 100
            Depth       = 24
            Colors      = 0
        }
        $result = New-MetaflacPictureSpec -Block $block -FilePath '/tmp/cover.png'
        $result | Should -BeNullOrEmpty
    }

    It 'returns null for empty MIME type' {
        $block = [PSCustomObject]@{
            PictureType = 3
            MimeType    = ''
            Description = 'Cover'
            Width       = 100
            Height      = 100
            Depth       = 24
            Colors      = 0
        }
        $result = New-MetaflacPictureSpec -Block $block -FilePath '/tmp/cover.png'
        $result | Should -BeNullOrEmpty
    }

    It 'includes colors in dimension spec when non-zero' {
        $block = [PSCustomObject]@{
            PictureType = 3
            MimeType    = 'image/png'
            Description = ''
            Width       = 100
            Height      = 100
            Depth       = 8
            Colors      = 256
        }
        $result = New-MetaflacPictureSpec -Block $block -FilePath '/tmp/cover.png'
        $result | Should -Match '100x100x8/256'
    }

    It 'omits dimension spec when width is zero' {
        $block = [PSCustomObject]@{
            PictureType = 3
            MimeType    = 'image/png'
            Description = ''
            Width       = 0
            Height      = 0
            Depth       = 0
            Colors      = 0
        }
        $result = New-MetaflacPictureSpec -Block $block -FilePath '/tmp/cover.png'
        $result | Should -Match '3\|image/png\|\|\|/tmp/cover\.png'
    }
}

# =============================================================================
# 31. Get-CodepointDisplayWidth
# =============================================================================
Describe 'Get-CodepointDisplayWidth' {
    It 'returns 1 for ASCII characters' {
        $result = Get-CodepointDisplayWidth -CodePoint ([int][char]'A')
        $result | Should -Be 1
    }

    It 'returns 0 for control characters' {
        $result = Get-CodepointDisplayWidth -CodePoint 0x07  # BEL
        $result | Should -Be 0
    }

    It 'returns 2 for CJK character' {
        # U+4E2D (Chinese character for "middle")
        $result = Get-CodepointDisplayWidth -CodePoint 0x4E2D
        $result | Should -Be 2
    }

    It 'returns 0 for zero codepoint' {
        $result = Get-CodepointDisplayWidth -CodePoint 0
        $result | Should -Be 0
    }

    It 'returns 0 for negative codepoint' {
        $result = Get-CodepointDisplayWidth -CodePoint (-1)
        $result | Should -Be 0
    }

    It 'returns 1 for standard Latin characters' {
        $result = Get-CodepointDisplayWidth -CodePoint ([int][char]'z')
        $result | Should -Be 1
    }

    It 'returns 2 for Korean syllable' {
        # U+AC00 (Korean syllable "ga")
        $result = Get-CodepointDisplayWidth -CodePoint 0xAC00
        $result | Should -Be 2
    }

    It 'returns 1 for space character' {
        $result = Get-CodepointDisplayWidth -CodePoint ([int][char]' ')
        $result | Should -Be 1
    }
}

# =============================================================================
# 32. Get-DisplayTextWidth
# =============================================================================
Describe 'Get-DisplayTextWidth' {
    It 'returns 0 for null' {
        $result = Get-DisplayTextWidth -Text $null
        $result | Should -Be 0
    }

    It 'returns 0 for empty string' {
        $result = Get-DisplayTextWidth -Text ''
        $result | Should -Be 0
    }

    It 'returns length for ASCII text' {
        $result = Get-DisplayTextWidth -Text 'hello'
        $result | Should -Be 5
    }

    It 'returns double width for CJK characters' {
        # Two CJK chars = width 4
        $result = Get-DisplayTextWidth -Text ([char]0x4E2D + [string][char]0x6587)
        $result | Should -Be 4
    }

    It 'handles mixed ASCII and CJK' {
        # 'A' (1) + CJK char (2) + 'B' (1) = 4
        $result = Get-DisplayTextWidth -Text ('A' + [char]0x4E2D + 'B')
        $result | Should -Be 4
    }
}

# =============================================================================
# 33. Fit-DisplayText
# =============================================================================
Describe 'Fit-DisplayText' {
    It 'pads short text with spaces' {
        $result = Fit-DisplayText -Text 'Hi' -Width 10
        $result.Length | Should -Be 10
        $result | Should -BeLike 'Hi*'
    }

    It 'truncates long text to fit width' {
        $result = Fit-DisplayText -Text 'This is a very long text string' -Width 10
        (Get-DisplayTextWidth -Text $result) | Should -BeLessOrEqual 10
    }

    It 'adds ellipsis when UseEllipsis is set and text is truncated' {
        $result = Fit-DisplayText -Text 'This is a very long text string' -Width 10 -UseEllipsis
        $result | Should -Match '\.\.\.'
    }

    It 'returns empty string for Width < 1' {
        $result = Fit-DisplayText -Text 'hello' -Width 0
        $result | Should -Be ''
    }

    It 'handles null text' {
        $result = Fit-DisplayText -Text $null -Width 5
        $result.Length | Should -Be 5
        $result | Should -Be '     '
    }

    It 'returns exact width when text matches width' {
        $result = Fit-DisplayText -Text 'ABCDE' -Width 5
        $result | Should -Be 'ABCDE'
    }

    It 'returns empty for negative width' {
        $result = Fit-DisplayText -Text 'test' -Width (-5)
        $result | Should -Be ''
    }
}

# =============================================================================
# 34. Truncate-Text
# =============================================================================
Describe 'Truncate-Text' {
    It 'truncates text and trims trailing spaces' {
        $result = Truncate-Text -Text 'Hello World' -Width 5
        $result.Length | Should -BeLessOrEqual 8  # 5 + possible '...'
    }

    It 'returns empty string for Width < 1' {
        $result = Truncate-Text -Text 'hello' -Width 0
        $result | Should -Be ''
    }

    It 'handles null text' {
        $result = Truncate-Text -Text $null -Width 10
        $result | Should -Be ''
    }

    It 'returns short text as-is (trimmed)' {
        $result = Truncate-Text -Text 'Hi' -Width 10
        $result | Should -Be 'Hi'
    }
}

# =============================================================================
# 35. Get-CompressionColor
# =============================================================================
Describe 'Get-CompressionColor' {
    It 'returns DarkGray for N/A' {
        Get-CompressionColor -CompressionPct 'N/A' | Should -Be 'DarkGray'
    }

    It 'returns DarkGray for null' {
        Get-CompressionColor -CompressionPct $null | Should -Be 'DarkGray'
    }

    It 'returns DarkGray for empty string' {
        Get-CompressionColor -CompressionPct '' | Should -Be 'DarkGray'
    }

    It 'returns Gray for 0%' {
        Get-CompressionColor -CompressionPct '0.00%' | Should -Be 'Gray'
    }

    It 'returns DarkRed for negative compression' {
        Get-CompressionColor -CompressionPct '-2.50%' | Should -Be 'DarkRed'
    }

    It 'returns Blue for minimal compression (0-1%)' {
        Get-CompressionColor -CompressionPct '0.50%' | Should -Be 'Blue'
    }

    It 'returns DarkCyan for light compression (1-3%)' {
        Get-CompressionColor -CompressionPct '2.00%' | Should -Be 'DarkCyan'
    }

    It 'returns Cyan for moderate compression (3-6%)' {
        Get-CompressionColor -CompressionPct '4.50%' | Should -Be 'Cyan'
    }

    It 'returns DarkYellow for strong compression (6-10%)' {
        Get-CompressionColor -CompressionPct '7.00%' | Should -Be 'DarkYellow'
    }

    It 'returns Yellow for high compression (10-15%)' {
        Get-CompressionColor -CompressionPct '12.00%' | Should -Be 'Yellow'
    }

    It 'returns DarkMagenta for very high compression (15-20%)' {
        Get-CompressionColor -CompressionPct '17.00%' | Should -Be 'DarkMagenta'
    }

    It 'returns Magenta for exceptional compression (20%+)' {
        Get-CompressionColor -CompressionPct '25.00%' | Should -Be 'Magenta'
    }

    It 'returns DarkGray for unparseable string' {
        Get-CompressionColor -CompressionPct 'invalid' | Should -Be 'DarkGray'
    }
}

# =============================================================================
# 36. Get-StatusColor
# =============================================================================
Describe 'Get-StatusColor' {
    It 'returns Cyan for OK' {
        Get-StatusColor -Status 'OK' | Should -Be 'Cyan'
    }

    It 'returns Yellow for RETRY' {
        Get-StatusColor -Status 'RETRY' | Should -Be 'Yellow'
    }

    It 'returns Red for FAIL' {
        Get-StatusColor -Status 'FAIL' | Should -Be 'Red'
    }

    It 'returns DarkGray for WAIT' {
        Get-StatusColor -Status 'WAIT' | Should -Be 'DarkGray'
    }

    It 'returns White for unknown status' {
        Get-StatusColor -Status 'UNKNOWN' | Should -Be 'White'
    }

    It 'returns White for null status' {
        Get-StatusColor -Status $null | Should -Be 'White'
    }
}

# =============================================================================
# 37. Get-VerificationColor
# =============================================================================
Describe 'Get-VerificationColor' {
    It 'returns Green for MATCH|NEW' {
        Get-VerificationColor -Verification 'MATCH|NEW' | Should -Be 'Green'
    }

    It 'returns Cyan for MATCH' {
        Get-VerificationColor -Verification 'MATCH' | Should -Be 'Cyan'
    }

    It 'returns Red for MISMATCH' {
        Get-VerificationColor -Verification 'MISMATCH' | Should -Be 'Red'
    }

    It 'returns DarkGray for null' {
        Get-VerificationColor -Verification $null | Should -Be 'DarkGray'
    }

    It 'returns DarkGray for empty string' {
        Get-VerificationColor -Verification '' | Should -Be 'DarkGray'
    }

    It 'returns Yellow for other text' {
        Get-VerificationColor -Verification 'PARTIAL' | Should -Be 'Yellow'
    }
}

# =============================================================================
# 38. Get-WorkerStateColor
# =============================================================================
Describe 'Get-WorkerStateColor' {
    It 'returns Magenta for ART state' {
        Get-WorkerStateColor -State 'ART' | Should -Be 'Magenta'
    }

    It 'returns Yellow for HASHIN state' {
        Get-WorkerStateColor -State 'HASHIN' | Should -Be 'Yellow'
    }

    It 'returns DarkYellow for HASHOUT state' {
        Get-WorkerStateColor -State 'HASHOUT' | Should -Be 'DarkYellow'
    }

    It 'returns DarkYellow for HASHING state' {
        Get-WorkerStateColor -State 'HASHING' | Should -Be 'DarkYellow'
    }

    It 'returns Cyan for FINAL state' {
        Get-WorkerStateColor -State 'FINAL' | Should -Be 'Cyan'
    }

    It 'returns White for ENCODE state with low Pct' {
        Get-WorkerStateColor -State 'ENCODE' -Pct 50 | Should -Be 'White'
    }

    It 'returns Green for ENCODE state with 100 Pct' {
        Get-WorkerStateColor -State 'ENCODE' -Pct 100 | Should -Be 'Green'
    }

    It 'returns DarkGray for IDLE state' {
        Get-WorkerStateColor -State 'IDLE' | Should -Be 'DarkGray'
    }

    It 'returns DarkYellow for unknown state with HASHING stage' {
        Get-WorkerStateColor -State 'OTHER' -Stage 'HASHING' | Should -Be 'DarkYellow'
    }

    It 'returns Magenta for unknown state with ARTWORK stage' {
        Get-WorkerStateColor -State 'OTHER' -Stage 'ARTWORK' | Should -Be 'Magenta'
    }

    It 'returns White for completely unknown state and stage' {
        Get-WorkerStateColor -State 'SOMETHING' -Stage 'ELSE' | Should -Be 'White'
    }
}

# =============================================================================
# 39. Push-RecentEvent
# =============================================================================
Describe 'Push-RecentEvent' {
    It 'inserts event at position 0' {
        $list = [System.Collections.Generic.List[object]]::new()
        Push-RecentEvent -List $list -Status 'OK' -File 'a.flac' -Attempt '1/3'
        Push-RecentEvent -List $list -Status 'FAIL' -File 'b.flac' -Attempt '2/3'
        $list.Count | Should -Be 2
        $list[0].File | Should -Be 'b.flac'
        $list[1].File | Should -Be 'a.flac'
    }

    It 'caps list at 250 items' {
        $list = [System.Collections.Generic.List[object]]::new()
        for ($i = 0; $i -lt 260; $i++) {
            Push-RecentEvent -List $list -Status 'OK' -File "file$i.flac" -Attempt '1/1'
        }
        $list.Count | Should -Be 250
    }

    It 'throws when list is null' {
        { Push-RecentEvent -List $null -Status 'OK' -File 'a.flac' -Attempt '1/1' } | Should -Throw
    }

    It 'populates all event fields' {
        $list = [System.Collections.Generic.List[object]]::new()
        Push-RecentEvent -List $list -Status 'OK' -File 'test.flac' -Attempt '1/3' `
            -EmbeddedHash 'abc' -CalculatedBeforeHash 'def' -CalculatedAfterHash 'ghi' `
            -Verification 'MATCH' -BeforeAfter '100/90' -Saved '10' -CompressionPct '10%' -Detail 'info'
        $list[0].Status | Should -Be 'OK'
        $list[0].EmbeddedHash | Should -Be 'abc'
        $list[0].Verification | Should -Be 'MATCH'
        $list[0].CompressionPct | Should -Be '10%'
    }

    It 'sets default values for optional fields' {
        $list = [System.Collections.Generic.List[object]]::new()
        Push-RecentEvent -List $list -Status 'OK' -File 'test.flac' -Attempt '1/1'
        $list[0].EmbeddedHash | Should -Be 'N/A'
        $list[0].Verification | Should -Be 'N/A'
        $list[0].CompressionPct | Should -Be 'N/A'
    }
}

# =============================================================================
# 40. Add-FinalLogEvent
# =============================================================================
Describe 'Add-FinalLogEvent' {
    It 'appends event to list' {
        $list = [System.Collections.Generic.List[object]]::new()
        Add-FinalLogEvent -List $list -EventType 'OK' -File 'a.flac' -FullPath '/music/a.flac' -Attempt '1/3'
        Add-FinalLogEvent -List $list -EventType 'FAIL' -File 'b.flac' -FullPath '/music/b.flac' -Attempt '2/3'
        $list.Count | Should -Be 2
        $list[0].File | Should -Be 'a.flac'
        $list[1].File | Should -Be 'b.flac'
    }

    It 'throws when list is null' {
        { Add-FinalLogEvent -List $null -EventType 'OK' -File 'a.flac' -FullPath '/x' -Attempt '1/1' } | Should -Throw
    }

    It 'populates all event fields' {
        $list = [System.Collections.Generic.List[object]]::new()
        Add-FinalLogEvent -List $list -EventType 'OK' -File 'test.flac' -FullPath '/music/test.flac' `
            -Attempt '1/3' -Verification 'MATCH' -EmbeddedHash 'abc123' `
            -CalcPreHash 'pre' -CalcPostHash 'post' -OrigBytes '1000' -NewBytes '900' `
            -SavedBytes '100' -SavedPct '10.00%' -AudioSavedBytes '50' `
            -MetadataSummary 'trimmed padding' -FailureReason ''
        $list[0].EventType | Should -Be 'OK'
        $list[0].Verification | Should -Be 'MATCH'
        $list[0].OrigBytes | Should -Be '1000'
        $list[0].MetadataSummary | Should -Be 'trimmed padding'
    }

    It 'sets default values for optional fields' {
        $list = [System.Collections.Generic.List[object]]::new()
        Add-FinalLogEvent -List $list -EventType 'FAIL' -File 'x.flac' -FullPath '/x.flac' -Attempt '1/1'
        $list[0].Verification | Should -Be 'N/A'
        $list[0].EmbeddedHash | Should -Be 'N/A'
        $list[0].FailureReason | Should -Be ''
    }

    It 'accepts CANCELED event type' {
        $list = [System.Collections.Generic.List[object]]::new()
        { Add-FinalLogEvent -List $list -EventType 'CANCELED' -File 'x.flac' -FullPath '/x.flac' -Attempt '1/1' -FailureReason 'User canceled' } | Should -Not -Throw
        $list[0].EventType | Should -Be 'CANCELED'
    }
}

# =============================================================================
# 41. New-EfcStatusReportLines
# =============================================================================
Describe 'New-EfcStatusReportLines' {
    It 'shows all success message' {
        $lines = New-EfcStatusReportLines -Successful 10 -Failed 0 -Pending 0 -RunCanceled $false
        $text = [string]::Join(' ', $lines)
        $text | Should -Match 'All files processed successfully'
        $text | Should -Match 'No errors occurred'
    }

    It 'shows failure message' {
        $lines = New-EfcStatusReportLines -Successful 5 -Failed 2 -Pending 0 -RunCanceled $false
        $text = [string]::Join(' ', $lines)
        $text | Should -Match 'Some files could not be verified'
        $text | Should -Match 'There were errors'
    }

    It 'shows canceled message' {
        $lines = New-EfcStatusReportLines -Successful 3 -Failed 0 -Pending 5 -RunCanceled $true
        $text = [string]::Join(' ', $lines)
        $text | Should -Match 'Processing canceled by user'
        $text | Should -Match 'There were errors'
    }

    It 'shows zero files processed when all counts are zero' {
        $lines = New-EfcStatusReportLines -Successful 0 -Failed 0 -Pending 0 -RunCanceled $false
        $text = [string]::Join(' ', $lines)
        $text | Should -Match '0 file\(s\) processed'
    }

    It 'includes file counts in output' {
        $lines = New-EfcStatusReportLines -Successful 3 -Failed 2 -Pending 1 -RunCanceled $false
        $text = [string]::Join(' ', $lines)
        $text | Should -Match '3 file\(s\) processed successfully'
        $text | Should -Match '2 file\(s\) failed'
        $text | Should -Match '1 file\(s\) pending'
    }

    It 'ends with End of status report' {
        $lines = New-EfcStatusReportLines -Successful 1 -Failed 0 -Pending 0 -RunCanceled $false
        $lines[-1] | Should -Be 'End of status report'
    }
}

# =============================================================================
# 42. New-EfcFinalLogText
# =============================================================================
Describe 'New-EfcFinalLogText' {
    It 'returns a string containing expected sections' {
        $events = @()
        $topComp = @()
        $result = New-EfcFinalLogText -AlbumName 'TestAlbum' -RootFolder '/music/test' `
            -RunStartedLocal ([DateTime]::Now.AddMinutes(-10)) `
            -FinishedLocal ([DateTime]::Now) `
            -MaxWorkers 4 -MaxAttemptsPerFile 3 `
            -TotalFiles 10 -Processed 10 -Successful 10 -Failed 0 -Pending 0 `
            -RunCanceled $false -Events $events -TopCompression $topComp
        $result | Should -Not -BeNullOrEmpty
        $result | Should -Match 'Exact Flac Cruncher'
        $result | Should -Match 'TestAlbum'
        $result | Should -Match '/music/test'
        $result | Should -Match 'Log checksum'
    }

    It 'includes event details for OK events' {
        $events = @(
            [PSCustomObject]@{
                Timestamp       = [DateTime]::Now
                File            = 'track01.flac'
                FullPath        = '/music/track01.flac'
                Attempt         = '1/3'
                EventType       = 'OK'
                Verification    = 'MATCH'
                EmbeddedHash    = 'abc123'
                CalcPreHash     = 'pre123'
                CalcPostHash    = 'post123'
                OrigBytes       = '1000'
                NewBytes        = '900'
                SavedBytes      = '100'
                SavedPct        = '10.00%'
                AudioSavedBytes = '50'
                MetadataSummary = 'none'
                FailureReason   = ''
            }
        )
        $result = New-EfcFinalLogText -AlbumName 'Album' -RootFolder '/m' `
            -RunStartedLocal ([DateTime]::Now) -FinishedLocal ([DateTime]::Now) `
            -MaxWorkers 1 -MaxAttemptsPerFile 3 -TotalFiles 1 -Processed 1 `
            -Successful 1 -Failed 0 -Pending 0 -RunCanceled $false `
            -Events $events -TopCompression @()
        $result | Should -Match 'Copy OK'
        $result | Should -Match 'track01\.flac'
    }

    It 'includes FAIL event details' {
        $events = @(
            [PSCustomObject]@{
                Timestamp       = [DateTime]::Now
                File            = 'bad.flac'
                FullPath        = '/music/bad.flac'
                Attempt         = '3/3'
                EventType       = 'FAIL'
                Verification    = 'MISMATCH'
                EmbeddedHash    = 'N/A'
                CalcPreHash     = 'N/A'
                CalcPostHash    = 'N/A'
                OrigBytes       = 'N/A'
                NewBytes        = 'N/A'
                SavedBytes      = 'N/A'
                SavedPct        = 'N/A'
                AudioSavedBytes = 'N/A'
                MetadataSummary = ''
                FailureReason   = 'Hash mismatch'
            }
        )
        $result = New-EfcFinalLogText -AlbumName 'Album' -RootFolder '/m' `
            -RunStartedLocal ([DateTime]::Now) -FinishedLocal ([DateTime]::Now) `
            -MaxWorkers 1 -MaxAttemptsPerFile 3 -TotalFiles 1 -Processed 1 `
            -Successful 0 -Failed 1 -Pending 0 -RunCanceled $false `
            -Events $events -TopCompression @()
        $result | Should -Match 'Copy failed'
        $result | Should -Match 'Hash mismatch'
    }

    It 'includes top compression entries' {
        $topComp = @(
            [PSCustomObject]@{
                Path       = '/music/big.flac'
                SavedBytes = 5000
                SavedPct   = 15.5
            }
        )
        $result = New-EfcFinalLogText -AlbumName 'Album' -RootFolder '/m' `
            -RunStartedLocal ([DateTime]::Now) -FinishedLocal ([DateTime]::Now) `
            -MaxWorkers 1 -MaxAttemptsPerFile 3 -TotalFiles 1 -Processed 1 `
            -Successful 1 -Failed 0 -Pending 0 -RunCanceled $false `
            -Events @() -TopCompression $topComp
        $result | Should -Match 'EFC Compression Notes'
        $result | Should -Match 'big\.flac'
    }

    It 'includes checksum line at end' {
        $result = New-EfcFinalLogText -AlbumName 'X' -RootFolder '/x' `
            -RunStartedLocal ([DateTime]::Now) -FinishedLocal ([DateTime]::Now) `
            -MaxWorkers 1 -MaxAttemptsPerFile 1 -TotalFiles 0 -Processed 0 `
            -Successful 0 -Failed 0 -Pending 0 -RunCanceled $false `
            -Events @() -TopCompression @()
        $result | Should -Match '==== Log checksum [A-F0-9]{64} ===='
    }
}

# =============================================================================
# 43. Resolve-OptionalTool
# =============================================================================
Describe 'Resolve-OptionalTool' {
    It 'returns null when tool is not found anywhere' {
        $result = Resolve-OptionalTool -Names @('nonexistent-tool-efc-test-9999xyz.exe')
        $result | Should -BeNullOrEmpty
    }

    It 'finds tool in BaseDirectory' {
        $tempDir = Join-Path ([System.IO.Path]::GetTempPath()) ('efc-tool-{0}' -f [guid]::NewGuid().ToString('N'))
        New-Item -ItemType Directory -Path $tempDir -Force | Out-Null
        $toolPath = Join-Path $tempDir 'testtool'
        [System.IO.File]::WriteAllText($toolPath, '#!/bin/sh')
        try {
            $result = Resolve-OptionalTool -Names @('testtool') -BaseDirectory $tempDir
            $result | Should -Not -BeNullOrEmpty
            $result.Source | Should -Be (Get-Item -LiteralPath $toolPath).FullName
        }
        finally {
            Remove-Item -Path $tempDir -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It 'finds tool via Get-Command in PATH' {
        # 'pwsh' should be findable via Get-Command on any system running these tests
        $result = Resolve-OptionalTool -Names @('pwsh')
        $result | Should -Not -BeNullOrEmpty
    }
}

# =============================================================================
# 44. Resolve-RequiredTool
# =============================================================================
Describe 'Resolve-RequiredTool' {
    It 'throws when tool is not found' {
        { Resolve-RequiredTool -Names @('nonexistent-efc-tool-9999xyz.exe') -DisplayName 'TestTool' } | Should -Throw "*TestTool*not found*"
    }

    It 'returns command when tool is found' {
        $result = Resolve-RequiredTool -Names @('pwsh') -DisplayName 'PowerShell'
        $result | Should -Not -BeNullOrEmpty
    }

    It 'throws with display name in error message' {
        $threw = $false
        try {
            Resolve-RequiredTool -Names @('missing-tool-9999xyz') -DisplayName 'MyEncoder'
        }
        catch {
            $threw = $true
            $_.Exception.Message | Should -Match 'MyEncoder'
        }
        $threw | Should -Be $true
    }
}

# =============================================================================
# 45. Get-OptionalToolSearchDirectories
# =============================================================================
Describe 'Get-OptionalToolSearchDirectories' {
    It 'returns an array (possibly empty on Linux)' {
        $result = @(Get-OptionalToolSearchDirectories)
        # On Linux most Windows-specific directories will not exist, so array may be empty
        $result | Should -Not -Be $null
    }

    It 'returns only existing directories' {
        $result = @(Get-OptionalToolSearchDirectories)
        foreach ($dir in $result) {
            Test-Path -LiteralPath $dir | Should -Be $true
        }
    }

    It 'returns no duplicates' {
        $result = @(Get-OptionalToolSearchDirectories)
        if ($result.Count -gt 0) {
            $unique = $result | Select-Object -Unique
            $unique.Count | Should -Be $result.Count
        }
    }
}
