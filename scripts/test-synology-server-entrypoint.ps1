Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$root = Split-Path -Parent $PSScriptRoot
$entrypoint = Join-Path $root "deploy/synology/uniclipboard-server-entrypoint.sh"
$shCandidates = @(
  (Get-Command sh -ErrorAction SilentlyContinue | Select-Object -ExpandProperty Source -ErrorAction SilentlyContinue),
  "C:\Program Files\Git\bin\sh.exe",
  "C:\Program Files\Git\usr\bin\sh.exe"
) | Where-Object { $_ -and (Test-Path -LiteralPath $_) }
$sh = $shCandidates | Select-Object -First 1

if (-not $sh) {
  throw "a POSIX sh implementation is required to test the Synology entrypoint (Git for Windows is supported)"
}

function Convert-ToPosixPath {
  param([Parameter(Mandatory = $true)][string]$Path)

  $converted = & $sh -c 'cygpath -u "$1"' -- $Path
  if ($LASTEXITCODE -ne 0) { throw "failed to convert path for sh: $Path" }
  return ($converted | Select-Object -First 1).Trim()
}

$entrypointUnix = Convert-ToPosixPath $entrypoint

function Assert-Equal {
  param(
    [Parameter(Mandatory = $true)]$Actual,
    [Parameter(Mandatory = $true)]$Expected,
    [Parameter(Mandatory = $true)][string]$Message
  )

  if ($Actual -ne $Expected) {
    throw "$Message. Expected '$Expected', got '$Actual'."
  }
}

function Assert-Contains {
  param(
    [Parameter(Mandatory = $true)][string]$Actual,
    [Parameter(Mandatory = $true)][string]$Expected,
    [Parameter(Mandatory = $true)][string]$Message
  )

  if (-not $Actual.Contains($Expected)) {
    throw "$Message. Missing '$Expected' in '$Actual'."
  }
}

function New-TestFixture {
  $path = Join-Path ([System.IO.Path]::GetTempPath()) ("uniclip-entrypoint-" + [guid]::NewGuid())
  $bin = Join-Path $path "bin"
  New-Item -ItemType Directory -Force -Path $bin | Out-Null

  $fakeUniclip = Join-Path $bin "uniclip"
  $fakeUniclipBody = @'
#!/bin/sh
set -eu
printf '%s\n' "$*" >> "$UC_TEST_COMMAND_LOG"

case "$1" in
  init|join)
    status_dir="${HOME}/.local/share/app.uniclipboard.desktop/vault"
    mkdir -p "$status_dir"
    printf '%s\n' '{"has_completed": true}' > "${status_dir}/.setup_status"
    ;;
  start|stop|mobile)
    ;;
  *)
    printf 'unexpected uniclip command: %s\n' "$1" >&2
    exit 64
    ;;
esac
'@
  [System.IO.File]::WriteAllText($fakeUniclip, $fakeUniclipBody, [System.Text.UTF8Encoding]::new($false))

  $fakeUniclipUnix = Convert-ToPosixPath $fakeUniclip
  & $sh -c 'chmod +x "$1"' -- $fakeUniclipUnix
  if ($LASTEXITCODE -ne 0) { throw "failed to mark fake uniclip executable" }

  return [pscustomobject]@{
    Path = $path
    Bin = Convert-ToPosixPath $bin
    Home = Convert-ToPosixPath (Join-Path $path "home")
    Log = Convert-ToPosixPath (Join-Path $path "commands.log")
    LogWindows = Join-Path $path "commands.log"
    Config = Convert-ToPosixPath (Join-Path $path "missing.env")
    HomeWindows = Join-Path $path "home"
  }
}

function Invoke-Entrypoint {
  param(
    [Parameter(Mandatory = $true)]$Fixture,
    [Parameter(Mandatory = $true)][hashtable]$Variables
  )

  $saved = @{}
  $names = @(
    "HOME",
    "UC_SERVER_CONFIG",
    "UC_SERVER_BOOTSTRAP_DIR",
    "UC_TEST_COMMAND_LOG",
    "UC_SPACE_BOOTSTRAP_MODE",
    "UC_SPACE_INVITE_CODE",
    "UC_SPACE_PASSPHRASE",
    "UC_DEVICE_NAME",
    "UC_MOBILE_PUBLIC_URL",
    "UC_MOBILE_LABEL",
    "UC_ADMIN_WEB"
  )

  foreach ($name in $names) { $saved[$name] = [Environment]::GetEnvironmentVariable($name, "Process") }

  try {
    foreach ($name in $names) { Remove-Item "Env:$name" -ErrorAction SilentlyContinue }

    $env:HOME = $Fixture.Home
    $env:UC_SERVER_CONFIG = $Fixture.Config
    $env:UC_SERVER_BOOTSTRAP_DIR = (Join-Path $Fixture.Path "bootstrap")
    $env:UC_TEST_COMMAND_LOG = $Fixture.Log
    $env:UC_ADMIN_WEB = "0"

    foreach ($entry in $Variables.GetEnumerator()) {
      [Environment]::SetEnvironmentVariable($entry.Key, [string]$entry.Value, "Process")
    }

    $previousErrorActionPreference = $ErrorActionPreference
    $ErrorActionPreference = "Continue"
    try {
      $output = & $sh -c 'PATH="$1:$PATH"; export PATH; exec "$2"' -- $Fixture.Bin $entrypointUnix 2>&1
      $exitCode = $LASTEXITCODE
    } finally {
      $ErrorActionPreference = $previousErrorActionPreference
    }

    return [pscustomobject]@{
      ExitCode = $exitCode
      Output = ($output | Out-String)
      Commands = if (Test-Path -LiteralPath $Fixture.LogWindows) { Get-Content -LiteralPath $Fixture.LogWindows -Raw } else { "" }
    }
  } finally {
    foreach ($name in $names) {
      [Environment]::SetEnvironmentVariable($name, $saved[$name], "Process")
    }
  }
}

