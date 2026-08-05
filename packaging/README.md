# Package publication

Release builds generate package-manager-ready metadata from the signed release
asset checksums:

```powershell
pwsh scripts/generate-packages.ps1 -Version 0.4.0
```

The generated `packaging/out` directory contains Homebrew, Scoop, WinGet,
Chocolatey, and AUR submission files. GitHub Actions also creates Debian
packages for amd64 and arm64. Publishing to external registries requires the
maintainer credentials for each registry; Cargo publishing uses
`cargo publish` after the release commit is tagged.
