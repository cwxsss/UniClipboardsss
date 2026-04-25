#!/usr/bin/env bash
# dual-logs.sh — Helper for inspecting macOS + Windows uniclipboard logs side-by-side.
#
# Logs are JSONL (one JSON object per line). File names use UTC dates
# (uniclipboard.json.YYYY-MM-DD), so "today" here means UTC today.
#
# macOS path:    $MAC_BASE/app.uniclipboard.desktop[-<UC_PROFILE>]/logs/
# Windows path:  /tmp/win-uniclipboard/logs/   (SMB share mounted on this Mac)

set -euo pipefail

MAC_BASE="${MAC_BASE:-$HOME/Library/Application Support}"
WIN_LOGS="${WIN_LOGS:-/tmp/win-uniclipboard/logs}"
DEFAULT_PROFILE="${UC_PROFILE_DEFAULT:-dev}"

usage() {
  cat <<'EOF'
Usage: dual-logs.sh <command> [options]

Commands:
  status                       Show both sides: profile dirs, latest log files, freshness.
  list-profiles                List all macOS profile dirs that have a logs/ folder.
  paths [--profile <name>]     Print resolved log file paths for both sides.
  tail   [--profile <name>] [--lines N] [--side mac|win|both]
                               Tail latest log file. Default: --side both --lines 50.
  query  [--profile <name>] [--side mac|win|both]
                               Pipe latest logs through jq. Read --filter from arg or stdin.
                               Examples:
                                 dual-logs.sh query --filter '. | select(.level=="ERROR")'
                                 dual-logs.sh query --side mac --filter '. | select(.target|test("pairing"))'
  merge  [--profile <name>] [--since <ISO8601>] [--lines N]
                               Merge both sides chronologically by .timestamp.
                               Each line gets a .side field ("mac" or "win") prepended.
  grep   <pattern> [--profile <name>] [--side mac|win|both]
                               Plain-text grep on the latest log files.

Notes:
  * Log file names use UTC dates. "Latest" = newest by mtime, not by clock date.
  * If mtime is far behind current time, the profile is likely wrong — ask the user.
EOF
}

# --- profile resolution ----------------------------------------------------

mac_profile_dir() {
  local profile="${1:-}"
  if [[ -z "$profile" || "$profile" == "default" ]]; then
    echo "$MAC_BASE/app.uniclipboard.desktop"
  else
    echo "$MAC_BASE/app.uniclipboard.desktop-$profile"
  fi
}

list_profile_dirs() {
  # Print one line per existing profile dir that has a logs/ subdir.
  # Format: <profile>\t<dir>\t<latest_log>\t<latest_mtime_iso>
  shopt -s nullglob
  local d name profile latest mtime
  for d in "$MAC_BASE"/app.uniclipboard.desktop "$MAC_BASE"/app.uniclipboard.desktop-*; do
    [[ -d "$d/logs" ]] || continue
    name="$(basename "$d")"
    if [[ "$name" == "app.uniclipboard.desktop" ]]; then
      profile="default"
    else
      profile="${name#app.uniclipboard.desktop-}"
    fi
    latest="$(latest_log_in "$d/logs" || true)"
    if [[ -n "$latest" ]]; then
      mtime="$(stat -f '%Sm' -t '%Y-%m-%dT%H:%M:%S' "$latest")"
    else
      mtime=""
    fi
    printf '%s\t%s\t%s\t%s\n' "$profile" "$d/logs" "${latest:-<empty>}" "${mtime:-<no-logs>}"
  done
}

latest_log_in() {
  local dir="$1"
  [[ -d "$dir" ]] || return 1
  # Pick the most recently modified uniclipboard.json.* file.
  # ls -t orders by mtime descending.
  local f
  f="$(ls -t "$dir"/uniclipboard.json.* 2>/dev/null | head -n1 || true)"
  [[ -n "$f" ]] || return 1
  printf '%s' "$f"
}

# --- helpers ---------------------------------------------------------------

iso_now() { date -u "+%Y-%m-%dT%H:%M:%SZ"; }

mtime_iso() { stat -f '%Sm' -t '%Y-%m-%dT%H:%M:%S' "$1" 2>/dev/null || echo ""; }

