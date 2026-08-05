# install.ps1 — hostctl installer for Windows (PowerShell)
# Usage: irm https://raw.githubusercontent.com/daddasoft/hostctl/main/install.ps1 | iex
#
# Options (set before piping):
#   $env:HOSTCTL_INSTALL_DIR = "C:\Tools"   (default: $env:USERPROFILE\.hostctl\bin)

param(
    [string]$InstallDir = ""
)

$ErrorActionPreference = "Stop"

$REPO       = "daddasoft/hostctl"
$BIN_NAME   = "hostctl"
$ASSET_NAME = "hostctl-windows-x86_64.exe"

# ── Colour helpers ─────────────────────────────────────────────────────────────
function Write-Info    ($msg) { Write-Host "  info  " -ForegroundColor Cyan    -NoNewline; Write-Host $msg }
function Write-Ok      ($msg) { Write-Host "  ok    " -ForegroundColor Green   -NoNewline; Write-Host $msg }
function Write-Warn    ($msg) { Write-Host "  warn  " -ForegroundColor Yellow  -NoNewline; Write-Host $msg }
function Write-Err     ($msg) { Write-Host "  error " -ForegroundColor Red     -NoNewline; Write-Host $msg; exit 1 }

# ── Resolve install directory ──────────────────────────────────────────────────
if ($InstallDir -eq "" -and $env:HOSTCTL_INSTALL_DIR) {
    $InstallDir = $env:HOSTCTL_INSTALL_DIR
}
if ($InstallDir -eq "") {
    $InstallDir = Join-Path $env:USERPROFILE ".hostctl\bin"
}

# ── Fetch latest release tag from GitHub API ───────────────────────────────────
Write-Info "Fetching latest release from github.com/$REPO …"

try {
    $release = Invoke-RestMethod "https://api.github.com/repos/$REPO/releases/latest"
} catch {
    Write-Err "Could not reach GitHub API. Check your internet connection.`n  $_"
}

$tag = $release.tag_name
if (-not $tag) { Write-Err "Could not determine the latest release tag." }

Write-Info "Latest version: $tag"

$downloadUrl = "https://github.com/$REPO/releases/download/$tag/$ASSET_NAME"
$checksumsUrl = "https://github.com/$REPO/releases/download/$tag/SHA256SUMS"

# ── Download ───────────────────────────────────────────────────────────────────
$tmpDir  = Join-Path $env:TEMP "addhost-install"
$tmpExe  = Join-Path $tmpDir "$BIN_NAME.exe"
$tmpSums = Join-Path $tmpDir "SHA256SUMS"

if (Test-Path $tmpDir) { Remove-Item $tmpDir -Recurse -Force }
New-Item -ItemType Directory -Path $tmpDir | Out-Null

Write-Info "Downloading $ASSET_NAME …"
try {
    $wc = New-Object System.Net.WebClient
    $wc.DownloadFile($downloadUrl, $tmpExe)
    $wc.DownloadFile($checksumsUrl, $tmpSums)
} catch {
    Write-Err "Download failed.`n  URL : $downloadUrl`n  $_"
}

# ── Verify release checksum ────────────────────────────────────────────────────
$checksumLine = Get-Content $tmpSums | Where-Object {
    $_ -match ("\s+" + [regex]::Escape($ASSET_NAME) + "$")
} | Select-Object -First 1

if (-not $checksumLine) {
    Write-Err "Release checksums do not contain $ASSET_NAME."
}

$expectedHash = ($checksumLine -split "\s+")[0].ToLowerInvariant()
$actualHash = (Get-FileHash $tmpExe -Algorithm SHA256).Hash.ToLowerInvariant()
if ($actualHash -ne $expectedHash) {
    Write-Err "Checksum verification failed for $ASSET_NAME. The download was not installed."
}
Write-Ok "Verified SHA-256: $actualHash"

# ── Install ────────────────────────────────────────────────────────────────────
if (-not (Test-Path $InstallDir)) {
    New-Item -ItemType Directory -Path $InstallDir | Out-Null
}

$dest = Join-Path $InstallDir "$BIN_NAME.exe"
Move-Item $tmpExe $dest -Force
Remove-Item $tmpDir -Recurse -Force

Write-Ok "Installed $BIN_NAME $tag → $dest"

# ── Add to PATH (user scope, persists across sessions) ─────────────────────────
$userPath = [System.Environment]::GetEnvironmentVariable("PATH", "User")
if ($userPath -notlike "*$InstallDir*") {
    [System.Environment]::SetEnvironmentVariable(
        "PATH",
        "$userPath;$InstallDir",
        "User"
    )
    Write-Ok "Added $InstallDir to your user PATH."
    Write-Warn "Restart your terminal (or open a new tab) for PATH to take effect."
} else {
    Write-Info "$InstallDir is already in PATH."
}

# ── Verify ─────────────────────────────────────────────────────────────────────
$version = & $dest --version 2>&1
Write-Ok "Verified: $version"
Write-Host ""
Write-Host "  Run " -NoNewline
Write-Host "hostctl --help" -ForegroundColor Cyan -NoNewline
Write-Host " to get started."
Write-Host ""
