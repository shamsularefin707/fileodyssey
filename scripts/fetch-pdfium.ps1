# Downloads the pinned PDFium build into .\pdfium\win-x64\
$ErrorActionPreference = "Stop"
$Version = "8125"
$Root = Split-Path -Parent $PSScriptRoot
$Dest = Join-Path $Root "pdfium\win-x64"
New-Item -ItemType Directory -Force -Path $Dest | Out-Null
$Tgz = Join-Path $env:TEMP "pdfium-win-x64.tgz"
Invoke-WebRequest "https://github.com/bblanchon/pdfium-binaries/releases/download/chromium%2F$Version/pdfium-win-x64.tgz" -OutFile $Tgz
tar -xzf $Tgz -C $Dest
Write-Host "PDFium $Version (win-x64) -> $Dest"
