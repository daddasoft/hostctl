param([string]$InstallDir = "")

$ErrorActionPreference = "Stop"
if (-not $InstallDir) {
    $InstallDir = if ($env:HOSTCTL_INSTALL_DIR) {
        $env:HOSTCTL_INSTALL_DIR
    } else {
        Join-Path $env:USERPROFILE ".hostctl\bin"
    }
}

$binary = Join-Path $InstallDir "hostctl.exe"
if (Test-Path $binary) {
    Remove-Item -LiteralPath $binary -Force
}

$userPath = [Environment]::GetEnvironmentVariable("PATH", "User")
$parts = $userPath -split ";" | Where-Object { $_ -and $_ -ne $InstallDir }
[Environment]::SetEnvironmentVariable("PATH", ($parts -join ";"), "User")
Write-Host "Uninstalled hostctl from $InstallDir"
