# generate-icon.ps1
# Renders assets/icons/logo.svg to multiple PNG sizes using resvg,
# then packs them into a single .ico file.
#
# Prerequisites: resvg (cargo install resvg)
# Usage: .\generate-icon.ps1

$ErrorActionPreference = "Stop"

$repoRoot = Split-Path $PSScriptRoot -Parent
$svgPath = Join-Path $repoRoot "assets\icons\logo.svg"
$icoPath = Join-Path $repoRoot "assets\icons\share-frame.ico"
$tempDir = Join-Path $repoRoot "assets\icons\.pngtmp"

if (-not (Test-Path $svgPath)) {
    Write-Error "SVG not found: $svgPath"
    exit 1
}

if (-not (Get-Command resvg -ErrorAction SilentlyContinue)) {
    Write-Error "resvg not found. Install with: cargo install resvg"
    exit 1
}

# Render SVG to PNGs at each required size
$sizes = @(16, 32, 48, 256)
New-Item -ItemType Directory -Path $tempDir -Force | Out-Null

foreach ($sz in $sizes) {
    $outPng = Join-Path $tempDir "$sz.png"
    resvg $svgPath $outPng --width $sz --height $sz
    if ($LASTEXITCODE -ne 0) {
        Write-Error "resvg failed for size $sz"
        exit 1
    }
}

# Pack PNGs into ICO format
$ms = New-Object System.IO.MemoryStream
$writer = New-Object System.IO.BinaryWriter($ms)

# ICO header
$writer.Write([UInt16]0)             # Reserved
$writer.Write([UInt16]1)             # Type (1 = ICO)
$writer.Write([UInt16]$sizes.Count)  # Image count

# Read PNG data
$pngData = @()
foreach ($sz in $sizes) {
    $pngPath = Join-Path $tempDir "$sz.png"
    $pngData += ,([System.IO.File]::ReadAllBytes($pngPath))
}

# Directory entries (6-byte header + 16 bytes per entry precedes image data)
$dataOffset = 6 + ($sizes.Count * 16)

for ($i = 0; $i -lt $sizes.Count; $i++) {
    $sz = $sizes[$i]
    $data = $pngData[$i]

    $writer.Write([byte]$(if ($sz -eq 256) { 0 } else { $sz }))  # Width (0 = 256)
    $writer.Write([byte]$(if ($sz -eq 256) { 0 } else { $sz }))  # Height
    $writer.Write([byte]0)       # Color palette count
    $writer.Write([byte]0)       # Reserved
    $writer.Write([UInt16]1)     # Color planes
    $writer.Write([UInt16]32)    # Bits per pixel
    $writer.Write([UInt32]$data.Length)   # Image data size
    $writer.Write([UInt32]$dataOffset)    # Offset to image data

    $dataOffset += $data.Length
}

# Image data
foreach ($data in $pngData) {
    $writer.Write($data)
}

$writer.Flush()
[System.IO.File]::WriteAllBytes($icoPath, $ms.ToArray())
$writer.Dispose()
$ms.Dispose()

# Cleanup temp PNGs
Remove-Item $tempDir -Recurse -Force

Write-Host "Created $icoPath with sizes: $($sizes -join ', ')px"
