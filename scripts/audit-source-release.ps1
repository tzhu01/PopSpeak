[CmdletBinding()]
param(
    [Parameter()]
    [string]$PublishRef = 'HEAD',

    [Parameter()]
    [string]$DenylistPath,

    [Parameter()]
    [switch]$RequireCleanWorktree
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$repoPathPrefix = $repoRoot.TrimEnd(
    [System.IO.Path]::DirectorySeparatorChar,
    [System.IO.Path]::AltDirectorySeparatorChar
) + [System.IO.Path]::DirectorySeparatorChar
$pathComparison = if ([System.IO.Path]::DirectorySeparatorChar -eq '\') {
    [System.StringComparison]::OrdinalIgnoreCase
}
else {
    [System.StringComparison]::Ordinal
}
$maxUntrackedContentBytes = 16MB
$findings = [System.Collections.Generic.List[object]]::new()
$findingKeys = [System.Collections.Generic.HashSet[string]]::new(
    [System.StringComparer]::OrdinalIgnoreCase
)

function Add-Finding {
    param(
        [Parameter(Mandatory)]
        [string]$Source,

        [Parameter(Mandatory)]
        [string]$Category,

        [Parameter(Mandatory)]
        [string]$Path
    )

    # Findings deliberately contain metadata and paths only. Never add matched
    # content, a denylist value, or a line excerpt here.
    $normalizedPath = $Path.Replace('\', '/')
    $key = "$Source`n$Category`n$normalizedPath"
    if ($findingKeys.Add($key)) {
        $findings.Add([pscustomobject]@{
                Source   = $Source
                Category = $Category
                Path     = $normalizedPath
            })
    }
}

function Get-SensitivePathCategories {
    param(
        [Parameter(Mandatory)]
        [string]$Path
    )

    $normalized = $Path.Replace('\', '/').ToLowerInvariant()
    $leaf = [System.IO.Path]::GetFileName($normalized)
    $extension = [System.IO.Path]::GetExtension($leaf)
    $isTemplate = $leaf -match '\.(?:example|sample|template)\.'
    $categories = [System.Collections.Generic.List[string]]::new()

    if (
        $leaf -match '^\.env(?:\.|$)' -and
        $leaf -notmatch '\.(?:example|sample|template)$'
    ) {
        $categories.Add('environment-file')
    }

    if (
        $leaf -in @(
            'settings.json',
            'credentials.json',
            'secrets.json',
            'pending-transcript.json',
            '.npmrc',
            '.pypirc',
            '.netrc',
            '_netrc'
        ) -or
        (
            -not $isTemplate -and
            $leaf -match '^(?:client[_-]?secret|service[_-]?account|credentials|secrets)(?:\.[a-z0-9_-]+)*\.(?:json|ya?ml|toml|ini|conf|txt)$'
        ) -or
        $normalized -match '(^|/)\.aws/credentials$' -or
        $normalized -match '(^|/)\.docker/config\.json$'
    ) {
        $categories.Add('credential-or-local-settings')
    }

    if (
        $extension -in @('.db', '.db3', '.sqlite', '.sqlite3') -or
        $leaf -match '\.(?:sqlite|sqlite3)-(?:wal|shm)$'
    ) {
        $categories.Add('database')
    }

    if (
        $extension -in @('.pem', '.key', '.pfx', '.p12', '.ppk', '.jks', '.keystore') -or
        $leaf -match '^id_(?:rsa|dsa|ecdsa|ed25519)$'
    ) {
        $categories.Add('private-key-or-keystore')
    }

    if ($extension -in @('.zip', '.7z', '.rar', '.tar', '.tgz', '.gz', '.bz2', '.xz', '.cab', '.iso')) {
        $categories.Add('archive')
    }

    if (
        $extension -in @(
            '.wav', '.mp3', '.m4a', '.aac', '.flac', '.ogg', '.oga', '.opus',
            '.wma', '.pcm', '.raw', '.caf', '.aiff', '.aif', '.amr'
        ) -or
        $normalized -match '(^|/)(?:recordings|audio-captures)(?:/|$)'
    ) {
        $categories.Add('audio-or-recording')
    }

    return $categories
}

function Add-PathFindings {
    param(
        [Parameter(Mandatory)]
        [string]$Source,

        [Parameter(Mandatory)]
        [AllowEmptyCollection()]
        [string[]]$Paths
    )

    foreach ($path in $Paths) {
        if ([string]::IsNullOrWhiteSpace($path)) {
            continue
        }

        foreach ($category in (Get-SensitivePathCategories -Path $path)) {
            Add-Finding -Source $Source -Category $category -Path $path
        }
    }
}

function Invoke-GitLiteralGrep {
    param(
        [Parameter(Mandatory)]
        [string[]]$Patterns,

        [Parameter(Mandatory)]
        [string]$Source,

        [Parameter()]
        [string]$Commit
    )

    if ($Patterns.Count -eq 0) {
        return
    }

    # Patterns travel over stdin rather than the process command line. `-l`
    # guarantees that Git emits matching paths only, never matching content.
    if ($Commit) {
        $output = $Patterns | & git -c core.quotepath=false grep -l -F -f - $Commit -- 2>$null
    }
    else {
        $output = $Patterns | & git -c core.quotepath=false grep -l -F -f - -- 2>$null
    }
    $exitCode = $LASTEXITCODE

    # git grep returns 1 for no matches and >1 for an operational error.
    if ($exitCode -gt 1) {
        throw "git grep failed while scanning $Source (exit code $exitCode)."
    }

    foreach ($line in @($output)) {
        $path = [string]$line
        if ($Commit -and $path.StartsWith("$Commit`:", [System.StringComparison]::OrdinalIgnoreCase)) {
            $path = $path.Substring($Commit.Length + 1)
        }

        if (-not [string]::IsNullOrWhiteSpace($path)) {
            $path
        }
    }
}

function Add-UntrackedContentFindings {
    param(
        [Parameter(Mandatory)]
        [AllowEmptyCollection()]
        [string[]]$Paths,

        [Parameter(Mandatory)]
        [object[]]$Checks
    )

    foreach ($path in $Paths) {
        if ([string]::IsNullOrWhiteSpace($path)) {
            continue
        }

        try {
            $fullPath = [System.IO.Path]::GetFullPath((Join-Path $repoRoot $path))
        }
        catch {
            Add-Finding -Source 'untracked-worktree' -Category 'invalid-untracked-path' -Path $path
            continue
        }

        # Git supplied the relative path, but keep content reads inside the
        # repository even if a malformed path reaches this function.
        if (-not $fullPath.StartsWith($repoPathPrefix, $pathComparison)) {
            Add-Finding -Source 'untracked-worktree' -Category 'outside-repository-path' -Path $path
            continue
        }

        try {
            $item = Get-Item -LiteralPath $fullPath -Force -ErrorAction Stop
            if (-not $item.PSIsContainer -and (
                    $item.Attributes -band [System.IO.FileAttributes]::ReparsePoint
                ) -ne 0) {
                Add-Finding -Source 'untracked-worktree' -Category 'linked-file-not-content-scanned' -Path $path
                continue
            }
            if ($item.PSIsContainer) {
                Add-Finding -Source 'untracked-worktree' -Category 'unexpected-untracked-directory' -Path $path
                continue
            }
            if ($item.Length -gt $maxUntrackedContentBytes) {
                Add-Finding -Source 'untracked-worktree' -Category 'large-file-not-content-scanned' -Path $path
                continue
            }

            $content = [System.IO.File]::ReadAllText($fullPath)
        }
        catch {
            Add-Finding -Source 'untracked-worktree' -Category 'unreadable-untracked-file' -Path $path
            continue
        }

        foreach ($check in $Checks) {
            foreach ($pattern in @($check.Patterns)) {
                if ($content.IndexOf($pattern, [System.StringComparison]::Ordinal) -ge 0) {
                    Add-Finding -Source 'untracked-worktree' -Category $check.Category -Path $path
                    break
                }
            }
        }
    }
}

function Get-DenylistPatterns {
    param(
        [Parameter(Mandatory)]
        [string]$Path
    )

    $resolved = (Resolve-Path -LiteralPath $Path).Path
    $patterns = [System.Collections.Generic.List[string]]::new()
    $seen = [System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::Ordinal)
    $lineNumber = 0

    foreach ($rawLine in [System.IO.File]::ReadAllLines($resolved)) {
        $lineNumber++
        $value = $rawLine.Trim()
        if ($value.Length -eq 0 -or $value.StartsWith('#')) {
            continue
        }

        if ($value.IndexOf([char]0) -ge 0) {
            throw "Denylist entry $lineNumber contains a NUL character."
        }
        if ($value.Length -lt 8) {
            throw "Denylist entry $lineNumber is shorter than 8 characters; refusing a noisy scan."
        }

        if ($seen.Add($value)) {
            $patterns.Add($value)
        }
    }

    if ($patterns.Count -eq 0) {
        throw 'The denylist contains no usable values.'
    }

    return $patterns.ToArray()
}

Push-Location $repoRoot
try {
    if (-not (Get-Command git -ErrorAction SilentlyContinue)) {
        throw 'Git is required but was not found on PATH.'
    }

    $insideWorktree = (& git rev-parse --is-inside-work-tree 2>$null).Trim()
    if ($LASTEXITCODE -ne 0 -or $insideWorktree -ne 'true') {
        throw "$repoRoot is not a Git working tree."
    }

    $commit = (& git rev-parse --verify "$PublishRef^{commit}" 2>$null).Trim()
    if ($LASTEXITCODE -ne 0 -or $commit -notmatch '^[0-9a-fA-F]{40,64}$') {
        throw "Publish ref '$PublishRef' does not resolve to a commit."
    }

    Write-Host "Source release snapshot audit: $PublishRef ($($commit.Substring(0, 12)))"
    Write-Host 'Scope: tracked files, the selected commit tree, and untracked non-ignored files.'

    $trackedPaths = @(& git -c core.quotepath=false ls-files --)
    if ($LASTEXITCODE -ne 0) {
        throw 'Unable to enumerate tracked worktree paths.'
    }
    Add-PathFindings -Source 'tracked-worktree' -Paths $trackedPaths

    $publishPaths = @(& git -c core.quotepath=false ls-tree -r --name-only $commit --)
    if ($LASTEXITCODE -ne 0) {
        throw 'Unable to enumerate the selected publish commit.'
    }
    Add-PathFindings -Source "publish-ref:$PublishRef" -Paths $publishPaths

    # This intentionally excludes anything matched by .gitignore, including
    # downloaded models, target/, node_modules/ and local build output.
    $untrackedPaths = @(& git -c core.quotepath=false ls-files --others --exclude-standard --)
    if ($LASTEXITCODE -ne 0) {
        throw 'Unable to enumerate untracked non-ignored paths.'
    }
    Add-PathFindings -Source 'untracked-worktree' -Paths $untrackedPaths

    # Build these at runtime so the audit script does not match its own source.
    $pemBegin = '-----' + 'BEGIN'
    $privateKeyMarkers = @(
        "$pemBegin PRIVATE KEY-----",
        "$pemBegin ENCRYPTED PRIVATE KEY-----",
        "$pemBegin OPENSSH PRIVATE KEY-----",
        "$pemBegin RSA PRIVATE KEY-----",
        "$pemBegin DSA PRIVATE KEY-----",
        "$pemBegin EC PRIVATE KEY-----"
    )

    foreach ($path in @(Invoke-GitLiteralGrep -Patterns $privateKeyMarkers -Source 'tracked-worktree')) {
        Add-Finding -Source 'tracked-worktree' -Category 'private-key-marker' -Path $path
    }
    foreach ($path in @(Invoke-GitLiteralGrep -Patterns $privateKeyMarkers -Source "publish-ref:$PublishRef" -Commit $commit)) {
        Add-Finding -Source "publish-ref:$PublishRef" -Category 'private-key-marker' -Path $path
    }

    $untrackedContentChecks = [System.Collections.Generic.List[object]]::new()
    $untrackedContentChecks.Add([pscustomobject]@{
            Category = 'private-key-marker'
            Patterns = $privateKeyMarkers
        })

    if ($DenylistPath) {
        $denylistPatterns = @(Get-DenylistPatterns -Path $DenylistPath)
        Write-Host "Known-value denylist loaded ($($denylistPatterns.Count) entries; values are never printed)."

        foreach ($path in @(Invoke-GitLiteralGrep -Patterns $denylistPatterns -Source 'tracked-worktree')) {
            Add-Finding -Source 'tracked-worktree' -Category 'known-sensitive-value' -Path $path
        }
        foreach ($path in @(Invoke-GitLiteralGrep -Patterns $denylistPatterns -Source "publish-ref:$PublishRef" -Commit $commit)) {
            Add-Finding -Source "publish-ref:$PublishRef" -Category 'known-sensitive-value' -Path $path
        }

        $untrackedContentChecks.Add([pscustomobject]@{
                Category = 'known-sensitive-value'
                Patterns = $denylistPatterns
            })
    }

    Add-UntrackedContentFindings -Paths $untrackedPaths -Checks $untrackedContentChecks.ToArray()

    if ($RequireCleanWorktree) {
        $dirtyEntries = @(& git -c core.quotepath=false status --porcelain=v1 --untracked-files=all)
        if ($LASTEXITCODE -ne 0) {
            throw 'Unable to inspect the worktree status.'
        }

        foreach ($entry in $dirtyEntries) {
            if ($entry.Length -ge 4) {
                Add-Finding -Source 'worktree-status' -Category 'dirty-worktree' -Path $entry.Substring(3)
            }
            elseif (-not [string]::IsNullOrWhiteSpace($entry)) {
                Add-Finding -Source 'worktree-status' -Category 'dirty-worktree' -Path '<unparsed-status-entry>'
            }
        }
    }

    Write-Host ''
    Write-Host 'History boundary: this script does not prove that Git history or other refs are clean.'
    Write-Host 'Before a public push, run Gitleaks in Git-history mode over every ref that will be published.'

    if ($findings.Count -gt 0) {
        Write-Host ''
        Write-Host "SOURCE RELEASE AUDIT FAILED: $($findings.Count) finding(s)." -ForegroundColor Red
        foreach ($finding in ($findings | Sort-Object Source, Category, Path)) {
            Write-Host "[$($finding.Category)] [$($finding.Source)] $($finding.Path)"
        }
        exit 1
    }

    Write-Host ''
    Write-Host 'SOURCE RELEASE SNAPSHOT AUDIT PASSED.' -ForegroundColor Green
    exit 0
}
finally {
    Pop-Location
}
