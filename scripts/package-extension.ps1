$ErrorActionPreference = 'Stop'

npm run extension:build
$extension = Join-Path $PSScriptRoot '..\apps\extension'
# The ZIP name follows the extension package version so a release never overwrites another version's archive.
$version = (Get-Content -Raw -LiteralPath (Join-Path $extension 'package.json') | ConvertFrom-Json).version
if (-not $version) {
  throw 'apps/extension/package.json has no version'
}
$source = Join-Path $extension 'dist'
$destination = Join-Path $extension "LingvoLoc-extension-$version.zip"
if (Test-Path -LiteralPath $destination) {
  Remove-Item -LiteralPath $destination -Force
}
Compress-Archive -Path (Join-Path $source '*') -DestinationPath $destination
Write-Output $destination
