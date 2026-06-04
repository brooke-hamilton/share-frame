<#
.SYNOPSIS
    Downloads and installs the latest Share Frame release for the current architecture.

.DESCRIPTION
    Queries the GitHub Releases API for the latest Share Frame release, downloads the
    matching archive for this PC's architecture (x64 or arm64), verifies its SHA-256
    checksum, and extracts share-frame.exe into the install directory. Optionally adds
    the install directory to the user PATH and creates a Start Menu shortcut.

.PARAMETER Version
    Install a specific version tag (for example 'v0.1.0') instead of the latest release.

.PARAMETER InstallDir
    Target directory for share-frame.exe. Defaults to
    "$env:LOCALAPPDATA\Programs\share-frame".

.PARAMETER AddToPath
    Add the install directory to the current user's PATH environment variable.

.PARAMETER NoShortcut
    Skip creating a Start Menu shortcut.

.EXAMPLE
    .\install.ps1

.EXAMPLE
    irm https://raw.githubusercontent.com/brooke-hamilton/share-frame/main/scripts/install.ps1 | iex

.EXAMPLE
    .\install.ps1 -Version v0.1.0 -AddToPath
#>
[CmdletBinding()]
param(
    [string]$Version,
    [string]$InstallDir = (Join-Path $env:LOCALAPPDATA "Programs\share-frame"),
    [switch]$AddToPath,
    [switch]$NoShortcut
)

$ErrorActionPreference = "Stop"
$Repo = "brooke-hamilton/share-frame"

function Get-Arch {
    switch ($env:PROCESSOR_ARCHITECTURE) {
        "ARM64" { return "arm64" }
        "AMD64" { return "x64" }
        default {
            throw "Unsupported architecture: $env:PROCESSOR_ARCHITECTURE. Share Frame ships x64 and arm64 builds only."
        }
    }
}

function Get-Release {
    param([string]$Tag)
    $headers = @{ "User-Agent" = "share-frame-installer"; "Accept" = "application/vnd.github+json" }
    if ($Tag) {
        $url = "https://api.github.com/repos/$Repo/releases/tags/$Tag"
    } else {
        $url = "https://api.github.com/repos/$Repo/releases/latest"
    }
    $label = if ([string]::IsNullOrEmpty($Tag)) { "latest" } else { $Tag }
    try {
        return Invoke-RestMethod -Uri $url -Headers $headers
    } catch {
        throw "Failed to query release '$label' from $Repo. $($_.Exception.Message)"
    }
}

$arch = Get-Arch
Write-Host "Detected architecture: $arch"

$release = Get-Release -Tag $Version
$tag = $release.tag_name
Write-Host "Installing Share Frame $tag"

$zipName = "share-frame-$($tag.TrimStart('v'))-$arch.zip"
$zipAsset = $release.assets | Where-Object { $_.name -eq $zipName }
$shaAsset = $release.assets | Where-Object { $_.name -eq "$zipName.sha256" }

if (-not $zipAsset) {
    $available = ($release.assets | ForEach-Object { $_.name }) -join ", "
    throw "Release $tag has no asset named '$zipName'. Available assets: $available"
}

$tempDir = Join-Path ([System.IO.Path]::GetTempPath()) ("share-frame-" + [guid]::NewGuid())
New-Item -ItemType Directory -Force -Path $tempDir | Out-Null
try {
    $zipPath = Join-Path $tempDir $zipName
    Write-Host "Downloading $zipName ..."
    Invoke-WebRequest -Uri $zipAsset.browser_download_url -OutFile $zipPath -UseBasicParsing

    if ($shaAsset) {
        Write-Host "Verifying SHA-256 checksum ..."
        $shaPath = Join-Path $tempDir "$zipName.sha256"
        Invoke-WebRequest -Uri $shaAsset.browser_download_url -OutFile $shaPath -UseBasicParsing
        $expected = ((Get-Content $shaPath -Raw).Trim() -split '\s+')[0].ToLower()
        $actual = (Get-FileHash -Algorithm SHA256 $zipPath).Hash.ToLower()
        if ($expected -ne $actual) {
            throw "Checksum mismatch. Expected $expected but got $actual. Aborting."
        }
        Write-Host "Checksum verified."
    } else {
        Write-Warning "No .sha256 asset found for $zipName; skipping checksum verification."
    }

    Write-Host "Extracting to $InstallDir ..."
    New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
    $extractDir = Join-Path $tempDir "extract"
    Expand-Archive -Path $zipPath -DestinationPath $extractDir -Force
    $exe = Get-ChildItem -Path $extractDir -Filter "share-frame.exe" -Recurse | Select-Object -First 1
    if (-not $exe) {
        throw "share-frame.exe was not found inside $zipName."
    }
    Copy-Item $exe.FullName -Destination (Join-Path $InstallDir "share-frame.exe") -Force
    Copy-Item (Join-Path $extractDir "LICENSE") -Destination $InstallDir -Force -ErrorAction SilentlyContinue
} finally {
    Remove-Item -Path $tempDir -Recurse -Force -ErrorAction SilentlyContinue
}

$exePath = Join-Path $InstallDir "share-frame.exe"
Write-Host "Installed: $exePath"

if ($AddToPath) {
    $userPath = [Environment]::GetEnvironmentVariable("Path", "User")
    $entries = $userPath -split ';' | Where-Object { $_ -ne "" }
    if ($entries -notcontains $InstallDir) {
        $newPath = (($entries + $InstallDir) -join ';')
        [Environment]::SetEnvironmentVariable("Path", $newPath, "User")
        Write-Host "Added $InstallDir to your user PATH. Restart your terminal to use 'share-frame'."
    } else {
        Write-Host "$InstallDir is already on your user PATH."
    }
}

if (-not $NoShortcut) {
    $startMenu = Join-Path $env:APPDATA "Microsoft\Windows\Start Menu\Programs"
    $shortcut = Join-Path $startMenu "Share Frame.lnk"
    $wsh = New-Object -ComObject WScript.Shell
    $lnk = $wsh.CreateShortcut($shortcut)
    $lnk.TargetPath = $exePath
    $lnk.WorkingDirectory = $InstallDir
    $lnk.Description = "Share exactly what you mean."
    $lnk.Save()
    Write-Host "Created Start Menu shortcut: $shortcut"
}

Write-Host ""
Write-Host "Share Frame $tag is ready. Launch it from the Start Menu or run:" -ForegroundColor Green
Write-Host "  `"$exePath`"" -ForegroundColor Green