function Invoke-Case {
  param(
    [Parameter(Mandatory = $true)][string]$Name,
    [Parameter(Mandatory = $true)][scriptblock]$Body
  )

  & $Body
  Write-Output "PASS $Name"
}

try {
  Invoke-Case "init bootstrap" {
    $fixture = New-TestFixture
    try {
      $result = Invoke-Entrypoint -Fixture $fixture -Variables @{
        UC_SPACE_BOOTSTRAP_MODE = "init"
        UC_SPACE_PASSPHRASE = "test-passphrase"
        UC_DEVICE_NAME = "Test Server"
      }
      Assert-Equal $result.ExitCode 0 "init bootstrap must succeed"
      Assert-Contains $result.Commands "init --passphrase test-passphrase --device-name Test Server" "init bootstrap must call uniclip init"
    } finally {
      Remove-Item -LiteralPath $fixture.Path -Recurse -Force -ErrorAction SilentlyContinue
    }
  }

  Invoke-Case "join bootstrap" {
    $fixture = New-TestFixture
    try {
      $result = Invoke-Entrypoint -Fixture $fixture -Variables @{
        UC_SPACE_BOOTSTRAP_MODE = "join"
        UC_SPACE_INVITE_CODE = "invite-code"
        UC_SPACE_PASSPHRASE = "test-passphrase"
        UC_DEVICE_NAME = "Joined Server"
      }
      Assert-Equal $result.ExitCode 0 "join bootstrap must succeed"
      Assert-Contains $result.Commands "join --code invite-code --passphrase test-passphrase --device-name Joined Server" "join bootstrap must call uniclip join"
    } finally {
      Remove-Item -LiteralPath $fixture.Path -Recurse -Force -ErrorAction SilentlyContinue
    }
  }

  Invoke-Case "invalid bootstrap mode" {
    $fixture = New-TestFixture
    try {
      $result = Invoke-Entrypoint -Fixture $fixture -Variables @{ UC_SPACE_BOOTSTRAP_MODE = "invalid" }
      Assert-Equal $result.ExitCode 1 "an invalid mode must fail"
      Assert-Contains $result.Output "UC_SPACE_BOOTSTRAP_MODE must be init or join" "invalid mode must explain the accepted values"
    } finally {
      Remove-Item -LiteralPath $fixture.Path -Recurse -Force -ErrorAction SilentlyContinue
    }
  }

  Invoke-Case "join requires invite and passphrase" {
    $fixture = New-TestFixture
    try {
      $result = Invoke-Entrypoint -Fixture $fixture -Variables @{ UC_SPACE_BOOTSTRAP_MODE = "join" }
      Assert-Equal $result.ExitCode 1 "join without credentials must fail"
      Assert-Contains $result.Output "UC_SPACE_BOOTSTRAP_MODE=join requires UC_SPACE_INVITE_CODE and UC_SPACE_PASSPHRASE" "join validation must name both required variables"
    } finally {
      Remove-Item -LiteralPath $fixture.Path -Recurse -Force -ErrorAction SilentlyContinue
    }
  }

  Invoke-Case "existing setup is never reprovisioned" {
    $fixture = New-TestFixture
    try {
      $statusDir = Join-Path $fixture.HomeWindows ".local/share/app.uniclipboard.desktop/vault"
      New-Item -ItemType Directory -Force -Path $statusDir | Out-Null
      [System.IO.File]::WriteAllText(
        (Join-Path $statusDir ".setup_status"),
        '{"has_completed": true}',
        [System.Text.UTF8Encoding]::new($false)
      )

      $result = Invoke-Entrypoint -Fixture $fixture -Variables @{ UC_SPACE_BOOTSTRAP_MODE = "join" }
      Assert-Equal $result.ExitCode 0 "an existing setup must start without bootstrap credentials"
      if ($result.Commands -match "^(init|join) ") {
        throw "an existing setup must not run init or join: $($result.Commands)"
      }
    } finally {
      Remove-Item -LiteralPath $fixture.Path -Recurse -Force -ErrorAction SilentlyContinue
    }
  }
} catch {
  Write-Error $_
  exit 1
}

Write-Output "Synology server entrypoint bootstrap tests passed."
