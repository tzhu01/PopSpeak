$ErrorActionPreference = "Stop"
. (Join-Path $PSScriptRoot "runtime-integrity.ps1")

function Assert-Throws([scriptblock]$Action, [string]$Label) {
    $threw = $false
    try { & $Action }
    catch { $threw = $true }
    if (!$threw) { throw "Expected integrity check to fail: $Label" }
}

$manifest = Get-RuntimeIntegrityManifest
if ([int]$manifest.schemaVersion -ne 1) { throw "Unexpected runtime integrity schema" }

foreach ($source in @($manifest.funasrBuildSources.funasr, $manifest.funasrBuildSources.llamaCpp)) {
    if ([string]$source.revision -notmatch '^[0-9a-f]{40}$') {
        throw "Build source revision is not a pinned commit: $($source.revision)"
    }
    if (-not ([string]$source.archiveUrl).StartsWith("https://codeload.github.com/", [StringComparison]::Ordinal)) {
        throw "Build source is not an official GitHub codeload URL: $($source.archiveUrl)"
    }
    Assert-Sha256Text ([string]$source.archiveSha256) "source archive"
    if ([long]$source.archiveSize -le 0) { throw "Source archive size must be positive" }
}

foreach ($runtime in @($manifest.whisper, $manifest.llama)) {
    if (-not ([string]$runtime.archiveUrl).StartsWith("https://github.com/", [StringComparison]::Ordinal)) {
        throw "Runtime archive is not an official GitHub release URL: $($runtime.archiveUrl)"
    }
    Assert-Sha256Text ([string]$runtime.archiveSha256) "runtime archive"
    if ([long]$runtime.archiveSize -le 0) { throw "Runtime archive size must be positive" }
    if (@($runtime.installedFiles).Count -eq 0) { throw "Runtime file manifest cannot be empty" }

    $seen = @{}
    foreach ($entry in $runtime.installedFiles) {
        Assert-ManifestFileName ([string]$entry.name)
        Assert-Sha256Text ([string]$entry.sha256) "runtime file $($entry.name)"
        if ([long]$entry.size -le 0) { throw "Runtime file size must be positive: $($entry.name)" }
        if ($seen.ContainsKey([string]$entry.name)) { throw "Duplicate runtime file: $($entry.name)" }
        $seen[[string]$entry.name] = $true
    }
}

$fixtureRoot = Join-Path ([IO.Path]::GetTempPath()) ("popspeak-integrity-test-" + [guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Path $fixtureRoot | Out-Null
try {
    $fixtureFile = Join-Path $fixtureRoot "runtime.dll"
    Set-Content -LiteralPath $fixtureFile -Value "reviewed runtime fixture" -NoNewline
    $fixtureItem = Get-Item -LiteralPath $fixtureFile
    $fixtureHash = (Get-FileHash -LiteralPath $fixtureFile -Algorithm SHA256).Hash.ToLowerInvariant()
    $fixtureManifest = @([pscustomobject]@{
            name = "runtime.dll"
            size = $fixtureItem.Length
            sha256 = $fixtureHash
        })

    Assert-ExactRuntimeFileSet $fixtureRoot $fixtureManifest

    Set-Content -LiteralPath $fixtureFile -Value "tampered runtime fixture" -NoNewline
    Assert-Throws { Assert-ExactRuntimeFileSet $fixtureRoot $fixtureManifest } "tampered file"

    Set-Content -LiteralPath $fixtureFile -Value "reviewed runtime fixture" -NoNewline
    Set-Content -LiteralPath (Join-Path $fixtureRoot "unexpected.dll") -Value "unexpected" -NoNewline
    Assert-Throws { Assert-ExactRuntimeFileSet $fixtureRoot $fixtureManifest } "unexpected DLL"

    Remove-Item -LiteralPath (Join-Path $fixtureRoot "unexpected.dll")
    $unexpectedDirectory = Join-Path $fixtureRoot "unexpected-directory"
    New-Item -ItemType Directory -Path $unexpectedDirectory | Out-Null
    Assert-Throws { Assert-ExactRuntimeFileSet $fixtureRoot $fixtureManifest } "unexpected directory"
    Remove-Item -LiteralPath $unexpectedDirectory

    Remove-Item -LiteralPath $fixtureFile
    Assert-Throws { Assert-ExactRuntimeFileSet $fixtureRoot $fixtureManifest } "missing file"

    $escapingManifest = @([pscustomobject]@{
            name = "..\escape.dll"
            size = 1
            sha256 = ("0" * 64)
        })
    Assert-Throws { Assert-ExactRuntimeFileSet $fixtureRoot $escapingManifest } "path traversal"
}
finally {
    $resolvedTemp = [IO.Path]::GetFullPath([IO.Path]::GetTempPath())
    $resolvedFixture = [IO.Path]::GetFullPath($fixtureRoot)
    if (!$resolvedFixture.StartsWith($resolvedTemp, [StringComparison]::OrdinalIgnoreCase) -or
        !(Split-Path -Leaf $resolvedFixture).StartsWith("popspeak-integrity-test-", [StringComparison]::Ordinal)) {
        throw "Refusing to remove an unexpected fixture path: $resolvedFixture"
    }
    if (Test-Path -LiteralPath $resolvedFixture) {
        Remove-Item -LiteralPath $resolvedFixture -Recurse -Force
    }
}

Write-Host "Runtime integrity manifest and fail-closed checks passed."
