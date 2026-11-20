# Pinned pandoc.wasm for the app UI (Windows builds).
$ErrorActionPreference = "Stop"
$Version = "3.10"
$Sha = "e0865674db6fa2698d29811ca2fcb91ab00a2f8b7d0220eae4ea28405d9cab2b"
$Root = Split-Path -Parent $PSScriptRoot
$Out = Join-Path $Root "crates\fo-ui\assets\pandoc"
if (Test-Path (Join-Path $Out "pandoc.wasm")) { exit 0 }
$Zip = Join-Path $env:TEMP "pandoc-wasm.zip"
Invoke-WebRequest "https://github.com/jgm/pandoc/releases/download/$Version/pandoc-$Version.wasm.zip" -OutFile $Zip
if ((Get-FileHash $Zip -Algorithm SHA256).Hash.ToLower() -ne $Sha) { throw "checksum mismatch" }
$Tmp = Join-Path $env:TEMP "pandoc-wasm"
Expand-Archive $Zip -DestinationPath $Tmp -Force
New-Item -ItemType Directory -Force -Path $Out | Out-Null
Copy-Item (Join-Path $Tmp "pandoc\pandoc.wasm") $Out
Copy-Item (Join-Path $Tmp "pandoc\COPYRIGHT") (Join-Path $Out "PANDOC-COPYRIGHT.txt")