age_seconds() {
  local f="$1"
  [[ -f "$f" ]] || { echo ""; return; }
  local now mt
  now="$(date +%s)"
  mt="$(stat -f '%m' "$f")"
  echo $(( now - mt ))
}

freshness_label() {
  # Map age in seconds to a human label.
  local s="$1"
  [[ -z "$s" ]] && { echo "unknown"; return; }
  if   (( s < 120     )); then echo "live (<2m)"
  elif (( s < 600     )); then echo "recent (<10m)"
  elif (( s < 3600    )); then echo "stale (<1h)"
  elif (( s < 86400   )); then echo "old (<1d)"
  else                          echo "cold (>1d)"
  fi
}

# --- commands --------------------------------------------------------------

cmd_status() {
  local profile="${1:-$DEFAULT_PROFILE}"
  local mac_dir win_dir mac_log win_log
  mac_dir="$(mac_profile_dir "$profile")/logs"
  win_dir="$WIN_LOGS"

  echo "=== uniclipboard dual-log status ==="
  echo "now (local):   $(date '+%Y-%m-%d %H:%M:%S %Z')"
  echo "now (UTC):     $(iso_now)"
  echo "mac profile:   $profile"
  echo

  echo "[mac] $mac_dir"
  if [[ -d "$mac_dir" ]]; then
    mac_log="$(latest_log_in "$mac_dir" || true)"
    if [[ -n "$mac_log" ]]; then
      local age; age="$(age_seconds "$mac_log")"
      printf '  latest: %s\n  mtime:  %s   freshness: %s\n' \
        "$(basename "$mac_log")" "$(mtime_iso "$mac_log")" "$(freshness_label "$age")"
    else
      echo "  (no log files in this profile)"
    fi
  else
    echo "  (profile dir does not exist — likely wrong UC_PROFILE)"
    echo "  available profiles:"
    list_profile_dirs | awk -F'\t' '{printf "    - %s  (latest=%s, mtime=%s)\n", $1, $3, $4}'
  fi
  echo

  echo "[win] $win_dir"
  if [[ -d "$win_dir" ]]; then
    win_log="$(latest_log_in "$win_dir" || true)"
    if [[ -n "$win_log" ]]; then
      local age; age="$(age_seconds "$win_log")"
      printf '  latest: %s\n  mtime:  %s   freshness: %s\n' \
        "$(basename "$win_log")" "$(mtime_iso "$win_log")" "$(freshness_label "$age")"
    else
      echo "  (no log files — Windows side may not be running)"
    fi
  else
    echo "  (mount missing: $win_dir — re-mount the SMB share)"
  fi
}

cmd_list_profiles() {
  printf '%s\t%s\t%s\t%s\n' "PROFILE" "DIR" "LATEST" "MTIME"
  list_profile_dirs
}

cmd_paths() {
  local profile="${1:-$DEFAULT_PROFILE}"
  local mac_dir mac_log win_log
  mac_dir="$(mac_profile_dir "$profile")/logs"
  mac_log="$(latest_log_in "$mac_dir" 2>/dev/null || true)"
  win_log="$(latest_log_in "$WIN_LOGS" 2>/dev/null || true)"
  echo "MAC=${mac_log:-<missing>}"
  echo "WIN=${win_log:-<missing>}"
}

resolve_pair() {
  # Sets globals MAC_LOG and WIN_LOG. Empties them if missing.
  local profile="$1"
  local mac_dir
  mac_dir="$(mac_profile_dir "$profile")/logs"
  MAC_LOG="$(latest_log_in "$mac_dir" 2>/dev/null || true)"
  WIN_LOG="$(latest_log_in "$WIN_LOGS" 2>/dev/null || true)"
}

