param(
  [string]$Version = "",
  [string]$InstallDir = "",
  [switch]$AllowPrerelease,
  [string]$SkillRef = "",
  [string]$CfrUrl = ""
)

$ErrorActionPreference = "Stop"

$Repo = "Groos-dev/class-finder"

function Get-LatestTag {
  $uri = "https://api.github.com/repos/$Repo/releases?per_page=20"
  $resp = Invoke-RestMethod -Uri $uri -Headers @{ "User-Agent" = "class-finder-installer" }
  if ($AllowPrerelease) {
    return $resp[0].tag_name
  }
  foreach ($r in $resp) {
    $tag = $r.tag_name
    if ($tag -notmatch 'beta|alpha|rc') {
      return $tag
    }
  }
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
$sumUrl = "https://github.com/$Repo/releases/download/$Version/SHA256SUMS"

if ([string]::IsNullOrWhiteSpace($InstallDir)) {
  $InstallDir = Join-Path $env:LOCALAPPDATA "Programs\class-finder\bin"
}

$tmp = Join-Path $env:TEMP ("class-finder-install-" + [guid]::NewGuid().ToString("n"))
New-Item -ItemType Directory -Force -Path $tmp | Out-Null

Write-Host "Downloading $url"
$zipPath = Join-Path $tmp $asset
Invoke-WebRequest -Uri $url -OutFile $zipPath -UseBasicParsing

$sumPath = Join-Path $tmp "SHA256SUMS"
Invoke-WebRequest -Uri $sumUrl -OutFile $sumPath -UseBasicParsing
$expected = (Select-String -Path $sumPath -Pattern ("  " + [regex]::Escape($asset) + "$") | Select-Object -First 1).Line.Split(" ", [System.StringSplitOptions]::RemoveEmptyEntries)[0].ToLower()
if ([string]::IsNullOrWhiteSpace($expected)) {
  throw "Missing checksum for $asset in SHA256SUMS"
}
$actual = (Get-FileHash $zipPath -Algorithm SHA256).Hash.ToLower()
if ($actual -ne $expected) {
  throw "SHA256 mismatch for $asset`nexpected: $expected`nactual:   $actual"
}

$unpack = Join-Path $tmp "unpack"
New-Item -ItemType Directory -Force -Path $unpack | Out-Null
Expand-Archive -Path $zipPath -DestinationPath $unpack -Force

New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
Copy-Item -Force (Join-Path $unpack "bin\class-finder.exe") (Join-Path $InstallDir "class-finder.exe")

$classFinderHome = Join-Path $env:LOCALAPPDATA "class-finder"
$cfrPath = Join-Path $classFinderHome "tools\cfr.jar"
if (-not (Test-Path $cfrPath)) {
  if ([string]::IsNullOrWhiteSpace($CfrUrl)) {
    $CfrUrl = "https://github.com/leibnitz27/cfr/releases/download/0.152/cfr-0.152.jar"
  }
  New-Item -ItemType Directory -Force -Path (Split-Path -Parent $cfrPath) | Out-Null
  Write-Host "Downloading CFR $CfrUrl"
  Invoke-WebRequest -Uri $CfrUrl -OutFile $cfrPath -UseBasicParsing
}

$skillDir = Join-Path $env:USERPROFILE ".claude\skill\find-class"
New-Item -ItemType Directory -Force -Path $skillDir | Out-Null
if ([string]::IsNullOrWhiteSpace($SkillRef)) {
  $SkillRef = $Version
}
$skillUrl = "https://raw.githubusercontent.com/$Repo/$SkillRef/.claude/skill/find-class/SKILL.md"
$skillFallbackUrl = "https://raw.githubusercontent.com/$Repo/main/.claude/skill/find-class/SKILL.md"
try {
  Invoke-WebRequest -Uri $skillUrl -OutFile (Join-Path $skillDir "SKILL.md") -UseBasicParsing
} catch {
  Invoke-WebRequest -Uri $skillFallbackUrl -OutFile (Join-Path $skillDir "SKILL.md") -UseBasicParsing
}

Remove-Item -Recurse -Force $tmp

Write-Host "Installed: $(Join-Path $InstallDir 'class-finder.exe')"
Write-Host "Installed: $(Join-Path $skillDir 'SKILL.md')"
Write-Host "Add to PATH if needed: $InstallDir"
Write-Host "Try: class-finder --help"
