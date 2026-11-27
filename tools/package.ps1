# Package a release build into dist\ (used by CI and for local releases).
#   powershell -ExecutionPolicy Bypass -File tools\package.ps1 -Version 0.4.0
# Expects `cargo build --release` to have run. Produces:
#   dist\moza-xinput-bridge-<Version>-windows-x64.zip  app + diagnostics + docs
#   dist\moza-xinput-bridge-<Version>.exe              the app alone
#   dist\SHA256SUMS.txt
param([Parameter(Mandatory)][string]$Version)
$ErrorActionPreference = 'Stop'

$root = Split-Path $PSScriptRoot -Parent
$rel  = Join-Path $root 'target\release'
$dist = Join-Path $root 'dist'
$name = "moza-xinput-bridge-$Version"

$bins = 'moza-xinput-bridge.exe', 'hid-probe.exe', 'pedal_log.exe'
foreach ($b in $bins) {
  if (-not (Test-Path (Join-Path $rel $b))) { throw "missing $b - run cargo build --release first" }
}

if (Test-Path $dist) { Remove-Item -Recurse -Force $dist }
$stage = Join-Path $dist $name
New-Item -ItemType Directory -Force $stage | Out-Null
foreach ($b in $bins) { Copy-Item (Join-Path $rel $b) $stage }
Copy-Item (Join-Path $root 'README.md'), (Join-Path $root 'LICENSE') $stage
New-Item -ItemType Directory -Force (Join-Path $stage 'docs') | Out-Null
Copy-Item (Join-Path $root 'docs\FFB.md') (Join-Path $stage 'docs')

Compress-Archive -Path $stage -DestinationPath (Join-Path $dist "$name-windows-x64.zip") -Force
Copy-Item (Join-Path $rel 'moza-xinput-bridge.exe') (Join-Path $dist "$name.exe")
Remove-Item -Recurse -Force $stage

Get-ChildItem $dist -File | ForEach-Object {
  "{0}  {1}" -f (Get-FileHash $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant(), $_.Name
} | Set-Content (Join-Path $dist 'SHA256SUMS.txt') -Encoding ascii

Get-ChildItem $dist | Format-Table Name, Length -AutoSize