cmd_tail() {
  local profile="$DEFAULT_PROFILE" lines=50 side="both"
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --profile) profile="$2"; shift 2;;
      --lines)   lines="$2";   shift 2;;
      --side)    side="$2";    shift 2;;
      *) echo "unknown arg: $1" >&2; exit 2;;
    esac
  done
  resolve_pair "$profile"
  if [[ "$side" == "mac" || "$side" == "both" ]]; then
    echo "===== [mac] ${MAC_LOG:-<missing>} ====="
    [[ -n "$MAC_LOG" ]] && tail -n "$lines" "$MAC_LOG"
  fi
  if [[ "$side" == "win" || "$side" == "both" ]]; then
    echo "===== [win] ${WIN_LOG:-<missing>} ====="
    [[ -n "$WIN_LOG" ]] && tail -n "$lines" "$WIN_LOG"
  fi
}

cmd_query() {
  local profile="$DEFAULT_PROFILE" side="both" filter=""
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --profile) profile="$2"; shift 2;;
      --side)    side="$2";    shift 2;;
      --filter)  filter="$2";  shift 2;;
      *) echo "unknown arg: $1" >&2; exit 2;;
    esac
  done
  if [[ -z "$filter" ]]; then
    if [[ -t 0 ]]; then
      echo "error: --filter required (or pipe jq filter via stdin)" >&2
      exit 2
    fi
    filter="$(cat)"
  fi
  resolve_pair "$profile"
  if [[ "$side" == "mac" || "$side" == "both" ]]; then
    if [[ -n "$MAC_LOG" ]]; then
      echo "===== [mac] $MAC_LOG ====="
      jq -c "$filter" "$MAC_LOG" || true
    fi
  fi
  if [[ "$side" == "win" || "$side" == "both" ]]; then
    if [[ -n "$WIN_LOG" ]]; then
      echo "===== [win] $WIN_LOG ====="
      jq -c "$filter" "$WIN_LOG" || true
    fi
  fi
}

cmd_merge() {
  local profile="$DEFAULT_PROFILE" since="" lines=200
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --profile) profile="$2"; shift 2;;
      --since)   since="$2";   shift 2;;
      --lines)   lines="$2";   shift 2;;
      *) echo "unknown arg: $1" >&2; exit 2;;
    esac
  done
  resolve_pair "$profile"
  [[ -n "$MAC_LOG" || -n "$WIN_LOG" ]] || { echo "no logs found"; exit 1; }

  local since_filter="."
  if [[ -n "$since" ]]; then
    since_filter="select(.timestamp >= \"$since\")"
  fi

  {
    [[ -n "$MAC_LOG" ]] && jq -c ". + {side:\"mac\"} | $since_filter" "$MAC_LOG"
    [[ -n "$WIN_LOG" ]] && jq -c ". + {side:\"win\"} | $since_filter" "$WIN_LOG"
  } | jq -s -c 'sort_by(.timestamp) | .[]' \
    | tail -n "$lines"
}

cmd_grep() {
  local pattern="$1"; shift || { echo "pattern required" >&2; exit 2; }
  local profile="$DEFAULT_PROFILE" side="both"
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --profile) profile="$2"; shift 2;;
      --side)    side="$2";    shift 2;;
      *) echo "unknown arg: $1" >&2; exit 2;;
    esac
  done
  resolve_pair "$profile"
  if [[ "$side" == "mac" || "$side" == "both" ]] && [[ -n "$MAC_LOG" ]]; then
    echo "===== [mac] $MAC_LOG ====="
    grep -F -- "$pattern" "$MAC_LOG" || true
  fi
  if [[ "$side" == "win" || "$side" == "both" ]] && [[ -n "$WIN_LOG" ]]; then
    echo "===== [win] $WIN_LOG ====="
    grep -F -- "$pattern" "$WIN_LOG" || true
  fi
}

# --- dispatch --------------------------------------------------------------

cmd="${1:-}"; shift || true
case "$cmd" in
  status)        cmd_status "${1:-$DEFAULT_PROFILE}";;
  list-profiles) cmd_list_profiles;;
  paths)
    profile="$DEFAULT_PROFILE"
    [[ "${1:-}" == "--profile" ]] && profile="$2"
    cmd_paths "$profile";;
  tail)          cmd_tail "$@";;
  query)         cmd_query "$@";;
  merge)         cmd_merge "$@";;
  grep)          cmd_grep "$@";;
  -h|--help|help|"") usage;;
  *) echo "unknown command: $cmd" >&2; usage; exit 2;;
esac
