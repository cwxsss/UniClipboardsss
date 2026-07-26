Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$root = Split-Path -Parent $PSScriptRoot
$entrypoint = Join-Path $root "deploy/synology/uniclipboard-server-entrypoint.sh"
$dockerfile = Join-Path $root "deploy/synology/Dockerfile"
$workflow = Join-Path $root ".github/workflows/mirror-server-image-dockerhub.yml"
$adminServer = Join-Path $root "deploy/synology/admin-web/server.js"
$adminIndex = Join-Path $root "deploy/synology/admin-web/static/index.html"
$adminApp = Join-Path $root "deploy/synology/admin-web/static/app.js"
$adminStyles = Join-Path $root "deploy/synology/admin-web/static/styles.css"

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
Assert-Contains -Path $entrypoint -Pattern "UC_ADMIN_WEB"
Assert-Contains -Path $entrypoint -Pattern "UC_ADMIN_PORT"
Assert-Contains -Path $entrypoint -Pattern "UC_ADMIN_PASSWORD"
Assert-Contains -Path $entrypoint -Pattern "UC_ADMIN_SHOW_SPACE_PASSPHRASE"
Assert-Contains -Path $entrypoint -Pattern "strip_outer_quotes()"
Assert-Contains -Path $entrypoint -Pattern "auto_init_enabled()"
Assert-Contains -Path $entrypoint -Pattern "admin_web_enabled()"
Assert-Contains -Path $entrypoint -Pattern "is_setup_complete()"
Assert-Contains -Path $entrypoint -Pattern "/.local/share/app.uniclipboard.desktop/vault/.setup_status"
Assert-Contains -Path $entrypoint -Pattern '"has_completed"[[:space:]]*:[[:space:]]*true'
Assert-Contains -Path $entrypoint -Pattern "setup is still incomplete; set UC_AUTO_INIT=1"
Assert-Contains -Path $entrypoint -Pattern "UC_ADMIN_WEB=1 requires UC_ADMIN_PASSWORD"
Assert-Contains -Path $entrypoint -Pattern "start_admin_web()"
Assert-Contains -Path $entrypoint -Pattern "stop_admin_web()"
Assert-Contains -Path $entrypoint -Pattern "uniclipboard-admin-web &"
Assert-Contains -Path $entrypoint -Pattern 'UC_SPACE_PASSPHRASE="${UC_SPACE_PASSPHRASE}"'
Assert-Contains -Path $entrypoint -Pattern "ADMIN_WEB_PID="
Assert-Contains -Path $entrypoint -Pattern "trap stop_admin_web INT TERM EXIT"
Assert-Contains -Path $entrypoint -Pattern "uniclip mobile network set"
Assert-Contains -Path $entrypoint -Pattern "--url"
Assert-Contains -Path $entrypoint -Pattern "uniclip mobile add --label"
Assert-Contains -Path $entrypoint -Pattern "uniclip stop"
Assert-Contains -Path $entrypoint -Pattern "exec uniclip start --server --foreground"

$entrypointContent = Get-Content -Raw -Path $entrypoint
$stopIndex = $entrypointContent.IndexOf("uniclip stop")
$foregroundStartIndex = $entrypointContent.IndexOf("exec uniclip start --server --foreground")
if ($stopIndex -lt 0 -or $foregroundStartIndex -lt 0 -or $stopIndex -gt $foregroundStartIndex) {
    throw "Expected the transient daemon to stop before foreground server startup"
}

Assert-Contains -Path $dockerfile -Pattern 'FROM ${BASE_IMAGE}'
Assert-Contains -Path $dockerfile -Pattern "apk add --no-cache nodejs"
Assert-Contains -Path $dockerfile -Pattern "COPY deploy/synology/admin-web/"
Assert-Contains -Path $dockerfile -Pattern "/opt/uniclipboard-admin-web/"
Assert-Contains -Path $dockerfile -Pattern "ln -s /opt/uniclipboard-admin-web/server.js /usr/local/bin/uniclipboard-admin-web"
Assert-Contains -Path $dockerfile -Pattern "uniclipboard-server-entrypoint"
Assert-Contains -Path $dockerfile -Pattern "CMD [""uniclipboard-server-entrypoint""]"

foreach ($path in @($adminServer, $adminIndex, $adminApp, $adminStyles)) {
    if (-not (Test-Path $path)) {
        throw "Missing Synology admin web file: $path"
    }
}

Assert-Contains -Path $adminServer -Pattern "POST /api/login"
Assert-Contains -Path $adminServer -Pattern "GET /api/status"
Assert-Contains -Path $adminServer -Pattern "GET /api/devices"
Assert-Contains -Path $adminServer -Pattern "POST /api/devices"
Assert-Contains -Path $adminServer -Pattern "DELETE /api/devices/"
Assert-Contains -Path $adminServer -Pattern "POST /api/desktop-invite"
Assert-Contains -Path $adminServer -Pattern "startDesktopInvite"
Assert-Contains -Path $adminServer -Pattern "INVITATION_CODE="
Assert-Contains -Path $adminServer -Pattern "UC_ADMIN_SHOW_SPACE_PASSPHRASE"
Assert-Contains -Path $adminServer -Pattern "cleanupDesktopInvite"
Assert-Contains -Path $adminServer -Pattern "rotate-password"
Assert-Contains -Path $adminServer -Pattern "HttpOnly"
Assert-Contains -Path $adminServer -Pattern "SameSite=Strict"
Assert-Contains -Path $adminServer -Pattern "password (redacted)"
Assert-Contains -Path $adminApp -Pattern "qrCodePngBase64"
Assert-Contains -Path $adminApp -Pattern "installQrCodePngBase64"
Assert-Contains -Path $adminApp -Pattern "/api/desktop-invite"
Assert-Contains -Path $adminApp -Pattern "desktopInviteCode"
Assert-Contains -Path $adminIndex -Pattern "desktopInvitePanel"
Assert-Contains -Path $adminStyles -Pattern ".desktop-invite"
Assert-Contains -Path $adminIndex -Pattern "loginView"

Assert-Contains -Path $workflow -Pattern "docker/build-push-action"
Assert-Contains -Path $workflow -Pattern "file: deploy/synology/Dockerfile"
Assert-Contains -Path $workflow -Pattern 'BASE_IMAGE=${{ inputs.source_image }}'

Write-Output "Synology server wrapper checks passed."
