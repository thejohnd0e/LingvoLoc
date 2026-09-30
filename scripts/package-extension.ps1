$ErrorActionPreference = 'Stop'

npm run extension:build
$source = Join-Path $PSScriptRoot '..\apps\extension\dist'
$destination = Join-Path $PSScriptRoot '..\apps\extension\LingvoLoc-extension-2.2.5.zip'
if (Test-Path -LiteralPath $destination) {
  Remove-Item -LiteralPath $destination -Force
}
Compress-Archive -Path (Join-Path $source '*') -DestinationPath $destination
Write-Output $destination
