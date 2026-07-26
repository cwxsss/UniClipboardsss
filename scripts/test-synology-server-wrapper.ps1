Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$root = Split-Path -Parent $PSScriptRoot
$entrypoint = Join-Path $root "deploy/synology/uniclipboard-server-entrypoint.sh"
$dockerfile = Join-Path $root "deploy/synology/Dockerfile"
$workflow = Join-Path $root ".github/workflows/mirror-server-image-dockerhub.yml"

function Assert-Contains {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)][string]$Pattern
    )

    $content = Get-Content -Raw -Path $Path
    if ($content -notmatch [regex]::Escape($Pattern)) {
        throw "Expected '$Path' to contain '$Pattern'"
    }
}

if (-not (Test-Path $entrypoint)) {
    throw "Missing Synology entrypoint script: $entrypoint"
}

if (-not (Test-Path $dockerfile)) {
    throw "Missing Synology Dockerfile: $dockerfile"
}

Assert-Contains -Path $entrypoint -Pattern "/data/uniclipboard-server.env"
Assert-Contains -Path $entrypoint -Pattern "UC_AUTO_INIT"
Assert-Contains -Path $entrypoint -Pattern "UC_SPACE_PASSPHRASE"
Assert-Contains -Path $entrypoint -Pattern "UC_MOBILE_PUBLIC_URL"
Assert-Contains -Path $entrypoint -Pattern "is_setup_complete()"
Assert-Contains -Path $entrypoint -Pattern "/.local/share/app.uniclipboard.desktop/vault/.setup_status"
Assert-Contains -Path $entrypoint -Pattern '"has_completed"[[:space:]]*:[[:space:]]*true'
Assert-Contains -Path $entrypoint -Pattern "uniclip mobile network set"
Assert-Contains -Path $entrypoint -Pattern "--url"
Assert-Contains -Path $entrypoint -Pattern "uniclip mobile add --label"
Assert-Contains -Path $entrypoint -Pattern "exec uniclip start --server --foreground"

Assert-Contains -Path $dockerfile -Pattern 'FROM ${BASE_IMAGE}'
Assert-Contains -Path $dockerfile -Pattern "uniclipboard-server-entrypoint"
Assert-Contains -Path $dockerfile -Pattern "CMD [""uniclipboard-server-entrypoint""]"

Assert-Contains -Path $workflow -Pattern "docker/build-push-action"
Assert-Contains -Path $workflow -Pattern "file: deploy/synology/Dockerfile"
Assert-Contains -Path $workflow -Pattern 'BASE_IMAGE=${{ inputs.source_image }}'

Write-Output "Synology server wrapper checks passed."
