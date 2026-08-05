param(
    [Parameter(Mandatory = $true)][string]$Version,
    [string]$ChecksumsPath = "SHA256SUMS",
    [string]$OutputDir = "packaging/out"
)

$ErrorActionPreference = "Stop"
$repo = "daddasoft/hostctl"
$baseUrl = "https://github.com/$repo/releases/download/v$Version"

function Get-AssetHash([string]$Asset) {
    $line = Get-Content -LiteralPath $ChecksumsPath | Where-Object {
        $_ -match ("\s+" + [regex]::Escape($Asset) + "$")
    } | Select-Object -First 1
    if (-not $line) { throw "Missing checksum for $Asset" }
    return ($line -split "\s+")[0]
}

New-Item -ItemType Directory -Path $OutputDir -Force | Out-Null

$winX64 = Get-AssetHash "hostctl-windows-x86_64.exe"
$winArm = Get-AssetHash "hostctl-windows-aarch64.exe"
$macX64 = Get-AssetHash "hostctl-macos-x86_64"
$macArm = Get-AssetHash "hostctl-macos-aarch64"
$linuxX64 = Get-AssetHash "hostctl-linux-x86_64"
$linuxArm = Get-AssetHash "hostctl-linux-aarch64"

$scoop = [ordered]@{
    version = $Version
    description = "Safe, cross-platform hosts file management"
    homepage = "https://github.com/$repo"
    license = "MIT"
    architecture = [ordered]@{
        "64bit" = @{ url = "$baseUrl/hostctl-windows-x86_64.exe#/hostctl.exe"; hash = $winX64 }
        "arm64" = @{ url = "$baseUrl/hostctl-windows-aarch64.exe#/hostctl.exe"; hash = $winArm }
    }
    bin = "hostctl.exe"
    checkver = @{ github = "https://github.com/$repo" }
    autoupdate = [ordered]@{
        architecture = [ordered]@{
            "64bit" = @{ url = "https://github.com/$repo/releases/download/v`$version/hostctl-windows-x86_64.exe#/hostctl.exe" }
            "arm64" = @{ url = "https://github.com/$repo/releases/download/v`$version/hostctl-windows-aarch64.exe#/hostctl.exe" }
        }
    }
}
$scoop | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath "$OutputDir/hostctl-scoop.json" -Encoding utf8

@"
class Hostctl < Formula
  desc "Safe, cross-platform hosts file management"
  homepage "https://github.com/$repo"
  version "$Version"
  license "MIT"

  on_macos do
    if Hardware::CPU.arm?
      url "$baseUrl/hostctl-macos-aarch64"
      sha256 "$macArm"
    else
      url "$baseUrl/hostctl-macos-x86_64"
      sha256 "$macX64"
    end
  end

  def install
    bin.install Dir["hostctl-macos-*"][0] => "hostctl"
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/hostctl --version")
  end
end
"@ | Set-Content -LiteralPath "$OutputDir/hostctl.rb" -Encoding utf8

@"
PackageIdentifier: daddasoft.hostctl
PackageVersion: $Version
PackageLocale: en-US
Publisher: daddasoft
PackageName: hostctl
License: MIT
ShortDescription: Safe, cross-platform hosts file management
ManifestType: defaultLocale
ManifestVersion: 1.9.0
"@ | Set-Content -LiteralPath "$OutputDir/daddasoft.hostctl.locale.en-US.yaml" -Encoding utf8

@"
PackageIdentifier: daddasoft.hostctl
PackageVersion: $Version
Installers:
  - Architecture: x64
    InstallerType: portable
    InstallerUrl: $baseUrl/hostctl-windows-x86_64.exe
    InstallerSha256: $winX64
    Commands: [hostctl]
  - Architecture: arm64
    InstallerType: portable
    InstallerUrl: $baseUrl/hostctl-windows-aarch64.exe
    InstallerSha256: $winArm
    Commands: [hostctl]
ManifestType: installer
ManifestVersion: 1.9.0
"@ | Set-Content -LiteralPath "$OutputDir/daddasoft.hostctl.installer.yaml" -Encoding utf8

@"
PackageIdentifier: daddasoft.hostctl
PackageVersion: $Version
DefaultLocale: en-US
ManifestType: version
ManifestVersion: 1.9.0
"@ | Set-Content -LiteralPath "$OutputDir/daddasoft.hostctl.yaml" -Encoding utf8

@"
pkgname=hostctl
pkgver=$Version
pkgrel=1
pkgdesc='Safe, cross-platform hosts file management'
arch=('x86_64' 'aarch64')
url='https://github.com/$repo'
license=('MIT')
source_x86_64=('hostctl::$baseUrl/hostctl-linux-x86_64')
source_aarch64=('hostctl::$baseUrl/hostctl-linux-aarch64')
sha256sums_x86_64=('$linuxX64')
sha256sums_aarch64=('$linuxArm')
package() {
  install -Dm755 hostctl "`$pkgdir/usr/bin/hostctl"
}
"@ | Set-Content -LiteralPath "$OutputDir/PKGBUILD" -Encoding utf8

@"
<?xml version="1.0"?>
<package xmlns="http://schemas.microsoft.com/packaging/2015/06/nuspec.xsd">
  <metadata>
    <id>hostctl</id><version>$Version</version><title>hostctl</title>
    <authors>daddasoft</authors><projectUrl>https://github.com/$repo</projectUrl>
    <license type="expression">MIT</license><requireLicenseAcceptance>false</requireLicenseAcceptance>
    <description>Safe, cross-platform hosts file management</description>
  </metadata>
</package>
"@ | Set-Content -LiteralPath "$OutputDir/hostctl.nuspec" -Encoding utf8

@"
`$ErrorActionPreference = 'Stop'
`$toolsDir = Split-Path -Parent `$MyInvocation.MyCommand.Definition
`$packageArgs = @{
  packageName = 'hostctl'
  fileFullPath = Join-Path `$toolsDir 'hostctl.exe'
  url64bit = '$baseUrl/hostctl-windows-x86_64.exe'
  checksum64 = '$winX64'
  checksumType64 = 'sha256'
}
Get-ChocolateyWebFile @packageArgs
"@ | Set-Content -LiteralPath "$OutputDir/chocolateyinstall.ps1" -Encoding utf8
