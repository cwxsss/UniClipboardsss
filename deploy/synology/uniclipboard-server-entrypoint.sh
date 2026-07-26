#!/bin/sh
set -eu

CONFIG_FILE="${UC_SERVER_CONFIG:-/data/uniclipboard-server.env}"
BOOTSTRAP_DIR="${UC_SERVER_BOOTSTRAP_DIR:-/data/.uniclipboard-server-bootstrap}"
MOBILE_MARKER="${BOOTSTRAP_DIR}/mobile-device-added"
ADMIN_WEB_PID=""

if [ -f "${CONFIG_FILE}" ]; then
  # shellcheck disable=SC1090
  . "${CONFIG_FILE}"
fi

mkdir -p "${BOOTSTRAP_DIR}"

strip_outer_quotes() {
  printf '%s' "$1" | sed -e 's/^"//' -e 's/"$//' -e "s/^'//" -e "s/'$//"
}

UC_AUTO_INIT="$(strip_outer_quotes "${UC_AUTO_INIT:-0}")"
UC_SPACE_PASSPHRASE="$(strip_outer_quotes "${UC_SPACE_PASSPHRASE:-}")"
UC_DEVICE_NAME="$(strip_outer_quotes "${UC_DEVICE_NAME:-Synology Server}")"
UC_MOBILE_PUBLIC_URL="$(strip_outer_quotes "${UC_MOBILE_PUBLIC_URL:-}")"
UC_MOBILE_LABEL="$(strip_outer_quotes "${UC_MOBILE_LABEL:-}")"
UC_ADMIN_WEB="$(strip_outer_quotes "${UC_ADMIN_WEB:-0}")"
UC_ADMIN_PORT="$(strip_outer_quotes "${UC_ADMIN_PORT:-42888}")"
UC_ADMIN_PASSWORD="$(strip_outer_quotes "${UC_ADMIN_PASSWORD:-}")"

export UC_ADMIN_PORT
export UC_ADMIN_PASSWORD

auto_init_enabled() {
  case "${UC_AUTO_INIT}" in
    1|true|TRUE|yes|YES|on|ON) return 0 ;;
    *) return 1 ;;
  esac
}

admin_web_enabled() {
  case "${UC_ADMIN_WEB}" in
    1|true|TRUE|yes|YES|on|ON) return 0 ;;
    *) return 1 ;;
  esac
}

stop_admin_web() {
  if [ -n "${ADMIN_WEB_PID:-}" ]; then
    kill "${ADMIN_WEB_PID}" 2>/dev/null || true
    wait "${ADMIN_WEB_PID}" 2>/dev/null || true
  fi
}

start_admin_web() {
  if [ -z "${UC_ADMIN_PASSWORD:-}" ]; then
    echo "UC_ADMIN_WEB=1 requires UC_ADMIN_PASSWORD" >&2
    exit 1
  fi

  uniclipboard-admin-web &
  ADMIN_WEB_PID="$!"
  trap stop_admin_web INT TERM EXIT
}

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

if auto_init_enabled && ! is_setup_complete; then
  if [ -z "${UC_SPACE_PASSPHRASE:-}" ]; then
    echo "UC_AUTO_INIT=1 requires UC_SPACE_PASSPHRASE in ${CONFIG_FILE}" >&2
    exit 1
  fi

  uniclip init \
    --passphrase "${UC_SPACE_PASSPHRASE}" \
    --device-name "${UC_DEVICE_NAME}"
fi

if ! is_setup_complete; then
  echo "setup is still incomplete; set UC_AUTO_INIT=1 and UC_SPACE_PASSPHRASE, or run uniclip init/join against /data first" >&2
  exit 1
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

if admin_web_enabled; then
  start_admin_web
fi

# Provisioning commands may leave a transient Oneshot daemon alive briefly.
# Wait for it to stop before the foreground daemon becomes the container's PID 1.
uniclip stop

exec uniclip start --server --foreground
