<#
Downloads the pinned official PDFium Windows x64 build (bblanchon/pdfium-binaries)
into apps/desktop/src-tauri/resources/pdfium so the Tauri bundle can ship it.
Run once before `npm run desktop:build`. The archive hash is verified.
#>
param(
  [string]$Archive = ''
)

$ErrorActionPreference = 'Stop'
$release = 'chromium/8076'
$sha256 = '808d36da9bc5a3104315fb307c80998121f565ee53953633bf33e80d7429e5ac'
$root = Split-Path -Parent $PSScriptRoot
$target = Join-Path $root 'apps/desktop/src-tauri/resources/pdfium'

if (Test-Path (Join-Path $target 'pdfium.dll')) {
  Write-Host "pdfium.dll already present in $target"
  return
}

$temporary = $null
if (-not $Archive) {
  $temporary = Join-Path ([System.IO.Path]::GetTempPath()) 'pdfium-win-x64.tgz'
  $url = "https://github.com/bblanchon/pdfium-binaries/releases/download/$([uri]::EscapeDataString($release))/pdfium-win-x64.tgz"
  Invoke-WebRequest -Uri $url -OutFile $temporary
  $Archive = $temporary
}

$stream = [System.IO.File]::OpenRead((Resolve-Path $Archive).Path)
try {
  $actual = ([System.BitConverter]::ToString(
      [System.Security.Cryptography.SHA256]::Create().ComputeHash($stream)
    ) -replace '-', '').ToLowerInvariant()
} finally {
  $stream.Dispose()
}
if ($actual -ne $sha256) {
  throw "PDFium archive hash mismatch: expected $sha256, got $actual"
}

$extract = Join-Path ([System.IO.Path]::GetTempPath()) 'pdfium-extract'
if (Test-Path $extract) { Remove-Item -Recurse -Force $extract }
New-Item -ItemType Directory -Force $extract | Out-Null
tar -xzf $Archive -C $extract
New-Item -ItemType Directory -Force $target | Out-Null
Copy-Item (Join-Path $extract 'bin/pdfium.dll') $target
Copy-Item (Join-Path $extract 'LICENSE') (Join-Path $target 'PDFIUM-LICENSE.txt')
Copy-Item (Join-Path $extract 'licenses') (Join-Path $target 'licenses') -Recurse
Remove-Item -Recurse -Force $extract
if ($temporary) { Remove-Item -Force $temporary }
Write-Host "PDFium $release installed in $target"
