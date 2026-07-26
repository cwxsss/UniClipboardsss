Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$root = Split-Path -Parent $PSScriptRoot
$adminWeb = Join-Path $root "deploy/synology/admin-web"
$entrypointTest = Join-Path $PSScriptRoot "test-synology-server-entrypoint.ps1"

& node --check (Join-Path $adminWeb "server.js")
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

& node --check (Join-Path $adminWeb "static/app.js")
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

& node --test (Join-Path $adminWeb "test/server.test.js")
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

& $PSHOME\powershell.exe -NoProfile -ExecutionPolicy Bypass -File $entrypointTest
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

Write-Output "Synology server deployment tests passed."
