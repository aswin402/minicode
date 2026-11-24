# ==============================================================================
# minicode — Windows Single-Script PowerShell Installer
# Repository: https://github.com/aswin402/minicode
#
# Quick Install (Windows PowerShell / Windows Terminal):
#   irm https://raw.githubusercontent.com/aswin402/minicode/main/install.ps1 | iex
#
# Optional Environment Variables:
#   $env:MINICODE_VERSION     = "v0.5.86"   # Pin a specific release tag
#   $env:MINICODE_INSTALL_DIR = "C:\tools"  # Custom install directory
#   $env:MINICODE_FROM_SOURCE = "1"         # Build from source using cargo
#   $env:MINICODE_NO_PATH     = "1"         # Skip updating User PATH
#   $env:MINICODE_DRY_RUN     = "1"         # Print resolved URL without installing
# ==============================================================================

$ErrorActionPreference = 'Stop'

# Ensure TLS 1.2+ on Windows PowerShell 5.1
try {
    [Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12
} catch {
    # Ignore on newer .NET runtimes where TLS 1.2/1.3 is negotiated automatically
}

$Repo = "aswin402/minicode"
$InstallDir = if ($env:MINICODE_INSTALL_DIR) { $env:MINICODE_INSTALL_DIR } else { Join-Path $env:LOCALAPPDATA "Programs\minicode\bin" }
$RequestedVersion = $env:MINICODE_VERSION
$FromSource = ($env:MINICODE_FROM_SOURCE -eq "1")
$NoPath = ($env:MINICODE_NO_PATH -eq "1")
$DryRun = ($env:MINICODE_DRY_RUN -eq "1")

Write-Host ""
Write-Host "   /\_/\     ___ ___                           _" -ForegroundColor Magenta
Write-Host "  ( o.o )   |   Y   | _   ___  _   ___  ___  _| | ___" -ForegroundColor Magenta
Write-Host "   > ^ <    |.      || | |   || | |  _|| . || . || -_|" -ForegroundColor Magenta
Write-Host "            |. \_/  ||_| |_|_||_| |___||___||___||___|" -ForegroundColor Magenta
Write-Host ""
Write-Host "  Installing minicode ($Repo) for Windows..." -ForegroundColor Cyan
Write-Host "  ----------------------------------------------------" -ForegroundColor DarkGray

$StartTime = Get-Date

# Detect Architecture
$Arch = $env:PROCESSOR_ARCHITECTURE
if (-not $Arch) { $Arch = "AMD64" }
$Target = switch ($Arch.ToUpperInvariant()) {
    "AMD64" { "x86_64-pc-windows-msvc" }
    "X86_64" { "x86_64-pc-windows-msvc" }
    "ARM64" { "x86_64-pc-windows-msvc" } # Runs seamlessly on Windows 11 ARM64 via prism emulation
    default {
        Write-Host "  [x] Unsupported Windows architecture: $Arch" -ForegroundColor Red
        exit 1
    }
}

Write-Host ("    {0,-14} {1}" -f "target", $Target) -ForegroundColor Cyan
Write-Host ("    {0,-14} {1}" -f "install dir", $InstallDir) -ForegroundColor Gray

function Install-FromCargo {
    $cargo = Get-Command cargo -ErrorAction SilentlyContinue
    if (-not $cargo) {
        Write-Host "  [x] Prebuilt binary is unavailable and 'cargo' is not installed." -ForegroundColor Red
        Write-Host "      Install Rust via https://rustup.rs or check https://github.com/$Repo/releases" -ForegroundColor DarkGray
        exit 1
    }
    Write-Host "  [*] Building minicode from source via cargo..." -ForegroundColor Yellow
    & cargo install --git "https://github.com/$Repo.git" --locked minicode
    if ($LASTEXITCODE -ne 0) {
        Write-Host "  [x] cargo install failed with exit code $LASTEXITCODE" -ForegroundColor Red
        exit $LASTEXITCODE
    }
    $elapsed = [int]((Get-Date) - $StartTime).TotalSeconds
    Write-Host "  [+] minicode built and installed via cargo in ${elapsed}s" -ForegroundColor Green
    exit 0
}

if ($FromSource) {
    if ($DryRun) {
        Write-Host "    mode           cargo --from-source (dry-run)" -ForegroundColor Yellow
        exit 0
    }
    Install-FromCargo
}

function Resolve-LatestTag {
    # 1. Resolve via releases/latest redirect (avoids unauthenticated GitHub API rate limit)
    try {
        $req = [System.Net.HttpWebRequest]::Create("https://github.com/$Repo/releases/latest")
        $req.Method = "HEAD"
        $req.AllowAutoRedirect = $true
        $req.UserAgent = "minicode-installer"
        $resp = $req.GetResponse()
        $finalUri = $resp.ResponseUri.AbsoluteUri
        $resp.Close()
        $tag = ($finalUri.TrimEnd('/') -split '/')[-1]
        if ($tag -and $tag -ne "latest" -and $tag -ne "releases") {
            return $tag
        }
    } catch {
        # Fall back to GitHub REST API below
    }

    # 2. Fallback to GitHub Releases REST API
    try {
        $headers = @{ "User-Agent" = "minicode-installer" }
        $release = Invoke-RestMethod -Uri "https://api.github.com/repos/$Repo/releases/latest" -Headers $headers
        if ($release.tag_name) {
            return $release.tag_name
        }
    } catch {
        return $null
    }
    return $null
}

if ($RequestedVersion) {
    $LatestTag = if ($RequestedVersion.StartsWith("v")) { $RequestedVersion } else { "v$RequestedVersion" }
} else {
    $LatestTag = Resolve-LatestTag
}

if (-not $LatestTag) {
    if ($DryRun) {
        $LatestTag = "latest"
    } else {
        Write-Host "  [!] Could not resolve latest release tag — falling back to cargo..." -ForegroundColor Yellow
        Install-FromCargo
    }
}

$AssetName = "minicode-$Target.zip"
$DownloadUrl = "https://github.com/$Repo/releases/download/$LatestTag/$AssetName"
$ShaUrl = "$DownloadUrl.sha256"

Write-Host ("    {0,-14} {1}" -f "release", $LatestTag) -ForegroundColor Magenta

if ($DryRun) {
    Write-Host ("    {0,-14} {1}" -f "asset", $AssetName) -ForegroundColor Cyan
    Write-Host ("    {0,-14} {1}" -f "url", $DownloadUrl) -ForegroundColor DarkGray
    Write-Host "  [+] Dry run complete (no files modified)." -ForegroundColor Green
    exit 0
}

$TempDir = Join-Path ([System.IO.Path]::GetTempPath()) ("minicode-install-" + [System.Guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Force -Path $TempDir | Out-Null

try {
    $ZipPath = Join-Path $TempDir $AssetName
    $ShaPath = Join-Path $TempDir "$AssetName.sha256"

    Write-Host "  [*] Downloading $AssetName..." -ForegroundColor Cyan
    try {
        Invoke-WebRequest -Uri $DownloadUrl -OutFile $ZipPath -UseBasicParsing -Headers @{ "User-Agent" = "minicode-installer" }
    } catch {
        Write-Host "  [!] Prebuilt Windows archive not yet published for $LatestTag." -ForegroundColor Yellow
        Install-FromCargo
    }

    # Verify SHA-256 checksum if published
    try {
        Invoke-WebRequest -Uri $ShaUrl -OutFile $ShaPath -UseBasicParsing -Headers @{ "User-Agent" = "minicode-installer" }
        $shaLine = (Get-Content -Path $ShaPath -TotalCount 1).Trim()
        $expected = ($shaLine -split '\s+')[0].ToLowerInvariant()
        if ($expected.Length -eq 64) {
            $actual = (Get-FileHash -Path $ZipPath -Algorithm SHA256).Hash.ToLowerInvariant()
            if ($expected -ne $actual) {
                Write-Host "  [x] SHA-256 checksum mismatch!" -ForegroundColor Red
                Write-Host "      expected: $expected" -ForegroundColor Red
                Write-Host "      actual:   $actual" -ForegroundColor Red
                exit 1
            }
            Write-Host ("    {0,-14} {1}" -f "checksum", "sha256 verified") -ForegroundColor Green
        }
    } catch {
        # Optional checksum file unavailable
    }

    $ExtractDir = Join-Path $TempDir "extract"
    Expand-Archive -Path $ZipPath -DestinationPath $ExtractDir -Force

    New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null

    $InstalledBins = @()
    foreach ($binName in @("minicode.exe", "minidoctor.exe", "minibrowse.exe")) {
        $found = Get-ChildItem -Path $ExtractDir -Filter $binName -Recurse -File | Select-Object -First 1
        if ($found) {
            Copy-Item -Path $found.FullName -Destination (Join-Path $InstallDir $binName) -Force
            $InstalledBins += $binName
        }
    }

    if ($InstalledBins.Count -eq 0) {
        Write-Host "  [x] Archive did not contain minicode.exe." -ForegroundColor Red
        exit 1
    }

    $elapsed = [int]((Get-Date) - $StartTime).TotalSeconds
    $binSizeMb = [math]::Round((Get-Item (Join-Path $InstallDir "minicode.exe")).Length / 1MB, 1)

    Write-Host ""
    Write-Host "  [+] minicode $LatestTag installed in ${elapsed}s" -ForegroundColor Green
    Write-Host ("    {0,-14} {1}" -f "binaries", ($InstalledBins -join ", ")) -ForegroundColor Green
    Write-Host ("    {0,-14} {1} MB" -f "binary size", $binSizeMb) -ForegroundColor Cyan
    Write-Host ("    {0,-14} {1}" -f "installed to", $InstallDir) -ForegroundColor Green

    # Persist InstallDir in User PATH (idempotent) + update current session PATH
    if (-not $NoPath) {
        $userPath = [Environment]::GetEnvironmentVariable("Path", "User")
        $entries = @()
        if ($userPath) {
            $entries = $userPath -split ';' | Where-Object { $_ -ne '' }
        }
        $normalizedTarget = $InstallDir.TrimEnd('\').ToLowerInvariant()
        $alreadyInUserPath = $entries | Where-Object { $_.TrimEnd('\').ToLowerInvariant() -eq $normalizedTarget }

        if (-not $alreadyInUserPath) {
            $newUserPath = if ($userPath) { "$userPath;$InstallDir" } else { $InstallDir }
            [Environment]::SetEnvironmentVariable("Path", $newUserPath, "User")
            Write-Host ("    {0,-14} added to User PATH" -f "PATH") -ForegroundColor Green
        }

        $sessionEntries = $env:Path -split ';' | Where-Object { $_ -ne '' }
        $alreadyInSession = $sessionEntries | Where-Object { $_.TrimEnd('\').ToLowerInvariant() -eq $normalizedTarget }
        if (-not $alreadyInSession) {
            $env:Path = "$InstallDir;$env:Path"
        }
    }

    Write-Host "  ----------------------------------------------------" -ForegroundColor DarkGray
    try {
        $ver = & (Join-Path $InstallDir "minicode.exe") --version 2>$null
        if ($ver) {
            Write-Host ("    {0,-14} {1}" -f "verified", $ver) -ForegroundColor Cyan
        }
    } catch { }

    Write-Host "  Next steps:" -ForegroundColor Green
    Write-Host "    1. Run 'minicode setup'   - configure your LLM provider & API key" -ForegroundColor White
    Write-Host "    2. Run 'minicode'         - launch the interactive TUI" -ForegroundColor White
    Write-Host "    3. Run 'minicode doctor'  - audit codebase health (.minicode\minidoctor\NEEDEDFIX.md)" -ForegroundColor White
    Write-Host ""
} finally {
    if (Test-Path $TempDir) {
        Remove-Item -Path $TempDir -Recurse -Force -ErrorAction SilentlyContinue
    }
}
