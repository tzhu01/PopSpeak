Set-StrictMode -Version Latest

function Get-RuntimeIntegrityManifest {
    $manifestPath = Join-Path $PSScriptRoot "runtime-integrity-manifest.json"
    if (!(Test-Path -LiteralPath $manifestPath -PathType Leaf)) {
        throw "Runtime integrity manifest is missing: $manifestPath"
    }
    return Get-Content -LiteralPath $manifestPath -Raw | ConvertFrom-Json
}

function Assert-Sha256Text([string]$Sha256, [string]$Label) {
    if ($Sha256 -notmatch '^[0-9a-f]{64}$') {
        throw "Invalid lowercase SHA-256 in the integrity manifest for ${Label}: $Sha256"
    }
}

function Assert-ManifestFileName([string]$Name) {
    if ([string]::IsNullOrWhiteSpace($Name) -or
        [IO.Path]::IsPathRooted($Name) -or
        $Name -ne [IO.Path]::GetFileName($Name) -or
        $Name.Contains('/') -or
        $Name.Contains('\')) {
        throw "Runtime manifest entries must be plain file names: $Name"
    }
}

function Assert-PinnedFile(
    [string]$Path,
    [string]$ExpectedSha256,
    [long]$ExpectedSize,
    [string]$Label
) {
    Assert-Sha256Text $ExpectedSha256 $Label
    if (!(Test-Path -LiteralPath $Path -PathType Leaf)) {
        throw "Missing ${Label}: $Path"
    }
    $item = Get-Item -LiteralPath $Path
    if ($ExpectedSize -ge 0 -and $item.Length -ne $ExpectedSize) {
        throw "Unexpected size for $Label. Expected $ExpectedSize bytes, got $($item.Length): $Path"
    }
    $actualSha256 = (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($actualSha256 -ne $ExpectedSha256) {
        throw "SHA-256 mismatch for $Label. Expected $ExpectedSha256, got ${actualSha256}: $Path"
    }
}

function Assert-ExactRuntimeFileSet([string]$Directory, [object[]]$ExpectedFiles) {
    if (!(Test-Path -LiteralPath $Directory -PathType Container)) {
        throw "Runtime directory is missing: $Directory"
    }

    $expectedByName = @{}
    foreach ($entry in $ExpectedFiles) {
        $name = [string]$entry.name
        Assert-ManifestFileName $name
        if ($expectedByName.ContainsKey($name)) {
            throw "Duplicate runtime manifest entry: $name"
        }
        $expectedByName[$name] = $entry
        Assert-PinnedFile `
            (Join-Path $Directory $name) `
            ([string]$entry.sha256) `
            ([long]$entry.size) `
            "runtime file $name"
    }

    $actualDirectories = @(Get-ChildItem -LiteralPath $Directory -Force -Directory)
    if ($actualDirectories.Count -gt 0) {
        $names = ($actualDirectories.Name | Sort-Object) -join ', '
        throw "Unexpected directories in integrity-gated runtime directory ${Directory}: $names"
    }
    $actualFiles = @(Get-ChildItem -LiteralPath $Directory -Force -File)
    $unexpected = @($actualFiles | Where-Object { !$expectedByName.ContainsKey($_.Name) })
    if ($unexpected.Count -gt 0) {
        $names = ($unexpected.Name | Sort-Object) -join ', '
        throw "Unexpected files in integrity-gated runtime directory ${Directory}: $names"
    }
    if ($actualFiles.Count -ne $expectedByName.Count) {
        throw "Runtime file count mismatch in $Directory. Expected $($expectedByName.Count), got $($actualFiles.Count)"
    }
}

function Test-ExactRuntimeFileSet([string]$Directory, [object[]]$ExpectedFiles) {
    try {
        Assert-ExactRuntimeFileSet $Directory $ExpectedFiles
        return $true
    }
    catch {
        return $false
    }
}
