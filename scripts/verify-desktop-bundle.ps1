param(
  [string]$Root = ''
)

$ErrorActionPreference = 'Stop'
$repository = Split-Path -Parent $PSScriptRoot
if (-not $Root) {
  $staged = Join-Path $repository 'apps/desktop/src-tauri/target/release'
  $source = Join-Path $repository 'apps/desktop/src-tauri/resources'
  $Root = if (Test-Path -LiteralPath $staged -PathType Container) { $staged } else { $source }
}

$required = @(
  'pdfium/pdfium.dll',
  'pdfium/PDFIUM-LICENSE.txt',
  'pdfium/licenses/abseil.txt',
  'pdfium/licenses/agg23.txt',
  'pdfium/licenses/fast_float.txt',
  'pdfium/licenses/freetype.txt',
  'pdfium/licenses/icu.txt',
  'pdfium/licenses/lcms.txt',
  'pdfium/licenses/libjpeg_turbo.ijg',
  'pdfium/licenses/libjpeg_turbo.md',
  'pdfium/licenses/libopenjpeg.txt',
  'pdfium/licenses/libpng.txt',
  'pdfium/licenses/llvm-libc.txt',
  'pdfium/licenses/pdfium.txt',
  'pdfium/licenses/simdutf.txt',
  'pdfium/licenses/zlib.txt',
  'fonts/OFL.txt',
  'fonts/SOURCE.md'
)

$missing = @(
  $required | Where-Object { -not (Test-Path -LiteralPath (Join-Path $Root $_) -PathType Leaf) }
)
if ($missing.Count -gt 0) {
  Write-Error ("Missing bundled desktop resources under {0}:`n{1}" -f $Root, ($missing -join "`n"))
}

Write-Host "Desktop bundle resources verified under $Root"
