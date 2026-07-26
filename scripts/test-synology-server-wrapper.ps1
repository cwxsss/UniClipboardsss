Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$root = Split-Path -Parent $PSScriptRoot
$adminWeb = Join-Path $root "deploy/synology/admin-web"

& node --check (Join-Path $adminWeb "server.js")
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

& node --check (Join-Path $adminWeb "static/app.js")
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

& node --test (Join-Path $adminWeb "test/server.test.js")
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

Write-Output "Synology admin web behavior tests passed."
