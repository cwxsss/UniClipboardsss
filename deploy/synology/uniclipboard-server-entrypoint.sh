#!/bin/sh
set -eu

CONFIG_FILE="${UC_SERVER_CONFIG:-/data/uniclipboard-server.env}"
BOOTSTRAP_DIR="${UC_SERVER_BOOTSTRAP_DIR:-/data/.uniclipboard-server-bootstrap}"
MOBILE_MARKER="${BOOTSTRAP_DIR}/mobile-device-added"

if [ -f "${CONFIG_FILE}" ]; then
  # shellcheck disable=SC1090
  . "${CONFIG_FILE}"
fi

mkdir -p "${BOOTSTRAP_DIR}"

is_setup_complete() {
  status_file="${HOME:-/data}/.local/share/app.uniclipboard.desktop/vault/.setup_status"
  legacy_marker="${HOME:-/data}/.local/share/app.uniclipboard.desktop/vault/.initialized_encryption"

  if [ -f "${status_file}" ] && grep -q '"has_completed"[[:space:]]*:[[:space:]]*true' "${status_file}"; then
    return 0
  fi

  if [ -f "${legacy_marker}" ]; then
    return 0
  fi

  return 1
}

if [ "${UC_AUTO_INIT:-0}" = "1" ] && ! is_setup_complete; then
  if [ -z "${UC_SPACE_PASSPHRASE:-}" ]; then
    echo "UC_AUTO_INIT=1 requires UC_SPACE_PASSPHRASE in ${CONFIG_FILE}" >&2
    exit 1
  fi

  uniclip init \
    --passphrase "${UC_SPACE_PASSPHRASE}" \
    --device-name "${UC_DEVICE_NAME:-Synology Server}"
fi

if [ -n "${UC_MOBILE_PUBLIC_URL:-}" ]; then
  uniclip mobile network set \
    --url "${UC_MOBILE_PUBLIC_URL}" \
    --accept-network-risk
fi

if [ -n "${UC_MOBILE_LABEL:-}" ] && [ ! -f "${MOBILE_MARKER}" ]; then
  uniclip mobile add --label "${UC_MOBILE_LABEL}"
  touch "${MOBILE_MARKER}"
fi

exec uniclip start --server --foreground
