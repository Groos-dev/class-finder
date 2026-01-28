param(
  [string]$Version = "",
  [string]$InstallDir = ""
)

$ErrorActionPreference = "Stop"

$Repo = "Groos-dev/class-finder"

function Get-LatestTag {
  $uri = "https://api.github.com/repos/$Repo/releases?per_page=1"
  $resp = Invoke-RestMethod -Uri $uri -Headers @{ "User-Agent" = "class-finder-installer" }
  return $resp[0].tag_name
}

if ([string]::IsNullOrWhiteSpace($Version)) {
  $Version = Get-LatestTag
}

if ([string]::IsNullOrWhiteSpace($Version)) {
  throw "Failed to resolve latest version; set -Version v0.0.1-beta"
}

$arch = $env:PROCESSOR_ARCHITECTURE
if ($arch -eq "AMD64") {
  $arch = "x86_64"
} else {
  throw "Unsupported architecture: $arch"
}

$asset = "class-finder-windows-$arch.zip"
$url = "https://github.com/$Repo/releases/download/$Version/$asset"

if ([string]::IsNullOrWhiteSpace($InstallDir)) {
  $InstallDir = Join-Path $env:LOCALAPPDATA "Programs\class-finder\bin"
}

$tmp = Join-Path $env:TEMP ("class-finder-install-" + [guid]::NewGuid().ToString("n"))
New-Item -ItemType Directory -Force -Path $tmp | Out-Null

Write-Host "Downloading $url"
$zipPath = Join-Path $tmp $asset
Invoke-WebRequest -Uri $url -OutFile $zipPath -UseBasicParsing

$unpack = Join-Path $tmp "unpack"
New-Item -ItemType Directory -Force -Path $unpack | Out-Null
Expand-Archive -Path $zipPath -DestinationPath $unpack -Force

New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
Copy-Item -Force (Join-Path $unpack "bin\class-finder.exe") (Join-Path $InstallDir "class-finder.exe")

Remove-Item -Recurse -Force $tmp

Write-Host "Installed: $(Join-Path $InstallDir 'class-finder.exe')"
Write-Host "Add to PATH if needed: $InstallDir"
Write-Host "Try: class-finder --help"
