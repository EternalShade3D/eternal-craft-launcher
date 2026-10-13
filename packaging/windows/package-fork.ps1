<#
.SYNOPSIS
  Build and publish the fork's Windows portable release.

.DESCRIPTION
  The fork ships one artifact: the portable zip. No MSI, no signing, no other platforms.

  Produces in dist/release:
    eternal-craft-launcher-<version>-windows-x64-portable.zip   exe + portable.txt + licenses
    SHA256SUMS.txt

  With -Publish, creates or updates a GitHub release on this fork with those two files.

  Needs: Rust 1.95 or newer (egui 0.36.2 needs it), and gh signed in for -Publish.

.EXAMPLE
  pwsh packaging/windows/package-fork.ps1
  pwsh packaging/windows/package-fork.ps1 -SkipBuild -Publish
#>
param(
  [switch] $SkipBuild,
  [switch] $Publish
)

$ErrorActionPreference = 'Stop'
$Root = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path

# The fork's name is the folder and the asset; the cargo package and the binary stay as upstream.
$Pkg = 'eternal-craft-launcher'
$Title = 'Eternal Craft Launcher'
$CargoPkg = 'artcraft-launcher'

# The version lives in one place: [workspace.package] version in the root Cargo.toml.
$Version = $null
$inPkg = $false
foreach ($line in Get-Content (Join-Path $Root 'Cargo.toml')) {
  if ($line -match '^\s*\[') { $inPkg = ($line.Trim() -eq '[workspace.package]'); continue }
  if ($inPkg -and $line -match '^\s*version\s*=\s*"([^"]+)"') { $Version = $Matches[1]; break }
}
if (-not $Version) { throw 'no version in Cargo.toml' }

if (-not $SkipBuild) {
  Write-Output "==> cargo build --release --locked -p $CargoPkg"
  cargo build --release --locked -p $CargoPkg
  if ($LASTEXITCODE -ne 0) { throw 'build failed' }
}

$Dist = Join-Path $Root 'dist/release'
$Folder = Join-Path $Dist "$Pkg-$Version-windows-x64-portable"
Remove-Item -Recurse -Force $Folder -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $Folder | Out-Null
Copy-Item (Join-Path $Root "target/release/$CargoPkg.exe") (Join-Path $Folder "$Pkg.exe")
foreach ($f in @('portable.txt', 'README.md', 'FORK.md', 'ATTRIBUTION.md', 'LICENSE-APACHE', 'LICENSE-MIT', 'NOTICE')) {
  $src = Join-Path $Root "packaging/windows/$f"
  if (-not (Test-Path $src)) { $src = Join-Path $Root $f }
  if (Test-Path $src) { Copy-Item $src $Folder }
}

$Zip = Join-Path $Dist "$Pkg-$Version-windows-x64-portable.zip"
Remove-Item -Force $Zip -ErrorAction SilentlyContinue
Compress-Archive -Path $Folder -DestinationPath $Zip

$hash = (Get-FileHash $Zip -Algorithm SHA256).Hash.ToLower()
$sums = Join-Path $Dist 'SHA256SUMS.txt'
"$hash  $(Split-Path $Zip -Leaf)" | Set-Content -Encoding ascii $sums
Write-Output "SHA256 $hash  $(Split-Path $Zip -Leaf)"

if ($Publish) {
  $tag = "v$Version"
  $notes = Join-Path $Root 'packaging/windows/fork-release-notes.md'
  if (gh release view $tag --json isDraft --jq .isDraft 2>$null) {
    Write-Output "==> updating release $tag"
    gh release edit $tag --title "$Title v$Version" --notes-file $notes
    gh release upload $tag --clobber $Zip $sums
  } else {
    Write-Output "==> creating release $tag"
    gh release create $tag --title "$Title v$Version" --notes-file $notes $Zip $sums
  }
}
