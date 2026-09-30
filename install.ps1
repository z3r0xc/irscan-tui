<#
.SYNOPSIS
    Install irscan-tui, or upgrade it if it is already installed.

.DESCRIPTION
    Downloads the latest release from GitHub, verifies it against the published
    SHA-256, and puts it somewhere on PATH.

    Why verify at all: the download is over HTTPS, but the whole point of a
    one-liner installer is that nobody reads what it is about to run, and a hash
    published in the same place as the binary only proves the file arrived
    intact. It catches the more likely failure by far - a truncated download
    behind a proxy - and it is cheap. It is not a signature and does not pretend
    to be one.

.PARAMETER Version
    A tag to install, e.g. v0.1.0. Defaults to the latest release.

.PARAMETER InstallDir
    Where to put the executable. Defaults to a per-user directory on PATH, so
    no administrator rights are needed. Defaults to
    $env:LOCALAPPDATA\Programs\irscan-tui.

.PARAMETER Force
    Install even if the hash does not match, and even if a different version is
    already installed. The hash override is deliberately named rather than
    implied: if you need it, something is wrong that you should look at.

.EXAMPLE
    irm https://raw.githubusercontent.com/z3r0xc/irscan-tui/main/install.ps1 | iex

.EXAMPLE
    & ([scriptblock]::Create((irm https://raw.githubusercontent.com/z3r0xc/irscan-tui/main/install.ps1))) -Version v0.1.0

.NOTES
    The binary is built with a static CRT, so it needs no Visual C++
    redistributable and no runtime beside it. That is not an optimisation - a
    dynamically linked build would install successfully here and then refuse to
    start on a machine without the redistributable.
#>

[CmdletBinding()]
param(
    [string] $Version,
    [string] $InstallDir = (Join-Path $env:LOCALAPPDATA 'Programs\irscan-tui'),
    [switch] $Force
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$Repo = 'z3r0xc/irscan-tui'
$BinaryName = 'irscan-tui.exe'
# The asset name carries the triple, because a user who has both an arm64 build
# and an x64 one on PATH should be able to tell which is which.
$AssetSuffix = 'x86_64-pc-windows-msvc'

function Write-Step { param($m) Write-Host "==> $m" -ForegroundColor Cyan }
function Write-Fail { param($m) Write-Host "==> $m" -ForegroundColor Red }

function Get-Headers {
    # GitHub answers 403 for both a rate limit and a private repository, and the
    # two need different advice. Check for a token first so a user with one does
    # not get told to wait an hour they do not need to wait.
    $h = @{ 'User-Agent' = 'irscan-tui-installer' }
    if ($env:GITHUB_TOKEN) { $h['Authorization'] = "Bearer $env:GITHUB_TOKEN" }
    return $h
}

function Get-LatestRelease {
    $headers = Get-Headers
    if ($Version) { return $Version.TrimStart('v') }

    $api = "https://api.github.com/repos/$Repo/releases/latest"
    try {
        $rel = Invoke-RestMethod -Uri $api -Headers $headers -UseBasicParsing
    }
    catch {
        $status = $null
        try { $status = $_.Exception.Response.StatusCode.value__ } catch { }
        if ($status -eq 403 -or $status -eq 429) {
            throw @"
The GitHub API refused the request (HTTP $status).

  - If you have a token, set it first:  `$env:GITHUB_TOKEN = '...'
    (only 'repo' scope is needed, and only for a private repository).
  - Otherwise you have hit the anonymous rate limit, which is 60 requests an
    hour per IP address. Wait, or name a version explicitly:
      -Version v0.1.0
"@
        }
        throw
    }
    return $rel.tag_name.TrimStart('v')
}

function Get-Asset {
    param([string] $Version)
    $headers = Get-Headers
    # Pinned to the tag, not to "latest", so the version printed at the end is the
    # version that was actually downloaded. Two requests to "latest" can be two
    # different builds.
    $api = "https://api.github.com/repos/$Repo/releases/tags/v$Version"
    $rel = Invoke-RestMethod -Uri $api -Headers $headers -UseBasicParsing

    # Matched on a pattern, not on an exact name. The release names its asset after
    # the version - `irscan-tui-v0.1.0-x86_64-pc-windows-msvc.zip` - so an installer
    # that looks for a fixed name works for exactly one release and then tells every
    # user it cannot find a file that is right there. The pattern is the part that
    # does not change: the program, the triple, the extension.
    $pattern = '^irscan-tui-v?[0-9][^-]*.*' + [regex]::Escape($AssetSuffix) + '\.zip$'
    $asset = $rel.assets | Where-Object { $_.name -match $pattern } | Select-Object -First 1
    if (-not $asset) {
        $have = ($rel.assets | ForEach-Object { $_.name }) -join ', '
        throw @"
Release v$Version has no asset matching 'irscan-tui-*-$AssetSuffix.zip'.

It has: $have

If this is a tag with no assets yet, the release workflow may still be running.
Check https://github.com/$Repo/releases
"@
    }
    return $asset
}

# --- main -----------------------------------------------------------------

try {
    $v = Get-LatestRelease
    Write-Step "installing irscan-tui $v"

    $asset = Get-Asset -Version $v

    $temp = Join-Path ([System.IO.Path]::GetTempPath()) ("irscan-tui-" + [guid]::NewGuid().ToString('N'))
    New-Item -ItemType Directory -Path $temp -Force | Out-Null

    $zip = Join-Path $temp $asset.name
    $sha = Join-Path $temp 'SHA256SUMS.txt'

    try {
        Write-Step "downloading $($asset.name)"
        # -UseBasicParsing and TLS 1.2: the default on older Windows PowerShell is
        # still negotiating in a way GitHub rejects, and the error it gives looks
        # like a network problem rather than a protocol one.
        [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
        Invoke-WebRequest -Uri $asset.browser_download_url -OutFile $zip -UseBasicParsing
        Invoke-WebRequest -Uri "https://github.com/$Repo/releases/download/v$v/SHA256SUMS.txt" -OutFile $sha -UseBasicParsing

        Expand-Archive -Path $zip -DestinationPath $temp -Force

        $exe = Join-Path $temp $BinaryName
        if (-not (Test-Path $exe)) {
            throw "the archive did not contain $BinaryName"
        }

        # --- verify the BINARY, and refuse to continue on a mismatch ---------
        # The published checksum is of the executable, not of the archive, so it
        # is checked after expanding and against the extracted file. Verifying the
        # zip against a hash of the exe would compare two different things and
        # always fail - which is the sort of mistake that gets "fixed" by deleting
        # the check.
        $expected = $null
        foreach ($line in (Get-Content $sha)) {
            if ($line -match "^\s*([0-9a-fA-F]{64})\s+\*?$([regex]::Escape($BinaryName))\s*$") {
                $expected = $Matches[1].ToLowerInvariant()
                break
            }
        }
        if (-not $expected) {
            throw "SHA256SUMS.txt does not list $BinaryName. Refusing to install an unverified binary."
        }
        $actual = (Get-FileHash -Path $exe -Algorithm SHA256).Hash.ToLowerInvariant()
        if ($actual -ne $expected) {
            if ($Force) {
                Write-Fail "hash mismatch, continuing because -Force was given"
                Write-Fail "  expected $expected"
                Write-Fail "  actual   $actual"
            }
            else {
                throw @"
The binary does not match the published SHA-256.

  expected $expected
  actual   $actual

Nothing was installed. Either the download was corrupted in transit, or the
published checksum does not describe this file. Do not pass -Force to get past
this without knowing which of those it is.
"@
            }
        }
        Write-Step 'checksum verified'

        # Mark-of-the-Web: a file fetched over the internet keeps a zone
        # identifier, and PowerShell refuses to run a script from a zone it did
        # not create. The binary is not a script, but Exe files carry the same
        # advisory, and an unblocked copy is the difference between installing
        # and installing-then-failing-on-first-run.
        Unblock-File -Path $exe -ErrorAction SilentlyContinue

        New-Item -ItemType Directory -Path $InstallDir -Force | Out-Null
        $target = Join-Path $InstallDir $BinaryName

        if ((Test-Path $target) -and -not $Force) {
            $existing = & $target --version 2>&1
            Write-Step "replacing $($existing -join ' ')"
        }
        Copy-Item -Path $exe -Destination $target -Force
        Write-Step "installed $target"

        # --- PATH -----------------------------------------------------------
        # The user PATH in the registry, not $env:PATH in this process: a
        # one-liner runs in its own process, and a change to its environment dies
        # with it. The registry edit is what the next shell inherits.
        $userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
        $parts = @()
        if ($userPath) { $parts = $userPath -split ';' | Where-Object { $_ } }
        if ($parts -notcontains $InstallDir) {
            $newPath = (@($parts) + $InstallDir) -join ';'
            [Environment]::SetEnvironmentVariable('Path', $newPath, 'User')
            Write-Step 'added to your user PATH (open a new shell to see it)'
        }
        else {
            Write-Step 'already on your user PATH'
        }
        # This process still cannot see it, so the session PATH is fixed too - the
        # user should not have to open a second terminal to try the thing they just
        # installed.
        if (($env:PATH -split ';') -notcontains $InstallDir) {
            $env:PATH = "$env:PATH;$InstallDir"
        }

        Write-Host ''
        Write-Host "  irscan-tui $v is installed." -ForegroundColor Green
        Write-Host ''
        Write-Host '  Try it:'
        Write-Host '    irscan-tui --help'
        Write-Host ''
        Write-Host '  To scan a machine with full coverage, run it elevated. Without'
        Write-Host '  administrator rights the scan cannot see the Security event log,'
        Write-Host '  Prefetch, or the image paths of protected processes:'
        Write-Host '    irscan-tui --elevate'
        Write-Host ''
    }
    finally {
        if (Test-Path $temp) { Remove-Item -Path $temp -Recurse -Force -ErrorAction SilentlyContinue }
    }
}
catch {
    Write-Fail $_.Exception.Message
    exit 1
}
